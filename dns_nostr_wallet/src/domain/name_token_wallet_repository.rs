use crate::domain::dns_nostr_wallet::DnsNostrWallet;
use bip39::Mnemonic;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletRepositoryError {
    WalletAlreadyExists,
    CorruptedState(String),
    Storage(String),
}

impl fmt::Display for WalletRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::CorruptedState(reason) => write!(formatter, "corrupted wallet state: {reason}"),
            Self::Storage(reason) => write!(formatter, "storage error: {reason}"),
        }
    }
}

impl std::error::Error for WalletRepositoryError {}

/// Synchronous persistence port for storing and loading `DnsNostrWallet`.
pub trait NameTokenWalletRepository {
    /// Atomically persists a newly created or imported wallet and its mnemonic,
    /// clearing the wallet's staged `ChangeSet` only after the storage transaction commits.
    fn create(
        &self,
        mnemonic: &Mnemonic,
        wallet: &mut DnsNostrWallet,
    ) -> Result<(), WalletRepositoryError>;

    /// Atomically persists the wallet's staged `ChangeSet`, clearing it in memory
    /// only after the storage transaction commits.
    fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>;

    /// Loads the persisted wallet from storage, returning `Ok(None)` if no wallet exists.
    fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError>;
}
