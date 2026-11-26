use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use crate::domain::sync_service::{SyncService, SyncServiceError};
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;
use std::sync::Arc;

/// Use case: generate a new BIP39 mnemonic, anchor the wallet at the active chain tip,
/// and persist it atomically via the repository.
pub struct CreateWalletUsecase {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
    blockchain: Arc<dyn BlockchainPort>,
    sync_service: Arc<SyncService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateWalletUsecaseError {
    Blockchain(BlockchainError),
    Sync(SyncServiceError),
    WalletAlreadyExists,
    Wallet(String),
}

impl CreateWalletUsecase {
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

    pub fn execute(&self) -> Result<Mnemonic, CreateWalletUsecaseError> {
        const MNEMONIC_WORD_COUNT: usize = 12;
        if self.repository.load()?.is_some() {
            return Err(CreateWalletUsecaseError::WalletAlreadyExists);
        }

        let mnemonic =
            Mnemonic::generate(MNEMONIC_WORD_COUNT).expect("12 is a valid BIP39 word count");
        let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?;
        self.sync_service
            .tip_anchor(&mut wallet, &*self.blockchain)?;
        self.repository.create(&mnemonic, &mut wallet)?;
        Ok(mnemonic)
    }
}

impl From<BlockchainError> for CreateWalletUsecaseError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<SyncServiceError> for CreateWalletUsecaseError {
    fn from(error: SyncServiceError) -> Self {
        Self::Sync(error)
    }
}

impl From<WalletError> for CreateWalletUsecaseError {
    fn from(error: WalletError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl From<WalletRepositoryError> for CreateWalletUsecaseError {
    fn from(error: WalletRepositoryError) -> Self {
        match error {
            WalletRepositoryError::WalletAlreadyExists => Self::WalletAlreadyExists,
            other => Self::Wallet(other.to_string()),
        }
    }
}

impl fmt::Display for CreateWalletUsecaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blockchain(error) => write!(formatter, "{error}"),
            Self::Sync(error) => write!(formatter, "{error}"),
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for CreateWalletUsecaseError {}

#[cfg(test)]
mod test_create_wallet_usecase {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::blockchain_port::FakeBlockchain;
    use bdk_wallet::chain::BlockId;
    use bitcoin::hashes::Hash;
    use bitcoin::BlockHash;

    #[test]
    fn stores_a_valid_12_word_mnemonic_and_anchors_tip() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let blockchain = Arc::new(FakeBlockchain::new());
        let tip = BlockId {
            height: 42,
            hash: BlockHash::from_byte_array([0x42; 32]),
        };
        blockchain.set_tip_anchor_result(Ok(vec![tip]));

        let mnemonic = CreateWalletUsecase::new(
            Network::Regtest,
            Arc::clone(&repository),
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap();
        assert_eq!(mnemonic.word_count(), 12);
        assert_eq!(blockchain.tip_anchor_calls(), 1);

        let loaded = repository.load().unwrap().expect("wallet should persist");
        assert_eq!(loaded.latest_checkpoint().block_id(), tip);
    }

    #[test]
    fn fails_when_wallet_exists_without_calling_rpc() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let blockchain = Arc::new(FakeBlockchain::new());
        let use_case = CreateWalletUsecase::new(
            Network::Bitcoin,
            repository,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        );
        use_case.execute().unwrap();
        assert_eq!(blockchain.tip_anchor_calls(), 1);

        let result = use_case.execute();
        assert_eq!(result, Err(CreateWalletUsecaseError::WalletAlreadyExists));
        assert_eq!(blockchain.tip_anchor_calls(), 1);
    }

    #[test]
    fn rpc_failure_does_not_persist_wallet() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_tip_anchor_result(Err(BlockchainError::Rpc("node unreachable".into())));

        let err = CreateWalletUsecase::new(
            Network::Regtest,
            Arc::clone(&repository),
            blockchain,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(
            err,
            CreateWalletUsecaseError::Sync(SyncServiceError::Blockchain(BlockchainError::Rpc(
                "node unreachable".into()
            )))
        );
        assert!(repository.load().unwrap().is_none());
    }
}
