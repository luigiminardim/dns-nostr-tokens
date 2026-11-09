use crate::domain::key_derivation_policy::KeyDerivationPolicy;
use bdk_wallet::{AddressInfo, ChangeSet, KeychainKind};
use bip39::Mnemonic;
use bitcoin::bip32::Xpriv;
use bitcoin::Network;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletError {
    BdkCreate(String),
    Bip32(bitcoin::bip32::Error),
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
        }
    }
}

impl std::error::Error for WalletError {}

/// The DNS-Nostr Wallet entity.
///
/// Wraps an in-memory `bdk_wallet::Wallet` for BIP 84 address management and staged
/// `ChangeSet` tracking alongside the wallet's `master_key: Xpriv`.
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

    /// Reveals the next external BIP 84 receive address and stages the keychain index update.
    pub fn reveal_next_bitcoin_address(&mut self) -> AddressInfo {
        self.bdk_wallet.reveal_next_address(KeychainKind::External)
    }

    /// Returns an immutable reference to the staged `ChangeSet`, if any, without draining it.
    pub fn staged(&self) -> Option<&ChangeSet> {
        self.bdk_wallet.staged()
    }

    /// Takes and clears the staged `ChangeSet` after a repository transaction commits.
    pub fn take_staged(&mut self) -> Option<ChangeSet> {
        self.bdk_wallet.take_staged()
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

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";

    fn sample_mnemonic() -> Mnemonic {
        Mnemonic::parse(MNEMONIC).unwrap()
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
}
