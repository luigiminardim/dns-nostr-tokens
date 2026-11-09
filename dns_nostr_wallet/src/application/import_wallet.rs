use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;
use std::sync::Arc;

/// Use case: validate a user-supplied BIP39 mnemonic, initialize the wallet and persist it via the repository.
pub struct ImportWallet {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWalletError {
    InvalidMnemonic(String),
    WalletAlreadyExists,
    Wallet(String),
}

impl ImportWallet {
    pub fn new(network: Network, repository: Arc<dyn NameTokenWalletRepository>) -> Self {
        Self {
            network,
            repository,
        }
    }

    pub fn execute(&self, phrase: &str) -> Result<(), ImportWalletError> {
        let mnemonic = Self::parse_mnemonic(phrase)?;
        let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?;
        self.repository.create(&mnemonic, &mut wallet)?;
        Ok(())
    }

    fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, ImportWalletError> {
        let normalized = phrase
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ");
        Mnemonic::parse(normalized).map_err(|e| ImportWalletError::InvalidMnemonic(e.to_string()))
    }
}

impl From<WalletError> for ImportWalletError {
    fn from(error: WalletError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl From<WalletRepositoryError> for ImportWalletError {
    fn from(error: WalletRepositoryError) -> Self {
        match error {
            WalletRepositoryError::WalletAlreadyExists => Self::WalletAlreadyExists,
            other => Self::Wallet(other.to_string()),
        }
    }
}

impl fmt::Display for ImportWalletError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMnemonic(error) => write!(formatter, "invalid seed phrase: {error}"),
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ImportWalletError {}

#[cfg(test)]
mod test_import_wallet {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;

    const VALID: &str = "abandon abandon abandon abandon abandon abandon \
                         abandon abandon abandon abandon abandon about";

    fn in_memory_repo() -> Arc<dyn NameTokenWalletRepository> {
        Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap())
    }

    fn import(
        repository: Arc<dyn NameTokenWalletRepository>,
        phrase: &str,
    ) -> Result<(), ImportWalletError> {
        ImportWallet::new(Network::Bitcoin, repository).execute(phrase)
    }

    fn assert_invalid(phrase: &str) {
        let repository = in_memory_repo();
        let result = import(repository, phrase);
        assert!(
            matches!(result, Err(ImportWalletError::InvalidMnemonic(_))),
            "{result:?}"
        );
    }

    #[test]
    fn valid_mnemonic_is_stored() {
        let repository = in_memory_repo();
        import(Arc::clone(&repository), VALID).unwrap();
        let loaded = repository.load().unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn normalises_whitespace_and_case() {
        let repository = in_memory_repo();
        let messy = format!("  {}\n", VALID.to_uppercase().replace(' ', "   "));
        import(Arc::clone(&repository), &messy).unwrap();
        let loaded = repository.load().unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn rejects_bad_checksum() {
        assert_invalid(&["abandon"; 12].join(" "));
    }

    #[test]
    fn rejects_unknown_word() {
        assert_invalid(&VALID.replace("about", "notaword"));
    }

    #[test]
    fn rejects_wrong_word_count() {
        assert_invalid(&["abandon"; 11].join(" "));
    }

    #[test]
    fn rejects_empty_input() {
        assert_invalid("");
        assert_invalid("   \n ");
    }

    #[test]
    fn rejected_import_stores_nothing() {
        let repository = in_memory_repo();
        let _ = import(Arc::clone(&repository), "not a mnemonic");
        let loaded = repository.load().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn fails_when_wallet_exists() {
        let repository = in_memory_repo();
        import(Arc::clone(&repository), VALID).unwrap();
        let result = import(repository, VALID);
        assert_eq!(result, Err(ImportWalletError::WalletAlreadyExists));
    }
}
