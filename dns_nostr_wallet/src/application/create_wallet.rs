use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use bdk_wallet::rusqlite::Connection;
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;

const MNEMONIC_WORD_COUNT: usize = 12;

/// Use case: generate a new BIP39 mnemonic, initialize the wallet and persist it in SQLite.
pub struct CreateWallet {
    network: Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateWalletError {
    WalletAlreadyExists,
    Wallet(String),
}

impl CreateWallet {
    pub fn new(network: Network) -> Self {
        Self { network }
    }

    pub fn execute(&self, conn: &mut Connection) -> Result<Mnemonic, CreateWalletError> {
        let mnemonic =
            Mnemonic::generate(MNEMONIC_WORD_COUNT).expect("12 is a valid BIP39 word count");
        DnsNostrWallet::create(&mnemonic, self.network, conn)?;
        Ok(mnemonic)
    }
}

impl From<WalletError> for CreateWalletError {
    fn from(error: WalletError) -> Self {
        match error {
            WalletError::WalletAlreadyExists => Self::WalletAlreadyExists,
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

    #[test]
    fn stores_a_valid_12_word_mnemonic() {
        let mut conn = Connection::open_in_memory().unwrap();
        let mnemonic = CreateWallet::new(Network::Bitcoin)
            .execute(&mut conn)
            .unwrap();
        assert_eq!(mnemonic.word_count(), 12);

        let loaded = DnsNostrWallet::load(&mut conn).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn fails_when_wallet_exists() {
        let mut conn = Connection::open_in_memory().unwrap();
        let use_case = CreateWallet::new(Network::Bitcoin);
        use_case.execute(&mut conn).unwrap();

        let result = use_case.execute(&mut conn);
        assert_eq!(result, Err(CreateWalletError::WalletAlreadyExists));
    }
}
