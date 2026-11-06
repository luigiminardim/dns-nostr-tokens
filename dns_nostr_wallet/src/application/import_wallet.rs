use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use bdk_wallet::rusqlite::Connection;
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;

/// Use case: validate a user-supplied BIP39 mnemonic, initialize the wallet and persist it in SQLite.
pub struct ImportWallet {
    network: Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWalletError {
    InvalidMnemonic(String),
    WalletAlreadyExists,
    Wallet(String),
}

impl ImportWallet {
    pub fn new(network: Network) -> Self {
        Self { network }
    }

    pub fn execute(&self, conn: &mut Connection, phrase: &str) -> Result<(), ImportWalletError> {
        let mnemonic = parse_mnemonic(phrase)?;
        DnsNostrWallet::create(&mnemonic, self.network, conn)?;
        Ok(())
    }
}

fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, ImportWalletError> {
    let normalized = phrase
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    Mnemonic::parse(normalized).map_err(|e| ImportWalletError::InvalidMnemonic(e.to_string()))
}

impl From<WalletError> for ImportWalletError {
    fn from(error: WalletError) -> Self {
        match error {
            WalletError::WalletAlreadyExists => Self::WalletAlreadyExists,
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

    const VALID: &str = "abandon abandon abandon abandon abandon abandon \
                         abandon abandon abandon abandon abandon about";

    fn import(conn: &mut Connection, phrase: &str) -> Result<(), ImportWalletError> {
        ImportWallet::new(Network::Bitcoin).execute(conn, phrase)
    }

    fn assert_invalid(phrase: &str) {
        let mut conn = Connection::open_in_memory().unwrap();
        let result = import(&mut conn, phrase);
        assert!(
            matches!(result, Err(ImportWalletError::InvalidMnemonic(_))),
            "{result:?}"
        );
    }

    #[test]
    fn valid_mnemonic_is_stored() {
        let mut conn = Connection::open_in_memory().unwrap();
        import(&mut conn, VALID).unwrap();
        let loaded = DnsNostrWallet::load(&mut conn).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn normalises_whitespace_and_case() {
        let mut conn = Connection::open_in_memory().unwrap();
        let messy = format!("  {}\n", VALID.to_uppercase().replace(' ', "   "));
        import(&mut conn, &messy).unwrap();
        let loaded = DnsNostrWallet::load(&mut conn).unwrap();
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
        let mut conn = Connection::open_in_memory().unwrap();
        let _ = import(&mut conn, "not a mnemonic");
        let loaded = DnsNostrWallet::load(&mut conn).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn fails_when_wallet_exists() {
        let mut conn = Connection::open_in_memory().unwrap();
        import(&mut conn, VALID).unwrap();
        let result = import(&mut conn, VALID);
        assert_eq!(result, Err(ImportWalletError::WalletAlreadyExists));
    }
}
