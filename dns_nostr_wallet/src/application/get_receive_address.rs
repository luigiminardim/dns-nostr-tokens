use crate::domain::dns_nostr_wallet::{AddressInfo, DnsNostrWallet, WalletError};
use bdk_wallet::rusqlite::Connection;
use std::fmt;

/// Use case: derive the standard external (receive) address.
///
/// When `index` is `None`, reveals the next address from the wallet's external keychain and persists it.
/// When `index` is `Some(i)`, peeks at index `i` without advancing the keychain.
#[derive(Default)]
pub struct GetReceiveAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetReceiveAddressError {
    WalletNotFound,
    Wallet(String),
}

impl GetReceiveAddress {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(
        &self,
        conn: &mut Connection,
        index: Option<u32>,
    ) -> Result<AddressInfo, GetReceiveAddressError> {
        let mut wallet =
            DnsNostrWallet::load(conn)?.ok_or(GetReceiveAddressError::WalletNotFound)?;
        let info = match index {
            Some(i) => wallet.peek_address(i)?,
            None => wallet.reveal_next_address(conn)?,
        };
        Ok(info)
    }
}

impl From<WalletError> for GetReceiveAddressError {
    fn from(error: WalletError) -> Self {
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
    use crate::domain::dns_nostr_wallet::StandardChain;
    use bip39::Mnemonic;
    use bitcoin::Network;

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";

    fn init_wallet(conn: &mut Connection, network: Network) {
        let mnemonic = Mnemonic::parse(
            "abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon about",
        )
        .unwrap();
        DnsNostrWallet::create(&mnemonic, network, conn).unwrap();
    }

    #[test]
    fn matches_known_vector_default() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_wallet(&mut conn, Network::Bitcoin);
        let mainnet = GetReceiveAddress::new().execute(&mut conn, None).unwrap();
        assert_eq!(mainnet.index, 0);
        assert_eq!(mainnet.chain, StandardChain::External);
        assert_eq!(mainnet.address.to_string(), MAINNET_EXTERNAL_0);

        let mut regtest_conn = Connection::open_in_memory().unwrap();
        init_wallet(&mut regtest_conn, Network::Regtest);
        let regtest = GetReceiveAddress::new()
            .execute(&mut regtest_conn, None)
            .unwrap();
        assert_eq!(regtest.index, 0);
        assert_eq!(regtest.chain, StandardChain::External);
        assert_eq!(regtest.address.to_string(), REGTEST_EXTERNAL_0);
    }

    #[test]
    fn peeks_at_specified_index() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_wallet(&mut conn, Network::Bitcoin);
        let address = GetReceiveAddress::new()
            .execute(&mut conn, Some(1))
            .unwrap();
        assert_eq!(address.index, 1);
        assert_eq!(address.chain, StandardChain::External);
        assert_eq!(address.address.to_string(), MAINNET_EXTERNAL_1);
    }

    #[test]
    fn fails_without_wallet() {
        let mut conn = Connection::open_in_memory().unwrap();
        let result = GetReceiveAddress::new().execute(&mut conn, None);
        assert_eq!(result, Err(GetReceiveAddressError::WalletNotFound));
    }
}
