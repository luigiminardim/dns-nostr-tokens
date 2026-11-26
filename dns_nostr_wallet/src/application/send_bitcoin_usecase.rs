use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::dns_nostr_wallet::WalletError;
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use crate::domain::sync_service::{SyncService, SyncServiceError};
use bitcoin::{Address, Amount, Denomination, Network, Txid};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Use case: validate recipient and amount, sync the wallet, estimate fee rate, build/sign/broadcast
/// a transaction, and persist both pre-broadcast sync state and post-broadcast spend state.
pub struct SendBitcoinUsecase {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
    blockchain: Arc<dyn BlockchainPort>,
    sync_service: Arc<SyncService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendBitcoinUsecaseError {
    InvalidAddress(String),
    AddressNetworkMismatch {
        expected: Network,
    },
    InvalidAmount(String),
    DustAmount {
        amount: Amount,
        minimum: Amount,
    },
    WalletNotFound,
    NetworkMismatch {
        configured: Network,
        stored: Network,
    },
    InsufficientFunds {
        needed: Amount,
        available: Amount,
        immature: Amount,
    },
    Blockchain(BlockchainError),
    Sync(SyncServiceError),
    Wallet(String),
    BroadcastSucceededButPersistFailed {
        txid: Txid,
        reason: String,
    },
}

impl SendBitcoinUsecase {
    pub fn new(
        network: Network,
        repository: Arc<dyn NameTokenWalletRepository>,
        blockchain: Arc<dyn BlockchainPort>,
        sync_service: Arc<SyncService>,
    ) -> Self {
        Self {
            network,
            repository,
            blockchain,
            sync_service,
        }
    }

    pub fn execute(&self, address: &str, amount_btc: &str) -> Result<Txid, SendBitcoinUsecaseError> {
        let recipient = self.parse_recipient(address)?;
        let amount = Self::parse_amount(amount_btc, &recipient)?;

        let mut wallet = self
            .repository
            .load()?
            .ok_or(SendBitcoinUsecaseError::WalletNotFound)?;
        if wallet.network() != self.network {
            return Err(SendBitcoinUsecaseError::NetworkMismatch {
                configured: self.network,
                stored: wallet.network(),
            });
        }

        self.sync_service.sync(&mut wallet, &*self.blockchain)?;
        // Pre-broadcast save preserves sync progress before fee estimation / tx build / broadcast.
        self.repository.save(&mut wallet)?;

        let fee_rate = self.blockchain.estimate_fee_rate()?;
        let tx = wallet.build_and_sign_transaction(recipient, amount, fee_rate)?;
        let txid = self.blockchain.broadcast(&tx)?;

        wallet.record_broadcasted_transaction(tx, Self::unix_timestamp_now());
        self.repository.save(&mut wallet).map_err(|error| {
            SendBitcoinUsecaseError::BroadcastSucceededButPersistFailed {
                txid,
                reason: error.to_string(),
            }
        })?;
        Ok(txid)
    }

    fn parse_recipient(&self, address: &str) -> Result<Address, SendBitcoinUsecaseError> {
        let unchecked = Address::from_str(address)
            .map_err(|error| SendBitcoinUsecaseError::InvalidAddress(error.to_string()))?;
        unchecked
            .require_network(self.network)
            .map_err(|_| SendBitcoinUsecaseError::AddressNetworkMismatch {
                expected: self.network,
            })
    }

    fn parse_amount(
        amount_btc: &str,
        recipient: &Address,
    ) -> Result<Amount, SendBitcoinUsecaseError> {
        let amount = Amount::from_str_in(amount_btc, Denomination::Bitcoin)
            .map_err(|error| SendBitcoinUsecaseError::InvalidAmount(error.to_string()))?;
        if amount == Amount::ZERO {
            return Err(SendBitcoinUsecaseError::InvalidAmount(
                "amount must be greater than zero".into(),
            ));
        }
        let minimum = recipient.script_pubkey().minimal_non_dust();
        if amount < minimum {
            return Err(SendBitcoinUsecaseError::DustAmount { amount, minimum });
        }
        Ok(amount)
    }

    fn unix_timestamp_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl From<BlockchainError> for SendBitcoinUsecaseError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<SyncServiceError> for SendBitcoinUsecaseError {
    fn from(error: SyncServiceError) -> Self {
        Self::Sync(error)
    }
}

impl From<WalletRepositoryError> for SendBitcoinUsecaseError {
    fn from(error: WalletRepositoryError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl From<WalletError> for SendBitcoinUsecaseError {
    fn from(error: WalletError) -> Self {
        match error {
            WalletError::InsufficientFunds {
                needed,
                available,
                immature,
            } => Self::InsufficientFunds {
                needed,
                available,
                immature,
            },
            other => Self::Wallet(other.to_string()),
        }
    }
}

impl fmt::Display for SendBitcoinUsecaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress(reason) => write!(formatter, "invalid recipient address: {reason}"),
            Self::AddressNetworkMismatch { expected } => write!(
                formatter,
                "recipient address does not belong to configured network ({expected})"
            ),
            Self::InvalidAmount(reason) => write!(formatter, "invalid BTC amount: {reason}"),
            Self::DustAmount { amount, minimum } => write!(
                formatter,
                "amount ({amount}) is below dust threshold ({minimum})"
            ),
            Self::WalletNotFound => write!(
                formatter,
                "no wallet found; run `wallet create` or `wallet import` first"
            ),
            Self::NetworkMismatch { configured, stored } => write!(
                formatter,
                "configured network ({configured}) does not match stored wallet network ({stored})"
            ),
            Self::InsufficientFunds {
                needed,
                available,
                immature,
            } => write!(
                formatter,
                "insufficient funds: needed {needed}, available {available}, immature {immature}"
            ),
            Self::Blockchain(error) => write!(formatter, "{error}"),
            Self::Sync(error) => write!(formatter, "{error}"),
            Self::Wallet(error) => write!(formatter, "{error}"),
            Self::BroadcastSucceededButPersistFailed { txid, reason } => write!(
                formatter,
                "transaction {txid} was broadcast, but persisting local wallet state failed: {reason}"
            ),
        }
    }
}

impl std::error::Error for SendBitcoinUsecaseError {}

#[cfg(test)]
mod test_send_bitcoin_usecase {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::blockchain_port::FakeBlockchain;
    use crate::domain::dns_nostr_wallet::DnsNostrWallet;
    use bip39::Mnemonic;
    use bitcoin::absolute::LockTime;
    use bitcoin::block::{Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{
        Block, CompactTarget, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxMerkleNode,
        TxOut, Witness,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SENDER_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                                   abandon abandon abandon abandon abandon about";
    const RECIPIENT_MNEMONIC: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const MAINNET_ADDRESS: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";

    struct FailOnNthSaveRepository {
        inner: Arc<dyn NameTokenWalletRepository>,
        fail_on_call: usize,
        save_calls: AtomicUsize,
    }

    impl FailOnNthSaveRepository {
        fn new(inner: Arc<dyn NameTokenWalletRepository>, fail_on_call: usize) -> Self {
            Self {
                inner,
                fail_on_call,
                save_calls: AtomicUsize::new(0),
            }
        }
    }

    impl NameTokenWalletRepository for FailOnNthSaveRepository {
        fn create(
            &self,
            mnemonic: &Mnemonic,
            wallet: &mut DnsNostrWallet,
        ) -> Result<(), WalletRepositoryError> {
            self.inner.create(mnemonic, wallet)
        }

        fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError> {
            let call = self.save_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call == self.fail_on_call {
                return Err(WalletRepositoryError::Storage("simulated disk full".into()));
            }
            self.inner.save(wallet)
        }

        fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError> {
            self.inner.load()
        }
    }

    fn recipient_address(network: Network) -> String {
        let mnemonic = Mnemonic::parse(RECIPIENT_MNEMONIC).unwrap();
        let mut wallet = DnsNostrWallet::new(&mnemonic, network).unwrap();
        wallet.reveal_next_bitcoin_address().address.to_string()
    }

    fn init_funded_wallet(
        amount: Amount,
        coinbase: bool,
    ) -> Arc<dyn NameTokenWalletRepository> {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = Mnemonic::parse(SENDER_MNEMONIC).unwrap();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Regtest).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        let addr = wallet.reveal_next_bitcoin_address().address;
        let genesis = wallet.latest_checkpoint().block_id();
        let tx = if coinbase {
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
                    script_pubkey: addr.script_pubkey(),
                }],
            }
        } else {
            Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint {
                        txid: Txid::from_byte_array([0x22; 32]),
                        vout: 0,
                    },
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![TxOut {
                    value: amount,
                    script_pubkey: addr.script_pubkey(),
                }],
            }
        };
        let block = Block {
            header: Header {
                version: BlockVersion::TWO,
                prev_blockhash: genesis.hash,
                merkle_root: TxMerkleNode::all_zeros(),
                time: 1_700_000_001,
                bits: CompactTarget::from_consensus(0x207f_ffff),
                nonce: 1,
            },
            txdata: vec![tx],
        };
        wallet.apply_block_connected_to(&block, 1, genesis).unwrap();
        repo.save(&mut wallet).unwrap();
        Arc::new(repo)
    }

    #[test]
    fn validates_address_network_amount_and_dust_with_zero_rpc_calls() {
        let repo: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let blockchain = Arc::new(FakeBlockchain::new());
        let use_case = SendBitcoinUsecase::new(
            Network::Regtest,
            repo,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        );
        let valid_regtest_addr = recipient_address(Network::Regtest);

        assert!(matches!(
            use_case.execute("not-an-address", "0.1"),
            Err(SendBitcoinUsecaseError::InvalidAddress(_))
        ));
        assert_eq!(
            use_case.execute(MAINNET_ADDRESS, "0.1"),
            Err(SendBitcoinUsecaseError::AddressNetworkMismatch {
                expected: Network::Regtest
            })
        );
        for bad_amount in ["0", "-0.1", "0.000000001", "abc", ""] {
            assert!(
                matches!(
                    use_case.execute(&valid_regtest_addr, bad_amount),
                    Err(SendBitcoinUsecaseError::InvalidAmount(_))
                ),
                "expected InvalidAmount for `{bad_amount}`"
            );
        }
        assert!(matches!(
            use_case.execute(&valid_regtest_addr, "0.00000293"),
            Err(SendBitcoinUsecaseError::DustAmount { .. })
        ));
        assert_eq!(blockchain.sync_calls(), 0);
        assert_eq!(blockchain.estimate_fee_rate_calls(), 0);
        assert_eq!(blockchain.broadcast_calls(), 0);
    }

    #[test]
    fn fails_without_wallet_or_on_network_mismatch() {
        let valid_regtest_addr = recipient_address(Network::Regtest);
        let empty_repo: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let blockchain = Arc::new(FakeBlockchain::new());
        let err = SendBitcoinUsecase::new(
            Network::Regtest,
            empty_repo,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute(&valid_regtest_addr, "0.1")
        .unwrap_err();
        assert_eq!(err, SendBitcoinUsecaseError::WalletNotFound);

        let regtest_repo = init_funded_wallet(Amount::from_btc(1.0).unwrap(), false);
        let err = SendBitcoinUsecase::new(
            Network::Bitcoin,
            regtest_repo,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute(MAINNET_ADDRESS, "0.1")
        .unwrap_err();
        assert_eq!(
            err,
            SendBitcoinUsecaseError::NetworkMismatch {
                configured: Network::Bitcoin,
                stored: Network::Regtest,
            }
        );
        assert_eq!(blockchain.sync_calls(), 0);
    }

    #[test]
    fn insufficient_funds_reports_immature_coinbase_and_preserves_pre_broadcast_sync() {
        let repo = init_funded_wallet(Amount::from_btc(50.0).unwrap(), true);
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_hook(|wallet| {
            let tip = wallet.latest_checkpoint().block_id();
            let block2 = Block {
                header: Header {
                    version: BlockVersion::TWO,
                    prev_blockhash: tip.hash,
                    merkle_root: TxMerkleNode::all_zeros(),
                    time: 1_700_000_002,
                    bits: CompactTarget::from_consensus(0x207f_ffff),
                    nonce: 2,
                },
                txdata: vec![],
            };
            wallet
                .apply_block_connected_to(&block2, 2, tip)
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))
        });

        let err = SendBitcoinUsecase::new(
            Network::Regtest,
            Arc::clone(&repo),
            blockchain,
            Arc::new(SyncService),
        )
        .execute(&recipient_address(Network::Regtest), "1.0")
        .unwrap_err();
        match err {
            SendBitcoinUsecaseError::InsufficientFunds {
                available,
                immature,
                ..
            } => {
                assert_eq!(available, Amount::ZERO);
                assert_eq!(immature, Amount::from_btc(50.0).unwrap());
            }
            other => panic!("expected InsufficientFunds, got {other:?}"),
        }

        // Pre-broadcast sync height 2 was persisted.
        let reloaded = repo.load().unwrap().unwrap();
        assert_eq!(reloaded.latest_checkpoint().height(), 2);
    }

    #[test]
    fn broadcast_failure_preserves_sync_and_discards_uncommitted_change() {
        let repo = init_funded_wallet(Amount::from_btc(1.0).unwrap(), false);
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_broadcast_error(Some(BlockchainError::Broadcast("mempool rejected".into())));

        let err = SendBitcoinUsecase::new(
            Network::Regtest,
            Arc::clone(&repo),
            blockchain,
            Arc::new(SyncService),
        )
        .execute(&recipient_address(Network::Regtest), "0.25")
        .unwrap_err();
        assert_eq!(
            err,
            SendBitcoinUsecaseError::Blockchain(BlockchainError::Broadcast(
                "mempool rejected".into()
            ))
        );

        let reloaded = repo.load().unwrap().unwrap();
        assert!(reloaded.unconfirmed_txids().is_empty());
        assert_eq!(reloaded.spendable_balance(), Amount::from_btc(1.0).unwrap());
    }

    #[test]
    fn post_broadcast_save_failure_returns_txid_in_error() {
        let inner_repo = init_funded_wallet(Amount::from_btc(1.0).unwrap(), false);
        // Pre-broadcast save is a no-op when sync makes no changes, so the first actual
        // `save` call with staged changes (call #2 on the trait) is the post-broadcast save.
        let failing_repo: Arc<dyn NameTokenWalletRepository> =
            Arc::new(FailOnNthSaveRepository::new(inner_repo, 2));
        let blockchain = Arc::new(FakeBlockchain::new());

        let err = SendBitcoinUsecase::new(
            Network::Regtest,
            failing_repo,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute(&recipient_address(Network::Regtest), "0.25")
        .unwrap_err();

        let broadcasted = blockchain.broadcasted_txs();
        assert_eq!(broadcasted.len(), 1);
        let expected_txid = broadcasted[0].compute_txid();
        match err {
            SendBitcoinUsecaseError::BroadcastSucceededButPersistFailed { txid, reason } => {
                assert_eq!(txid, expected_txid);
                assert!(reason.contains("simulated disk full"), "{reason}");
            }
            other => panic!("expected BroadcastSucceededButPersistFailed, got {other:?}"),
        }
    }

    #[test]
    fn consecutive_sends_spend_unconfirmed_trusted_pending_change() {
        let repo = init_funded_wallet(Amount::from_btc(1.0).unwrap(), false);
        let blockchain = Arc::new(FakeBlockchain::new());
        let use_case = SendBitcoinUsecase::new(
            Network::Regtest,
            Arc::clone(&repo),
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        );

        let recipient = recipient_address(Network::Regtest);
        let txid1 = use_case.execute(&recipient, "0.25").unwrap();
        let txid2 = use_case.execute(&recipient, "0.1").unwrap();
        assert_ne!(txid1, txid2);
        assert_eq!(blockchain.broadcast_calls(), 2);

        let reloaded = repo.load().unwrap().unwrap();
        assert_eq!(reloaded.unconfirmed_txids().len(), 2);
        assert!(reloaded.spendable_balance() > Amount::from_btc(0.64).unwrap());
        assert!(reloaded.spendable_balance() < Amount::from_btc(0.65).unwrap());
    }
}
