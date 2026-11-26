use crate::domain::key_derivation_policy::KeyDerivationPolicy;
use bdk_wallet::chain::{BlockId, ChainPosition, CheckPoint};
use bdk_wallet::error::CreateTxError;
use bdk_wallet::{AddressInfo, ChangeSet, KeychainKind, SignOptions, Update};
use bip39::Mnemonic;
use bitcoin::bip32::Xpriv;
use bitcoin::{Address, Amount, Block, BlockHash, FeeRate, Network, OutPoint, Transaction, Txid};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletError {
    BdkCreate(String),
    Bip32(bitcoin::bip32::Error),
    ChainUpdate(String),
    InsufficientFunds {
        needed: Amount,
        available: Amount,
        immature: Amount,
    },
    TxBuild(String),
    Signing(String),
    Finalization(String),
}

impl From<bitcoin::bip32::Error> for WalletError {
    fn from(error: bitcoin::bip32::Error) -> Self {
        Self::Bip32(error)
    }
}

impl fmt::Display for WalletError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BdkCreate(error) => write!(formatter, "failed to initialize BDK wallet: {error}"),
            Self::Bip32(error) => write!(formatter, "BIP32 key derivation error: {error}"),
            Self::ChainUpdate(error) => write!(formatter, "failed to update wallet chain: {error}"),
            Self::InsufficientFunds {
                needed,
                available,
                immature,
            } => write!(
                formatter,
                "insufficient funds: needed {needed}, available {available}, immature {immature}"
            ),
            Self::TxBuild(error) => write!(formatter, "failed to build transaction: {error}"),
            Self::Signing(error) => write!(formatter, "failed to sign transaction: {error}"),
            Self::Finalization(error) => {
                write!(formatter, "failed to finalize transaction: {error}")
            }
        }
    }
}

impl std::error::Error for WalletError {}

/// The DNS-Nostr Wallet entity.
///
/// Wraps an in-memory `bdk_wallet::Wallet` for BIP 84 address management, chain sync,
/// balance queries, transaction signing, and staged `ChangeSet` tracking alongside
/// the wallet's `master_key: Xpriv`.
///
/// Master keys and descriptors remain strictly private.
pub struct DnsNostrWallet {
    bdk_wallet: bdk_wallet::Wallet,
    #[allow(dead_code)]
    master_key: Xpriv,
}

impl DnsNostrWallet {
    /// Initializes a new in-memory wallet with BIP 84 descriptors from `KeyDerivationPolicy`,
    /// staging the initial creation `ChangeSet` inside `bdk_wallet`.
    pub fn new(mnemonic: &Mnemonic, network: Network) -> Result<Self, WalletError> {
        const NO_PASSPHRASE: &str = "";
        let seed = mnemonic.to_seed(NO_PASSPHRASE);
        let master_key = Xpriv::new_master(network, &seed)?;

        let bdk_wallet = bdk_wallet::Wallet::create(
            KeyDerivationPolicy::bitcoin_external_descriptor(&master_key),
            KeyDerivationPolicy::bitcoin_internal_descriptor(&master_key),
        )
        .network(network)
        .create_wallet_no_persist()
        .map_err(|error| WalletError::BdkCreate(error.to_string()))?;

        Ok(Self {
            bdk_wallet,
            master_key,
        })
    }

    /// Reconstructs a wallet entity from a `bdk_wallet::Wallet` loaded from a persisted
    /// `ChangeSet` and the `master_key` derived from the stored `Mnemonic`.
    pub fn from_loaded(bdk_wallet: bdk_wallet::Wallet, master_key: Xpriv) -> Self {
        Self {
            bdk_wallet,
            master_key,
        }
    }

    /// Returns the Bitcoin network this wallet is configured for.
    pub fn network(&self) -> Network {
        self.bdk_wallet.network()
    }

    /// Returns the genesis block hash of this wallet's local chain.
    pub fn genesis_hash(&self) -> BlockHash {
        self.bdk_wallet.local_chain().genesis_hash()
    }

    /// Returns the wallet's latest chain checkpoint.
    pub fn latest_checkpoint(&self) -> CheckPoint {
        self.bdk_wallet.latest_checkpoint()
    }

    /// Returns all currently unconfirmed transaction IDs tracked by the wallet.
    pub fn unconfirmed_txids(&self) -> Vec<Txid> {
        self.bdk_wallet
            .transactions()
            .filter(|tx| tx.chain_position.is_unconfirmed())
            .map(|tx| tx.tx_node.txid)
            .collect()
    }

    /// Returns the trusted spendable balance (`confirmed + trusted_pending`).
    pub fn spendable_balance(&self) -> Amount {
        self.bdk_wallet.balance().trusted_spendable()
    }

    /// Reveals the next external BIP 84 receive address and stages the keychain index update.
    pub fn reveal_next_bitcoin_address(&mut self) -> AddressInfo {
        self.bdk_wallet.reveal_next_address(KeychainKind::External)
    }

    /// Anchors the wallet's local chain to the given ascending `BlockId` window without
    /// downloading historical blocks.
    pub fn apply_tip_anchor(&mut self, block_ids: Vec<BlockId>) -> Result<(), WalletError> {
        if block_ids.is_empty() {
            return Ok(());
        }

        let expected_genesis = self.genesis_hash();
        let mut checkpoint = self.bdk_wallet.latest_checkpoint();
        for block_id in block_ids {
            if block_id.height == 0 {
                if block_id.hash != expected_genesis {
                    return Err(WalletError::ChainUpdate(format!(
                        "tip anchor genesis hash ({}) does not match wallet genesis ({expected_genesis})",
                        block_id.hash
                    )));
                }
                continue;
            }
            checkpoint = checkpoint.insert(block_id);
        }

        self.bdk_wallet
            .apply_update(Update {
                chain: Some(checkpoint),
                ..Default::default()
            })
            .map_err(|error| WalletError::ChainUpdate(error.to_string()))
    }

    /// Applies a confirmed block connected to `connected_to` at `height`.
    pub fn apply_block_connected_to(
        &mut self,
        block: &Block,
        height: u32,
        connected_to: BlockId,
    ) -> Result<(), WalletError> {
        self.bdk_wallet
            .apply_block_connected_to(block, height, connected_to)
            .map_err(|error| WalletError::ChainUpdate(error.to_string()))
    }

    /// Records evicted unconfirmed transactions with their eviction Unix timestamps (seconds).
    pub fn apply_evicted_txs(&mut self, evicted: Vec<(Txid, u64)>) {
        self.bdk_wallet.apply_evicted_txs(evicted);
    }

    /// Records a freshly broadcast wallet transaction as unconfirmed at `seen_at` (Unix seconds).
    pub fn record_broadcasted_transaction(&mut self, tx: Transaction, seen_at: u64) {
        self.bdk_wallet.apply_unconfirmed_txs([(tx, seen_at)]);
    }

    /// Builds, signs, and finalizes a transaction paying `amount` to `recipient` at `fee_rate`,
    /// spending only confirmed and wallet-originated `trusted_pending` (change) mature UTXOs.
    pub fn build_and_sign_transaction(
        &mut self,
        recipient: Address,
        amount: Amount,
        fee_rate: FeeRate,
    ) -> Result<Transaction, WalletError> {
        let excluded = self.unspendable_outpoints();

        let mut builder = self.bdk_wallet.build_tx();
        builder
            .add_recipient(recipient.script_pubkey(), amount)
            .fee_rate(fee_rate)
            .unspendable(excluded);

        let mut psbt = builder.finish().map_err(|error| match error {
            CreateTxError::CoinSelection(insufficient) => WalletError::InsufficientFunds {
                needed: insufficient.needed,
                available: insufficient.available,
                immature: self.bdk_wallet.balance().immature,
            },
            other => WalletError::TxBuild(other.to_string()),
        })?;

        let finalized = self
            .bdk_wallet
            .sign(&mut psbt, SignOptions::default())
            .map_err(|error| WalletError::Signing(error.to_string()))?;
        if !finalized {
            return Err(WalletError::Signing(
                "PSBT was not fully finalized after signing".into(),
            ));
        }

        psbt.extract_tx()
            .map_err(|error| WalletError::Finalization(error.to_string()))
    }

    /// Returns an immutable reference to the staged `ChangeSet`, if any, without draining it.
    pub fn staged(&self) -> Option<&ChangeSet> {
        self.bdk_wallet.staged()
    }

    /// Takes and clears the staged `ChangeSet` after a repository transaction commits.
    pub fn take_staged(&mut self) -> Option<ChangeSet> {
        self.bdk_wallet.take_staged()
    }

    fn unspendable_outpoints(&self) -> Vec<OutPoint> {
        const COINBASE_MATURITY: u32 = 100;
        let tip_height = self.bdk_wallet.latest_checkpoint().height();

        self.bdk_wallet
            .list_unspent()
            .filter(|utxo| {
                let is_unconfirmed_external = utxo.keychain == KeychainKind::External
                    && utxo.chain_position.is_unconfirmed();
                let is_immature_coinbase = self
                    .bdk_wallet
                    .get_tx(utxo.outpoint.txid)
                    .is_some_and(|wallet_tx| {
                        if !wallet_tx.tx_node.tx.is_coinbase() {
                            return false;
                        }
                        match utxo.chain_position {
                            ChainPosition::Confirmed { anchor, .. } => {
                                let confirmations = tip_height
                                    .checked_sub(anchor.block_id.height)
                                    .map_or(0, |delta| delta + 1);
                                confirmations < COINBASE_MATURITY
                            }
                            ChainPosition::Unconfirmed { .. } => true,
                        }
                    });
                is_unconfirmed_external || is_immature_coinbase
            })
            .map(|utxo| utxo.outpoint)
            .collect()
    }
}

impl fmt::Debug for DnsNostrWallet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DnsNostrWallet")
            .field("network", &self.bdk_wallet.network())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod test_dns_nostr_wallet {
    use super::*;
    use bdk_wallet::chain::Merge;
    use bitcoin::absolute::LockTime;
    use bitcoin::block::{Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{CompactTarget, ScriptBuf, Sequence, TxIn, TxMerkleNode, TxOut, Witness};

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";

    fn sample_mnemonic() -> Mnemonic {
        Mnemonic::parse(MNEMONIC).unwrap()
    }

    fn make_block(prev_blockhash: BlockHash, nonce: u32, txdata: Vec<Transaction>) -> Block {
        Block {
            header: Header {
                version: BlockVersion::TWO,
                prev_blockhash,
                merkle_root: TxMerkleNode::all_zeros(),
                time: 1_700_000_000 + nonce,
                bits: CompactTarget::from_consensus(0x207f_ffff),
                nonce,
            },
            txdata,
        }
    }

    fn make_funding_tx(recipient: &Address, amount: Amount, prev_tag: u8) -> Transaction {
        Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_byte_array([prev_tag; 32]),
                    vout: 0,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: amount,
                script_pubkey: recipient.script_pubkey(),
            }],
        }
    }

    fn make_coinbase_tx(recipient: &Address, amount: Amount) -> Transaction {
        Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::from_bytes(vec![0x01, 0x01]),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: amount,
                script_pubkey: recipient.script_pubkey(),
            }],
        }
    }

    #[test]
    fn debug_does_not_leak_mnemonic_or_keys() {
        let wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Bitcoin).unwrap();
        let printed = format!("{:?}", wallet);
        assert!(printed.contains("Bitcoin"), "expected network in {printed}");
        assert!(!printed.contains("abandon"), "leaked: {printed}");
        assert!(!printed.contains("about"), "leaked: {printed}");
        assert!(!printed.contains("xprv"), "leaked: {printed}");
        assert!(!printed.contains("xpub"), "leaked: {printed}");
    }

    #[test]
    fn new_stages_initial_changeset_until_take_staged() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Bitcoin).unwrap();
        assert!(wallet.staged().is_some());
        assert!(!wallet.staged().unwrap().is_empty());

        let taken = wallet.take_staged();
        assert!(taken.is_some());
        assert!(wallet.staged().is_none());
    }

    #[test]
    fn reveal_next_bitcoin_address_advances_indices_and_stages_changeset() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Bitcoin).unwrap();
        wallet.take_staged();
        assert!(wallet.staged().is_none());

        let addr0 = wallet.reveal_next_bitcoin_address();
        assert_eq!(addr0.index, 0);
        assert_eq!(addr0.address.to_string(), MAINNET_EXTERNAL_0);
        assert_eq!(addr0.keychain, KeychainKind::External);
        assert!(wallet.staged().is_some());

        wallet.take_staged();
        let addr1 = wallet.reveal_next_bitcoin_address();
        assert_eq!(addr1.index, 1);
        assert_eq!(addr1.address.to_string(), MAINNET_EXTERNAL_1);
        assert!(wallet.staged().is_some());
    }

    #[test]
    fn reveal_next_bitcoin_address_matches_regtest_vector() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let addr = wallet.reveal_next_bitcoin_address();
        assert_eq!(addr.index, 0);
        assert_eq!(addr.address.to_string(), REGTEST_EXTERNAL_0);
    }

    #[test]
    fn from_loaded_preserves_revealed_index() {
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        let _ = wallet.reveal_next_bitcoin_address();
        let changeset = wallet.take_staged().unwrap();

        let loaded_bdk = bdk_wallet::Wallet::load()
            .load_wallet_no_persist(changeset)
            .unwrap()
            .expect("wallet should load from changeset");
        let master_key = Xpriv::new_master(Network::Bitcoin, &mnemonic.to_seed("")).unwrap();
        let mut reloaded = DnsNostrWallet::from_loaded(loaded_bdk, master_key);

        let next = reloaded.reveal_next_bitcoin_address();
        assert_eq!(next.index, 1);
        assert_eq!(next.address.to_string(), MAINNET_EXTERNAL_1);
    }

    #[test]
    fn apply_tip_anchor_handles_empty_zero_short_high_and_genesis_mismatch() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let genesis = wallet.genesis_hash();

        // Empty slice is a no-op.
        wallet.apply_tip_anchor(vec![]).unwrap();
        assert_eq!(wallet.latest_checkpoint().height(), 0);

        // Height-0 only matching genesis.
        wallet
            .apply_tip_anchor(vec![BlockId {
                height: 0,
                hash: genesis,
            }])
            .unwrap();
        assert_eq!(wallet.latest_checkpoint().height(), 0);

        // Short window (< 10 headers).
        let short_window = vec![
            BlockId {
                height: 0,
                hash: genesis,
            },
            BlockId {
                height: 1,
                hash: BlockHash::from_byte_array([1; 32]),
            },
            BlockId {
                height: 2,
                hash: BlockHash::from_byte_array([2; 32]),
            },
        ];
        wallet.apply_tip_anchor(short_window).unwrap();
        assert_eq!(wallet.latest_checkpoint().height(), 2);

        // High window (>= 100).
        let mut high_wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let high_window: Vec<BlockId> = (100..=109)
            .map(|height| BlockId {
                height,
                hash: BlockHash::from_byte_array([height as u8; 32]),
            })
            .collect();
        high_wallet.apply_tip_anchor(high_window).unwrap();
        assert_eq!(high_wallet.latest_checkpoint().height(), 109);

        // Mismatched height-0 genesis returns ChainUpdate error.
        let err = high_wallet
            .apply_tip_anchor(vec![BlockId {
                height: 0,
                hash: BlockHash::from_byte_array([0xFF; 32]),
            }])
            .unwrap_err();
        assert!(matches!(err, WalletError::ChainUpdate(_)));
    }

    #[test]
    fn apply_block_connected_to_syncs_balance_and_handles_reorg_rollback() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let addr = wallet.reveal_next_bitcoin_address().address;
        let genesis = wallet.latest_checkpoint().block_id();

        let tx = make_funding_tx(&addr, Amount::from_sat(150_000), 1);
        let block1 = make_block(genesis.hash, 1, vec![tx]);
        wallet.apply_block_connected_to(&block1, 1, genesis).unwrap();
        assert_eq!(wallet.spendable_balance(), Amount::from_sat(150_000));

        // Reorg: competing chain at height 1 without the funding transaction.
        let reorg_block1 = make_block(genesis.hash, 99, vec![]);
        wallet
            .apply_block_connected_to(&reorg_block1, 1, genesis)
            .unwrap();
        assert_eq!(wallet.spendable_balance(), Amount::ZERO);
    }

    #[test]
    fn build_sign_record_and_evict_unconfirmed_spend_and_spend_trusted_pending_change() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let recv_addr = wallet.reveal_next_bitcoin_address().address;
        let genesis = wallet.latest_checkpoint().block_id();

        // Fund with 1.0 BTC confirmed at height 1.
        let fund_tx = make_funding_tx(&recv_addr, Amount::from_btc(1.0).unwrap(), 7);
        let block1 = make_block(genesis.hash, 1, vec![fund_tx]);
        wallet.apply_block_connected_to(&block1, 1, genesis).unwrap();
        assert_eq!(wallet.spendable_balance(), Amount::from_btc(1.0).unwrap());

        // Spend 0.25 BTC to an external address.
        let recipient = wallet.reveal_next_bitcoin_address().address;
        let other_wallet_mnemonic = Mnemonic::parse(
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
        )
        .unwrap();
        let mut other_wallet =
            DnsNostrWallet::new(&other_wallet_mnemonic, Network::Regtest).unwrap();
        let external_recipient = other_wallet.reveal_next_bitcoin_address().address;

        let fee_rate = FeeRate::from_sat_per_kwu(250);
        let tx1 = wallet
            .build_and_sign_transaction(
                external_recipient.clone(),
                Amount::from_btc(0.25).unwrap(),
                fee_rate,
            )
            .unwrap();
        assert!(!tx1.input[0].witness.is_empty(), "P2WPKH input must be signed");

        let tx1_id = tx1.compute_txid();
        wallet.record_broadcasted_transaction(tx1, 1_700_000_100);
        assert_eq!(wallet.unconfirmed_txids(), vec![tx1_id]);

        // Unconfirmed change is trusted_pending and immediately spendable.
        let after_tx1 = wallet.spendable_balance();
        assert!(after_tx1 > Amount::from_btc(0.74).unwrap());
        assert!(after_tx1 < Amount::from_btc(0.75).unwrap());

        let tx2 = wallet
            .build_and_sign_transaction(
                external_recipient,
                Amount::from_btc(0.1).unwrap(),
                fee_rate,
            )
            .unwrap();
        assert!(!tx2.input[0].witness.is_empty());

        // Evicting tx1 restores the original 1.0 BTC balance.
        wallet.apply_evicted_txs(vec![(tx1_id, 1_700_000_200)]);
        assert!(wallet.unconfirmed_txids().is_empty());
        assert_eq!(wallet.spendable_balance(), Amount::from_btc(1.0).unwrap());

        // Silence unused binding warning if any.
        let _ = recipient;
    }

    #[test]
    fn immature_coinbase_is_excluded_from_spendable_balance_and_coin_selection() {
        let mut wallet = DnsNostrWallet::new(&sample_mnemonic(), Network::Regtest).unwrap();
        let addr = wallet.reveal_next_bitcoin_address().address;
        let genesis = wallet.latest_checkpoint().block_id();

        let coinbase = make_coinbase_tx(&addr, Amount::from_btc(50.0).unwrap());
        let block1 = make_block(genesis.hash, 1, vec![coinbase]);
        wallet.apply_block_connected_to(&block1, 1, genesis).unwrap();

        // At height 1, coinbase has 1 confirmation (< 100) and is immature.
        assert_eq!(wallet.spendable_balance(), Amount::ZERO);

        let err = wallet
            .build_and_sign_transaction(
                addr,
                Amount::from_btc(1.0).unwrap(),
                FeeRate::from_sat_per_kwu(250),
            )
            .unwrap_err();
        match err {
            WalletError::InsufficientFunds {
                available,
                immature,
                ..
            } => {
                assert_eq!(available, Amount::ZERO);
                assert_eq!(immature, Amount::from_btc(50.0).unwrap());
            }
            other => panic!("expected InsufficientFunds, got {other:?}"),
        }
    }
}
