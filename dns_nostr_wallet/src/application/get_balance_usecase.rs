use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use crate::domain::sync_service::{SyncService, SyncServiceError};
use bitcoin::{Amount, Network};
use std::fmt;
use std::sync::Arc;

/// Use case: sync the wallet against Bitcoin Core, persist sync progress, and return the
/// trusted spendable balance (`confirmed + trusted_pending`).
pub struct GetBalanceUsecase {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
    blockchain: Arc<dyn BlockchainPort>,
    sync_service: Arc<SyncService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetBalanceUsecaseError {
    WalletNotFound,
    NetworkMismatch {
        configured: Network,
        stored: Network,
    },
    Blockchain(BlockchainError),
    Sync(SyncServiceError),
    Wallet(String),
}

impl GetBalanceUsecase {
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

    pub fn execute(&self) -> Result<Amount, GetBalanceUsecaseError> {
        let mut wallet = self
            .repository
            .load()?
            .ok_or(GetBalanceUsecaseError::WalletNotFound)?;
        if wallet.network() != self.network {
            return Err(GetBalanceUsecaseError::NetworkMismatch {
                configured: self.network,
                stored: wallet.network(),
            });
        }

        self.sync_service.sync(&mut wallet, &*self.blockchain)?;
        self.repository.save(&mut wallet)?;
        Ok(wallet.spendable_balance())
    }
}

impl From<BlockchainError> for GetBalanceUsecaseError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<SyncServiceError> for GetBalanceUsecaseError {
    fn from(error: SyncServiceError) -> Self {
        Self::Sync(error)
    }
}

impl From<WalletRepositoryError> for GetBalanceUsecaseError {
    fn from(error: WalletRepositoryError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl fmt::Display for GetBalanceUsecaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletNotFound => write!(
                formatter,
                "no wallet found; run `wallet create` or `wallet import` first"
            ),
            Self::NetworkMismatch { configured, stored } => write!(
                formatter,
                "configured network ({configured}) does not match stored wallet network ({stored})"
            ),
            Self::Blockchain(error) => write!(formatter, "{error}"),
            Self::Sync(error) => write!(formatter, "{error}"),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for GetBalanceUsecaseError {}

#[cfg(test)]
mod test_get_balance_usecase {
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
        Block, CompactTarget, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
        TxMerkleNode, TxOut, Txid, Witness,
    };

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    fn init_wallet(network: Network) -> Arc<dyn NameTokenWalletRepository> {
        let repository = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = Mnemonic::parse(MNEMONIC).unwrap();
        let mut wallet = DnsNostrWallet::new(&mnemonic, network).unwrap();
        repository.create(&mnemonic, &mut wallet).unwrap();
        Arc::new(repository)
    }

    #[test]
    fn syncs_persists_and_returns_confirmed_plus_trusted_pending_excluding_immature_coinbase() {
        let repository = init_wallet(Network::Regtest);
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_hook(|wallet| {
            let addr = wallet.reveal_next_bitcoin_address().address;
            let genesis = wallet.latest_checkpoint().block_id();

            let coinbase = Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint::null(),
                    script_sig: ScriptBuf::from_bytes(vec![0x01, 0x01]),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![TxOut {
                    value: Amount::from_btc(50.0).unwrap(),
                    script_pubkey: addr.script_pubkey(),
                }],
            };
            let confirmed_tx = Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint {
                        txid: Txid::from_byte_array([0x55; 32]),
                        vout: 0,
                    },
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![TxOut {
                    value: Amount::from_btc(1.0).unwrap(),
                    script_pubkey: addr.script_pubkey(),
                }],
            };
            let block1 = Block {
                header: Header {
                    version: BlockVersion::TWO,
                    prev_blockhash: genesis.hash,
                    merkle_root: TxMerkleNode::all_zeros(),
                    time: 1_700_000_001,
                    bits: CompactTarget::from_consensus(0x207f_ffff),
                    nonce: 1,
                },
                txdata: vec![coinbase, confirmed_tx],
            };
            wallet
                .apply_block_connected_to(&block1, 1, genesis)
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))?;

            // Create a self-sent unconfirmed spend so change is trusted_pending.
            let other_mnemonic =
                Mnemonic::parse("zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong").unwrap();
            let mut other = DnsNostrWallet::new(&other_mnemonic, Network::Regtest).unwrap();
            let external = other.reveal_next_bitcoin_address().address;
            let spend = wallet
                .build_and_sign_transaction(
                    external,
                    Amount::from_btc(0.25).unwrap(),
                    FeeRate::from_sat_per_kwu(250),
                )
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))?;
            wallet.record_broadcasted_transaction(spend, 1_700_000_100);
            Ok(())
        });

        let balance = GetBalanceUsecase::new(
            Network::Regtest,
            Arc::clone(&repository),
            blockchain,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap();

        // Immature 50 BTC coinbase is excluded; 1.0 - 0.25 - fee trusted_pending change is included.
        assert!(balance > Amount::from_btc(0.74).unwrap());
        assert!(balance < Amount::from_btc(0.75).unwrap());

        // Sync progress was persisted to repository.
        let reloaded = repository.load().unwrap().unwrap();
        assert_eq!(reloaded.latest_checkpoint().height(), 1);
        assert_eq!(reloaded.spendable_balance(), balance);
    }

    #[test]
    fn fails_without_wallet() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let err = GetBalanceUsecase::new(
            Network::Regtest,
            repository,
            Arc::new(FakeBlockchain::new()),
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(err, GetBalanceUsecaseError::WalletNotFound);
    }

    #[test]
    fn rejects_network_mismatch_with_zero_rpc_calls() {
        let repository = init_wallet(Network::Regtest);
        let blockchain = Arc::new(FakeBlockchain::new());
        let err = GetBalanceUsecase::new(
            Network::Bitcoin,
            repository,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(
            err,
            GetBalanceUsecaseError::NetworkMismatch {
                configured: Network::Bitcoin,
                stored: Network::Regtest,
            }
        );
        assert_eq!(blockchain.sync_calls(), 0);
    }

    #[test]
    fn propagates_sync_errors() {
        let repository = init_wallet(Network::Regtest);
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_error(BlockchainError::Rpc("connection refused".into()));
        let err = GetBalanceUsecase::new(
            Network::Regtest,
            repository,
            blockchain,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(
            err,
            GetBalanceUsecaseError::Sync(SyncServiceError::Blockchain(BlockchainError::Rpc(
                "connection refused".into()
            )))
        );
    }
}
