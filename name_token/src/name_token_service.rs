use crate::{
    inscription::{Bytes, NameTokenPosition},
    name_token::NameToken,
    name_token_repository::NameTokenRepository,
};
use bitcoin::{Block, OutPoint, Transaction, TxIn, TxOut};
use std::collections::HashMap;

/// Name-Tokens changed earlier in the block being applied, keyed by their last outpoint.
type PendingBlockUpdates = HashMap<OutPoint, NameToken>;

pub struct NameTokenService<R: NameTokenRepository> {
    repository: R,
}

impl<R: NameTokenRepository> NameTokenService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Applies the Name-Token changes of `block` and advances the next block height.
    pub async fn apply_block(&self, blockheight: u64, block: &Block) {
        let mut pending_block_updates = PendingBlockUpdates::new();
        for (blockindex, transaction) in block.txdata.iter().enumerate() {
            self.apply_transaction(
                transaction,
                blockindex,
                blockheight,
                &mut pending_block_updates,
            )
            .await;
        }
        let updates: Vec<NameToken> = pending_block_updates.into_values().collect();
        self.repository
            .save_block_updates(blockheight, &updates)
            .await;
    }

    /// Returns the Valid Name-Token of `label` according to the First Confirmed Rule.
    pub async fn get_name_token(&self, label: &Bytes) -> Option<NameToken> {
        let name_tokens_with_label = self.repository.get_name_tokens_by_label(label).await;
        NameToken::select_root_token(label, &name_tokens_with_label).cloned()
    }

    pub async fn next_block_height(&self) -> u64 {
        self.repository.get_next_block_height().await
    }

    async fn apply_transaction(
        &self,
        transaction: &Transaction,
        blockindex: usize,
        blockheight: u64,
        pending_block_updates: &mut PendingBlockUpdates,
    ) {
        let num_positional_correlations =
            usize::max(transaction.input.len(), transaction.output.len());
        for same_index in 0..num_positional_correlations {
            let position = NameTokenPosition {
                txid: transaction.compute_txid(),
                vout: same_index as u32,
                blockheight,
                blockindex,
            };
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
        position: NameTokenPosition,
        pending_block_updates: &mut PendingBlockUpdates,
    ) {
        let input_name_token = match txin {
            None => None,
            Some(txin) => {
                self.find_name_token_by_outpoint(txin.previous_output, pending_block_updates)
                    .await
            }
        };
        let updated_name_tokens = NameToken::process_same_index_chain(
            input_name_token.as_ref(),
            txout,
            &position,
        );
        for updated_name_token in updated_name_tokens {
            pending_block_updates.insert(updated_name_token.last_outpoint(), updated_name_token);
        }
    }

    async fn find_name_token_by_outpoint(
        &self,
        outpoint: OutPoint,
        pending_block_updates: &PendingBlockUpdates,
    ) -> Option<NameToken> {
        match pending_block_updates.get(&outpoint) {
            Some(name_token) => Some(name_token.clone()),
            None => self.repository.get_name_token_by_outpoint(outpoint).await,
        }
    }
}

#[cfg(test)]
mod test_name_token_service {
    use super::*;
    use async_trait::async_trait;
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
    use std::sync::Mutex;

    use crate::inscription::NameTokenPosition;

    #[derive(Default)]
    struct InMemoryNameTokenRepository {
        state: Mutex<InMemoryState>,
    }

    #[derive(Default)]
    struct InMemoryState {
        next_block_height: u64,
        name_tokens: Vec<NameToken>,
    }

    #[async_trait]
    impl NameTokenRepository for InMemoryNameTokenRepository {
        async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken> {
            let state = self.state.lock().unwrap();
            state
                .name_tokens
                .iter()
                .find(|name_token| name_token.last_outpoint() == outpoint)
                .cloned()
        }

        async fn get_name_tokens_by_label(&self, label: &Bytes) -> Vec<NameToken> {
            let state = self.state.lock().unwrap();
            state
                .name_tokens
                .iter()
                .filter(|name_token| &name_token.label == label)
                .cloned()
                .collect()
        }

        async fn save_block_updates(&self, blockheight: u64, updated_name_tokens: &[NameToken]) {
            let mut state = self.state.lock().unwrap();
            state.next_block_height = blockheight + 1;
            for updated in updated_name_tokens {
                state.name_tokens.retain(|stored| {
                    stored.first_position != updated.first_position
                });
                if !updated.is_revoked() {
                    state.name_tokens.push(updated.clone());
                }
            }
        }

        async fn get_next_block_height(&self) -> u64 {
            self.state.lock().unwrap().next_block_height
        }
    }

    fn new_service() -> NameTokenService<InMemoryNameTokenRepository> {
        NameTokenService::new(InMemoryNameTokenRepository::default())
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

    fn block(transactions: Vec<Transaction>) -> Block {
        Block {
            header: block::Header {
                version: block::Version::ONE,
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: CompactTarget::from_consensus(0),
                nonce: 0,
            },
            txdata: transactions,
        }
    }

    fn position(
        blockheight: u64,
        blockindex: usize,
        vout: u32,
        transaction: &Transaction,
    ) -> NameTokenPosition {
        NameTokenPosition {
            blockheight,
            blockindex,
            vout,
            txid: transaction.compute_txid(),
        }
    }

    fn label() -> Bytes {
        b"label".to_vec()
    }

    #[tokio::test]
    async fn should_create_name_token_when_output_is_inscribed() {
        let service = new_service();
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);

        service
            .apply_block(10, &block(vec![creation.clone()]))
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(
            name_token.first_position,
            position(10, 0, 0, &creation)
        );
        assert_eq!(
            name_token.last_position,
            position(10, 0, 0, &creation)
        );
    }

    #[tokio::test]
    async fn should_not_create_name_token_when_output_is_not_inscribed() {
        let service = new_service();
        let payment = transaction(vec![funding_outpoint(1)], vec![plain_output()]);

        service.apply_block(10, &block(vec![payment])).await;

        assert_eq!(service.get_name_token(&label()).await, None);
    }

    #[tokio::test]
    async fn should_advance_next_block_height_after_applying_a_block() {
        let service = new_service();
        assert_eq!(service.next_block_height().await, 0);

        service.apply_block(10, &block(vec![])).await;

        assert_eq!(service.next_block_height().await, 11);
    }

    #[tokio::test]
    async fn should_update_name_token_when_spent_and_reinscribed_with_same_label() {
        let service = new_service();
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        service
            .apply_block(10, &block(vec![creation.clone()]))
            .await;
        let update = transaction(
            vec![OutPoint {
                txid: creation.compute_txid(),
                vout: 0,
            }],
            vec![inscribed_output(b"label")],
        );

        service.apply_block(11, &block(vec![update.clone()])).await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(
            name_token.first_position,
            position(10, 0, 0, &creation)
        );
        assert_eq!(
            name_token.last_position,
            position(11, 0, 0, &update)
        );
    }

    #[tokio::test]
    async fn should_revoke_old_label_and_create_new_one_when_reinscribed_with_other_label() {
        let service = new_service();
        let creation = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        service
            .apply_block(10, &block(vec![creation.clone()]))
            .await;
        let relabel = transaction(
            vec![OutPoint {
                txid: creation.compute_txid(),
                vout: 0,
            }],
            vec![inscribed_output(b"other")],
        );

        service.apply_block(11, &block(vec![relabel.clone()])).await;

        assert_eq!(service.get_name_token(&label()).await, None);
        let other = service.get_name_token(&b"other".to_vec()).await.unwrap();
        assert_eq!(
            other.first_position,
            position(11, 0, 0, &relabel)
        );
    }

    #[tokio::test]
    async fn should_select_earliest_inscription_when_labels_conflict() {
        let service = new_service();
        let first = transaction(vec![funding_outpoint(1)], vec![inscribed_output(b"label")]);
        let second = transaction(vec![funding_outpoint(2)], vec![inscribed_output(b"label")]);

        service
            .apply_block(10, &block(vec![first.clone(), second]))
            .await;

        let name_token = service.get_name_token(&label()).await.unwrap();
        assert_eq!(
            name_token.first_position,
            position(10, 0, 0, &first)
        );
    }
}
