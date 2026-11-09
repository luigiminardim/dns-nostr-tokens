use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;
use std::sync::Arc;

/// Use case: generate a new BIP39 mnemonic, initialize the wallet and persist it via the repository.
pub struct CreateWallet {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateWalletError {
    WalletAlreadyExists,
    Wallet(String),
}

impl CreateWallet {
    pub fn new(network: Network, repository: Arc<dyn NameTokenWalletRepository>) -> Self {
        Self {
            network,
            repository,
        }
    }

    pub fn execute(&self) -> Result<Mnemonic, CreateWalletError> {
        const MNEMONIC_WORD_COUNT: usize = 12;
        let mnemonic =
            Mnemonic::generate(MNEMONIC_WORD_COUNT).expect("12 is a valid BIP39 word count");
        let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?;
        self.repository.create(&mnemonic, &mut wallet)?;
        Ok(mnemonic)
    }
}

impl From<WalletError> for CreateWalletError {
    fn from(error: WalletError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl From<WalletRepositoryError> for CreateWalletError {
    fn from(error: WalletRepositoryError) -> Self {
        match error {
            WalletRepositoryError::WalletAlreadyExists => Self::WalletAlreadyExists,
            other => Self::Wallet(other.to_string()),
        }
    }
}

impl fmt::Display for CreateWalletError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for CreateWalletError {}

#[cfg(test)]
mod test_create_wallet {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;

    #[test]
    fn stores_a_valid_12_word_mnemonic() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let mnemonic = CreateWallet::new(Network::Bitcoin, Arc::clone(&repository))
            .execute()
            .unwrap();
        assert_eq!(mnemonic.word_count(), 12);

        let loaded = repository.load().unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn fails_when_wallet_exists() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let use_case = CreateWallet::new(Network::Bitcoin, repository);
        use_case.execute().unwrap();

        let result = use_case.execute();
        assert_eq!(result, Err(CreateWalletError::WalletAlreadyExists));
    }
}
