use crate::{
    label::Label,
    name_token::NameToken,
    name_token_repository::NameTokenRepository,
    position::Position,
};
use bdk_chain::{local_chain::LocalChain, BlockId, CheckPoint};
use bitcoin::{Block, OutPoint, Transaction, TxIn, TxOut};
use std::{collections::HashMap, sync::Mutex};

/// Name-Tokens changed earlier in the block being applied, keyed by their first outpoint.
type PendingBlockUpdates = HashMap<OutPoint, NameToken>;

pub struct NameTokenService<R: NameTokenRepository> {
    repository: R,
    chain: Mutex<Option<LocalChain>>,
}

impl<R: NameTokenRepository> NameTokenService<R> {
    pub async fn new(repository: R) -> Self {
        let chain_points = repository.get_chain().await;
        let mut chain = None;
        if !chain_points.is_empty() {
            let mut tree = std::collections::BTreeMap::new();
            for point in chain_points {
                tree.insert(point.height, point.hash);
            }
            if let Ok(c) = LocalChain::from_blocks(tree) {
                chain = Some(c);
            }
        }
        Self {
            repository,
            chain: Mutex::new(chain),
        }
    }

    /// Applies the Name-Token changes of `block`, handling chain reorganizations if necessary.
    pub async fn apply_block_connected_to(
        &self,
        block: &Block,
        height: u32,
        connected_to: BlockId,
    ) {
        let mut blocks_to_disconnect = Vec::new();
        {
            let mut chain_opt = self.chain.lock().unwrap();
            if let Some(chain) = chain_opt.as_mut() {
                let current_tip = chain.tip().block_id();
                if current_tip != connected_to {
                    blocks_to_disconnect = chain
                        .iter_checkpoints()
                        .take_while(|cp| cp.height() > connected_to.height)
                        .map(|cp| cp.block_id())
                        .collect();
                }
            }
        }

        for block_id in blocks_to_disconnect {
            let mut affected_tokens = self.repository.get_name_tokens_by_block(block_id).await;
            for token in &mut affected_tokens {
                token.undo_block_events(block_id);
            }
            let updated_tokens: Vec<_> = affected_tokens.into_iter().filter(|t| t.has_minted()).collect();
            self.repository.undo_block_updates(block_id, &updated_tokens).await;
        }

        {
            let mut chain_opt = self.chain.lock().unwrap();
            if let Some(chain) = chain_opt.as_mut() {
                if let Some(cp) = chain.get(connected_to.height + 1) {
                    let _ = chain.disconnect_from(cp.block_id());
                }
            }
        }

        let block_hash = block.block_hash();
        let block_id = BlockId { height, hash: block_hash };
        let mut pending_block_updates = PendingBlockUpdates::new();
        for (blockindex, transaction) in block.txdata.iter().enumerate() {
            self.apply_transaction(
                transaction,
                blockindex,
                block_id,
                &mut pending_block_updates,
            )
            .await;
        }
        let updates: Vec<NameToken> = pending_block_updates.into_values().collect();

        self.repository
            .save_block_updates(block_hash, height, &updates)
            .await;

        {
            let mut chain_opt = self.chain.lock().unwrap();
            if chain_opt.is_none() && height == 0 {
                let (new_chain, _) = LocalChain::from_genesis_hash(block_hash);
                *chain_opt = Some(new_chain);
            } else if let Some(chain) = chain_opt.as_mut() {
                let _ = chain.insert_block(block_id);
            }
        }
    }

    /// Returns the Valid Name-Token of `label` according to the First Confirmed Rule.
    pub async fn get_name_token(&self, label_bytes: &[u8]) -> Option<NameToken> {
        let label = Label::from(label_bytes);
        let name_tokens_with_label = self.repository.get_name_tokens_by_label(&label).await;
        NameToken::select_root_token(&label, &name_tokens_with_label).cloned()
    }

    pub fn tip(&self) -> Option<CheckPoint> {
        let chain_opt = self.chain.lock().unwrap();
        chain_opt.as_ref().map(|chain| chain.tip())
    }

    async fn apply_transaction(
        &self,
        transaction: &Transaction,
        blockindex: usize,
        block_id: BlockId,
        pending_block_updates: &mut PendingBlockUpdates,
    ) {
        let num_positional_correlations =
            usize::max(transaction.input.len(), transaction.output.len());
        for same_index in 0..num_positional_correlations {
            let position = Position::new(
                block_id,
                blockindex,
                OutPoint {
                    txid: transaction.compute_txid(),
                    vout: same_index as u32,
                },
            );
            self.apply_same_index_chain(
                transaction.input.get(same_index),
                transaction.output.get(same_index),
                position,
                pending_block_updates,
            )
            .await;
        }
    }

    async fn apply_same_index_chain(
        &self,
        txin: Option<&TxIn>,
        txout: Option<&TxOut>,
        position: Position,
        pending_block_updates: &mut PendingBlockUpdates,
    ) {
        let input_name_token = match txin {
            None => None,
            Some(txin) => {
                self.find_name_token_by_outpoint(txin.previous_output, pending_block_updates)
                    .await
            }
        };
        let updated_name_tokens =
            NameToken::process_same_index_chain(input_name_token.as_ref(), txout, &position);
        for updated_name_token in updated_name_tokens {
            let first_outpoint = updated_name_token.first_position().outpoint();
            pending_block_updates.insert(first_outpoint, updated_name_token);
        }
    }

    async fn find_name_token_by_outpoint(
        &self,
        outpoint: OutPoint,
        pending_block_updates: &PendingBlockUpdates,
    ) -> Option<NameToken> {
        match pending_block_updates.values().find(|nt| nt.last_outpoint() == outpoint) {
            Some(name_token) => Some(name_token.clone()),
            None => self.repository.get_name_token_by_outpoint(outpoint).await,
        }
    }
}

#[cfg(test)]
mod test_name_token_service {
    use super::*;
    use bitcoin::{
        absolute::LockTime,
        block,
        hashes::Hash,
        opcodes::{
            all::{OP_ENDIF, OP_IF, OP_NOP},
            OP_FALSE,
        },
        script::{Builder, PushBytesBuf},
        transaction, Amount, BlockHash, CompactTarget, OutPoint, ScriptBuf, Sequence, Transaction,
        TxIn, TxMerkleNode, TxOut, Txid, Witness,
    };
    use crate::position::Position;



    async fn new_service() -> NameTokenService<crate::InMemoryNameTokenRepository> {
        NameTokenService::new(crate::InMemoryNameTokenRepository::default()).await
    }

    fn inscription_script(label: &[u8]) -> ScriptBuf {
        let label = PushBytesBuf::try_from(label.to_vec()).unwrap();
        Builder::default()
            .push_opcode(OP_FALSE)
            .push_opcode(OP_IF)
            .push_slice(b"name")
            .push_slice(&label)
            .push_opcode(OP_NOP)
            .push_slice(b"protocol")
            .push_slice(b"argument")
            .push_opcode(OP_ENDIF)
            .into_script()
    }

    fn inscribed_output(label: &[u8]) -> TxOut {
        TxOut {
            value: Amount::from_sat(546),
            script_pubkey: inscription_script(label),
        }
    }

    fn plain_output() -> TxOut {
        TxOut {
            value: Amount::from_sat(546),
            script_pubkey: ScriptBuf::new(),
        }
    }

    fn funding_outpoint(seed: u8) -> OutPoint {
        OutPoint {
            txid: Txid::from_byte_array([seed; 32]),
            vout: 0,
        }
    }

    fn transaction(spent_outpoints: Vec<OutPoint>, outputs: Vec<TxOut>) -> Transaction {
        Transaction {
            version: transaction::Version::ONE,
            lock_time: LockTime::ZERO,
            input: spent_outpoints
                .into_iter()
                .map(|previous_output| TxIn {
                    previous_output,
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                })
                .collect(),
            output: outputs,
        }
    }

    fn block(transactions: Vec<Transaction>, hash: u8) -> Block {
        Block {
            header: block::Header {
                version: block::Version::ONE,
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: CompactTarget::from_consensus(0),
                nonce: hash as u32,
            },
            txdata: transactions,
        }
    }

    fn position(
        blockheight: u32,
        blockhash: BlockHash,
        blockindex: usize,
        vout: u32,
        transaction: &Transaction,
    ) -> Position {
        Position::new(
            BlockId { height: blockheight, hash: blockhash },
            blockindex,
            OutPoint { txid: transaction.compute_txid(), vout },
        )
    }

    fn label() -> Vec<u8> {
        b"label".to_vec()
    }

    #[tokio::test]
    async fn should_create_name_token_when_output_is_inscribed() {
        let service = new_service().await;
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![creation.clone()], 10);

        service
            .apply_block_connected_to(
                &b10,
                10,
                BlockId {
                    height: 9,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(name_token.first_position(), &position(10, b10.block_hash(), 0, 0, &creation));
        assert_eq!(name_token.last_position(), &position(10, b10.block_hash(), 0, 0, &creation));
    }

    #[tokio::test]
    async fn should_not_create_name_token_when_output_is_not_inscribed() {
        let service = new_service().await;
        let payment = transaction(vec![funding_outpoint(1)], vec![plain_output()]);
        let b10 = block(vec![payment], 10);

        service
            .apply_block_connected_to(
                &b10,
                10,
                BlockId {
                    height: 9,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        assert_eq!(service.get_name_token(&label()).await, None);
    }

    #[tokio::test]
    async fn should_advance_next_block_height_after_applying_a_block() {
        let service = new_service().await;
        assert_eq!(service.tip(), None);
        let b10 = block(vec![], 10);

        service
            .apply_block_connected_to(
                &b10,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        assert_eq!(service.tip().unwrap().height(), 0);
    }

    #[tokio::test]
    async fn should_update_name_token_when_spent_and_reinscribed_with_same_label() {
        let service = new_service().await;
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![creation.clone()], 10);
        service
            .apply_block_connected_to(
                &b10,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;
        let update = transaction(
            vec![OutPoint {
                txid: creation.compute_txid(),
                vout: 0,
            }],
            vec![inscribed_output(b"label")],
        );
        let b11 = block(vec![update.clone()], 11);

        service
            .apply_block_connected_to(&b11, 1, service.tip().unwrap().block_id())
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(name_token.first_position(), &position(0, b10.block_hash(), 0, 0, &creation));
        assert_eq!(name_token.last_position(), &position(1, b11.block_hash(), 0, 0, &update));
    }

    #[tokio::test]
    async fn should_revoke_old_label_and_create_new_one_when_reinscribed_with_other_label() {
        let service = new_service().await;
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![creation.clone()], 10);
        service
            .apply_block_connected_to(
                &b10,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;
        let relabel = transaction(
            vec![OutPoint {
                txid: creation.compute_txid(),
                vout: 0,
            }],
            vec![inscribed_output(b"other")],
        );
        let b11 = block(vec![relabel.clone()], 11);

        service
            .apply_block_connected_to(&b11, 1, service.tip().unwrap().block_id())
            .await;

        assert_eq!(service.get_name_token(&label()).await, None);
        let other = service.get_name_token(b"other").await.unwrap();
        assert_eq!(other.first_position(), &position(1, b11.block_hash(), 0, 0, &relabel));
    }

    #[tokio::test]
    async fn should_select_earliest_inscription_when_labels_conflict() {
        let service = new_service().await;
        let first = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let second = transaction(vec![funding_outpoint(2)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![first.clone(), second], 10);

        service
            .apply_block_connected_to(
                &b10,
                10,
                BlockId {
                    height: 9,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(name_token.first_position(), &position(10, b10.block_hash(), 0, 0, &first));
    }

    #[tokio::test]
    async fn should_undo_block_events_on_reorg() {
        let service = new_service().await;
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![creation.clone()], 10);
        service
            .apply_block_connected_to(
                &b10,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(name_token.first_position(), &position(0, b10.block_hash(), 0, 0, &creation));

        // Emulate reorg where block 1 replaces block 1
        let alt_creation = transaction(vec![funding_outpoint(2)], vec![inscribed_output(b"alt")]);

        let service2 = new_service().await;
        let b1 = block(vec![creation.clone()], 1);
        service2
            .apply_block_connected_to(
                &b1,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;
        let b2 = block(vec![alt_creation.clone()], 2);
        service2
            .apply_block_connected_to(&b2, 1, service2.tip().unwrap().block_id())
            .await;

        // Reorg b2 out by submitting b2_alt connected to b1
        let update_tx = transaction(
            vec![OutPoint {
                txid: b1.txdata[0].compute_txid(),
                vout: 0,
            }],
            vec![inscribed_output(b"label")],
        );
        let b2_alt = block(vec![update_tx.clone()], 3);
        service2
            .apply_block_connected_to(
                &b2_alt,
                1,
                service2.tip().unwrap().prev().unwrap().block_id(),
            )
            .await;

        // "alt" label should no longer exist
        assert_eq!(service2.get_name_token(b"alt").await, None);

        // "label" should have been updated by b2_alt
        let name_token = service2.get_name_token(&label()).await.unwrap();
        // first position is 0, last position is 1
        assert_eq!(
            name_token.last_position(),
            &position(1, b2_alt.block_hash(), 0, 0, &b2_alt.txdata[0])
        );
    }

    #[tokio::test]
    async fn should_revoke_name_token_when_spent_without_reinscription() {
        let service = new_service().await;
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let b10 = block(vec![creation.clone()], 10);
        service
            .apply_block_connected_to(
                &b10,
                0,
                BlockId {
                    height: 0,
                    hash: BlockHash::all_zeros(),
                },
            )
            .await;

        let payment = transaction(
            vec![OutPoint {
                txid: creation.compute_txid(),
                vout: 0,
            }],
            vec![plain_output()],
        );
        let b11 = block(vec![payment.clone()], 11);

        service
            .apply_block_connected_to(&b11, 1, service.tip().unwrap().block_id())
            .await;

        let name_token = service.get_name_token(&label()).await;
        assert_eq!(name_token, None);
    }
}
