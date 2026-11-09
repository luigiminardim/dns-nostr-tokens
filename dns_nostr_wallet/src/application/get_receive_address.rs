use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use bdk_wallet::AddressInfo;
use std::fmt;
use std::sync::Arc;

/// Use case: reveal the next external (receive) BIP 84 Bitcoin address and persist the updated index.
pub struct GetReceiveAddress {
    repository: Arc<dyn NameTokenWalletRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetReceiveAddressError {
    WalletNotFound,
    Wallet(String),
}

impl GetReceiveAddress {
    pub fn new(repository: Arc<dyn NameTokenWalletRepository>) -> Self {
        Self { repository }
    }

    pub fn execute(&self) -> Result<AddressInfo, GetReceiveAddressError> {
        let mut wallet = self
            .repository
            .load()?
            .ok_or(GetReceiveAddressError::WalletNotFound)?;
        let info = wallet.reveal_next_bitcoin_address();
        self.repository.save(&mut wallet)?;
        Ok(info)
    }
}

impl From<WalletRepositoryError> for GetReceiveAddressError {
    fn from(error: WalletRepositoryError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl fmt::Display for GetReceiveAddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletNotFound => write!(
                formatter,
                "no wallet found; run `wallet create` or `wallet import` first"
            ),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for GetReceiveAddressError {}

#[cfg(test)]
mod test_get_receive_address {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::dns_nostr_wallet::DnsNostrWallet;
    use bdk_wallet::KeychainKind;
    use bip39::Mnemonic;
    use bitcoin::Network;

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
        let use_case = GetReceiveAddress::new(repository);

        let first = use_case.execute().unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(first.keychain, KeychainKind::External);
        assert_eq!(first.address.to_string(), MAINNET_EXTERNAL_0);

        let second = use_case.execute().unwrap();
        assert_eq!(second.index, 1);
        assert_eq!(second.keychain, KeychainKind::External);
        assert_eq!(second.address.to_string(), MAINNET_EXTERNAL_1);
    }

    #[test]
    fn matches_known_regtest_vector() {
        let repository = init_wallet(Network::Regtest);
        let regtest = GetReceiveAddress::new(repository).execute().unwrap();
        assert_eq!(regtest.index, 0);
        assert_eq!(regtest.keychain, KeychainKind::External);
        assert_eq!(regtest.address.to_string(), REGTEST_EXTERNAL_0);
    }

    #[test]
    fn fails_without_wallet() {
        let repository: Arc<dyn NameTokenWalletRepository> =
            Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap());
        let result = GetReceiveAddress::new(repository).execute();
        assert_eq!(result, Err(GetReceiveAddressError::WalletNotFound));
    }
}
