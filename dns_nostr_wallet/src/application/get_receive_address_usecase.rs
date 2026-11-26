use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use crate::domain::sync_service::{SyncService, SyncServiceError};
use bdk_wallet::AddressInfo;
use bitcoin::Network;
use std::fmt;
use std::sync::Arc;

/// Use case: sync the wallet, reveal the next external (receive) BIP 84 Bitcoin address,
/// and persist the updated state.
pub struct GetReceiveAddressUsecase {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
    blockchain: Arc<dyn BlockchainPort>,
    sync_service: Arc<SyncService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetReceiveAddressUsecaseError {
    WalletNotFound,
    NetworkMismatch {
        configured: Network,
        stored: Network,
    },
    Blockchain(BlockchainError),
    Sync(SyncServiceError),
    Wallet(String),
}

impl GetReceiveAddressUsecase {
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

    pub fn execute(&self) -> Result<AddressInfo, GetReceiveAddressUsecaseError> {
        let mut wallet = self
            .repository
            .load()?
            .ok_or(GetReceiveAddressUsecaseError::WalletNotFound)?;
        if wallet.network() != self.network {
            return Err(GetReceiveAddressUsecaseError::NetworkMismatch {
                configured: self.network,
                stored: wallet.network(),
            });
        }

        self.sync_service.sync(&mut wallet, &*self.blockchain)?;
        let info = wallet.reveal_next_bitcoin_address();
        self.repository.save(&mut wallet)?;
        Ok(info)
    }
}

impl From<BlockchainError> for GetReceiveAddressUsecaseError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<SyncServiceError> for GetReceiveAddressUsecaseError {
    fn from(error: SyncServiceError) -> Self {
        Self::Sync(error)
    }
}

impl From<WalletRepositoryError> for GetReceiveAddressUsecaseError {
    fn from(error: WalletRepositoryError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl fmt::Display for GetReceiveAddressUsecaseError {
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

impl std::error::Error for GetReceiveAddressUsecaseError {}

#[cfg(test)]
mod test_get_receive_address_usecase {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::blockchain_port::FakeBlockchain;
    use crate::domain::dns_nostr_wallet::DnsNostrWallet;
    use bdk_wallet::KeychainKind;
    use bip39::Mnemonic;

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";

    fn init_wallet(network: Network) -> Arc<dyn NameTokenWalletRepository> {
        let repository = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = Mnemonic::parse(
            "abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon about",
        )
        .unwrap();
        let mut wallet = DnsNostrWallet::new(&mnemonic, network).unwrap();
        repository.create(&mnemonic, &mut wallet).unwrap();
        Arc::new(repository)
    }

    #[test]
    fn reveals_and_persists_sequential_addresses() {
        let repository = init_wallet(Network::Bitcoin);
        let blockchain = Arc::new(FakeBlockchain::new());
        let use_case = GetReceiveAddressUsecase::new(
            Network::Bitcoin,
            repository,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        );

        let first = use_case.execute().unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(first.keychain, KeychainKind::External);
        assert_eq!(first.address.to_string(), MAINNET_EXTERNAL_0);

        let second = use_case.execute().unwrap();
        assert_eq!(second.index, 1);
        assert_eq!(second.keychain, KeychainKind::External);
        assert_eq!(second.address.to_string(), MAINNET_EXTERNAL_1);
        assert_eq!(blockchain.sync_calls(), 2);
    }

    #[test]
    fn derives_expected_prefixes_across_networks() {
        for (network, expected_prefix) in [
            (Network::Bitcoin, "bc1q"),
            (Network::Testnet, "tb1q"),
            (Network::Regtest, "bcrt1q"),
        ] {
            let repository = init_wallet(network);
            let info = GetReceiveAddressUsecase::new(
                network,
                repository,
                Arc::new(FakeBlockchain::new()),
                Arc::new(SyncService),
            )
            .execute()
            .unwrap();
            assert_eq!(info.index, 0);
            assert!(
                info.address.to_string().starts_with(expected_prefix),
                "expected {expected_prefix} for {network:?}, got {}",
                info.address
            );
            if network == Network::Regtest {
                assert_eq!(info.address.to_string(), REGTEST_EXTERNAL_0);
            }
        }
    }

    #[test]
    fn rejects_network_mismatch_with_zero_rpc_calls() {
        let repository = init_wallet(Network::Regtest);
        let blockchain = Arc::new(FakeBlockchain::new());
        let err = GetReceiveAddressUsecase::new(
            Network::Bitcoin,
            repository,
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(
            err,
            GetReceiveAddressUsecaseError::NetworkMismatch {
                configured: Network::Bitcoin,
                stored: Network::Regtest,
            }
        );
        assert_eq!(blockchain.sync_calls(), 0);
    }

    #[test]
    fn sync_failure_does_not_advance_or_persist_address_index() {
        let repository = init_wallet(Network::Regtest);
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_error(BlockchainError::Rpc("rpc unavailable".into()));

        let err = GetReceiveAddressUsecase::new(
            Network::Regtest,
            Arc::clone(&repository),
            blockchain,
            Arc::new(SyncService),
        )
        .execute()
        .unwrap_err();
        assert_eq!(
            err,
            GetReceiveAddressUsecaseError::Sync(SyncServiceError::Blockchain(
                BlockchainError::Rpc("rpc unavailable".into())
            ))
        );

        let info = GetReceiveAddressUsecase::new(
            Network::Regtest,
            repository,
            Arc::new(FakeBlockchain::new()),
            Arc::new(SyncService),
        )
        .execute()
        .unwrap();
        assert_eq!(info.index, 0);
        assert_eq!(info.address.to_string(), REGTEST_EXTERNAL_0);
    }

    #[test]
    fn fails_without_wallet() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let result = GetReceiveAddressUsecase::new(
            Network::Regtest,
            repository,
            Arc::new(FakeBlockchain::new()),
            Arc::new(SyncService),
        )
        .execute();
        assert_eq!(result, Err(GetReceiveAddressUsecaseError::WalletNotFound));
    }
}
