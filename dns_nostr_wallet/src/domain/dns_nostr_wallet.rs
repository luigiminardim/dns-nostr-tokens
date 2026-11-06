use bdk_wallet::rusqlite::{params, Connection};
use bdk_wallet::template::Bip84;
use bdk_wallet::{CreateWithPersistError, KeychainKind, PersistedWallet};
use bip39::Mnemonic;
use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
use bitcoin::hashes::{sha256d, Hash};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{Address, CompressedPublicKey, Network};
use dns_nostr_token::label::Label;
use nostr_sdk::{Keys, SecretKey};
use std::fmt;

const NO_PASSPHRASE: &str = "";
const NON_HARDENED_MASK: u32 = 0x7FFF_FFFF;
const NAME_TOKEN_PURPOSE: u32 = 44;
const FIRST_ACCOUNT: u32 = 0;

/// ASCII `"name"`: the chain level that keeps Name-Token UTXOs and their Nostr
/// keys invisible to standard wallets.
const NAME_TOKEN_CHAIN: u32 = 0x6E61_6D65;

/// Derivation index of a label: the first four bytes (big-endian) of the double
/// SHA-256 of the label, truncated to 31 bits so the index is non-hardened.
fn hash31(label: &Label) -> u32 {
    let digest = sha256d::Hash::hash(label.as_bytes()).to_byte_array();
    let prefix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    prefix & NON_HARDENED_MASK
}

/// BIP 44 / BIP 84 coin type: `0'` on mainnet, `1'` on every test network.
fn coin_type(network: Network) -> u32 {
    match network {
        Network::Bitcoin => 0,
        _ => 1,
    }
}

fn name_token_path(network: Network, index: u32) -> DerivationPath {
    account_path(NAME_TOKEN_PURPOSE, network, NAME_TOKEN_CHAIN, index)
}

fn nostr_key_path(network: Network, label: &Label) -> DerivationPath {
    account_path(NAME_TOKEN_PURPOSE, network, NAME_TOKEN_CHAIN, hash31(label))
}

fn account_path(purpose: u32, network: Network, chain: u32, index: u32) -> DerivationPath {
    DerivationPath::from(vec![
        hardened(purpose),
        hardened(coin_type(network)),
        hardened(FIRST_ACCOUNT),
        normal(chain),
        normal(index),
    ])
}

fn hardened(index: u32) -> ChildNumber {
    ChildNumber::from_hardened_idx(index).expect("hardened index below 2^31")
}

fn normal(index: u32) -> ChildNumber {
    ChildNumber::from_normal_idx(index).expect("normal index below 2^31")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardChain {
    External,
    Change,
}

/// Information about a derived Bitcoin address, including its index and keychain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo {
    pub index: u32,
    pub address: Address,
    pub chain: StandardChain,
}

#[derive(Debug)]
pub enum WalletError {
    WalletAlreadyExists,
    BdkCreate(String),
    BdkLoad(String),
    Persist(String),
    Sqlite(rusqlite::Error),
    Bip32(bitcoin::bip32::Error),
    InvalidNostrSecretKey,
    InvalidNetwork(String),
}

impl From<rusqlite::Error> for WalletError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<bitcoin::bip32::Error> for WalletError {
    fn from(error: bitcoin::bip32::Error) -> Self {
        Self::Bip32(error)
    }
}

impl PartialEq for WalletError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::WalletAlreadyExists, Self::WalletAlreadyExists) => true,
            (Self::BdkCreate(a), Self::BdkCreate(b)) => a == b,
            (Self::BdkLoad(a), Self::BdkLoad(b)) => a == b,
            (Self::Persist(a), Self::Persist(b)) => a == b,
            (Self::InvalidNostrSecretKey, Self::InvalidNostrSecretKey) => true,
            (Self::InvalidNetwork(a), Self::InvalidNetwork(b)) => a == b,
            (Self::Bip32(a), Self::Bip32(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for WalletError {}

impl fmt::Display for WalletError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::BdkCreate(error) => write!(formatter, "failed to initialize BDK wallet: {error}"),
            Self::BdkLoad(error) => write!(formatter, "failed to load BDK wallet: {error}"),
            Self::Persist(error) => write!(formatter, "failed to persist wallet state: {error}"),
            Self::Sqlite(error) => write!(formatter, "database error: {error}"),
            Self::Bip32(error) => write!(formatter, "BIP32 key derivation error: {error}"),
            Self::InvalidNostrSecretKey => write!(formatter, "derived an invalid Nostr secret key"),
            Self::InvalidNetwork(net) => write!(formatter, "invalid stored network `{net}`"),
        }
    }
}

impl std::error::Error for WalletError {}

/// The DNS-Nostr Wallet entity.
///
/// Embeds a `PersistedWallet<Connection>` for BIP 84 address management and persistence,
/// and owns protocol-specific derivation for Name-Tokens and per-label Nostr keys.
///
/// Master keys and seed phrases remain strictly private.
pub struct DnsNostrWallet {
    bdk_wallet: PersistedWallet<Connection>,
    master_key: Xpriv,
    network: Network,
}

impl DnsNostrWallet {
    /// Initializes a new wallet with BIP 84 descriptors and persists it into the SQLite connection.
    pub fn create(
        mnemonic: &Mnemonic,
        network: Network,
        conn: &mut Connection,
    ) -> Result<Self, WalletError> {
        init_table(conn)?;

        if has_wallet(conn)? {
            return Err(WalletError::WalletAlreadyExists);
        }

        let bdk_wallet = bdk_wallet::Wallet::create(
            Bip84(mnemonic.clone(), KeychainKind::External),
            Bip84(mnemonic.clone(), KeychainKind::Internal),
        )
        .network(network)
        .create_wallet(conn)
        .map_err(|error| match error {
            CreateWithPersistError::DataAlreadyExists(_) => WalletError::WalletAlreadyExists,
            other => WalletError::BdkCreate(other.to_string()),
        })?;

        conn.execute(
            "INSERT INTO dns_nostr_wallet (id, mnemonic, network) VALUES (0, ?1, ?2)",
            params![mnemonic.to_string(), network_to_string(network)],
        )?;

        let seed = mnemonic.to_seed(NO_PASSPHRASE);
        let master_key = Xpriv::new_master(network, &seed)?;

        Ok(Self {
            bdk_wallet,
            master_key,
            network,
        })
    }

    /// Loads an existing wallet from the SQLite database.
    pub fn load(conn: &mut Connection) -> Result<Option<Self>, WalletError> {
        init_table(conn)?;

        let stored = conn
            .query_row(
                "SELECT mnemonic, network FROM dns_nostr_wallet WHERE id = 0",
                [],
                |row| {
                    let mnemonic: String = row.get(0)?;
                    let network: String = row.get(1)?;
                    Ok((mnemonic, network))
                },
            )
            .optional()?;

        let (mnemonic_str, network_str) = match stored {
            Some(pair) => pair,
            None => return Ok(None),
        };

        let mnemonic = Mnemonic::parse(&mnemonic_str)
            .map_err(|error| WalletError::BdkLoad(format!("invalid stored mnemonic: {error}")))?;
        let network = parse_network(&network_str)?;

        let bdk_wallet = match bdk_wallet::Wallet::load().load_wallet(conn) {
            Ok(Some(wallet)) => wallet,
            Ok(None) => return Ok(None),
            Err(error) => return Err(WalletError::BdkLoad(error.to_string())),
        };

        let seed = mnemonic.to_seed(NO_PASSPHRASE);
        let master_key = Xpriv::new_master(network, &seed)?;

        Ok(Some(Self {
            bdk_wallet,
            master_key,
            network,
        }))
    }

    pub fn network(&self) -> Network {
        self.network
    }

    /// Reveals the next external receive address and automatically persists the incremented index.
    pub fn reveal_next_address(
        &mut self,
        conn: &mut Connection,
    ) -> Result<AddressInfo, WalletError> {
        let info = self.bdk_wallet.reveal_next_address(KeychainKind::External);
        self.bdk_wallet
            .persist(conn)
            .map_err(|error| WalletError::Persist(error.to_string()))?;

        Ok(AddressInfo {
            index: info.index,
            address: info.address,
            chain: StandardChain::External,
        })
    }

    /// Peeks at an address for a specific index without advancing the persisted index.
    pub fn peek_address(&self, index: u32) -> Result<AddressInfo, WalletError> {
        let info = self.bdk_wallet.peek_address(KeychainKind::External, index);
        Ok(AddressInfo {
            index: info.index,
            address: info.address,
            chain: StandardChain::External,
        })
    }

    /// Derives a Name-Token P2PKH address for the given index.
    pub fn name_token_address(&self, index: u32) -> Result<Address, WalletError> {
        let secp = Secp256k1::new();
        let path = name_token_path(self.network, index);
        let derived = self.master_key.derive_priv(&secp, &path)?;
        let public_key = CompressedPublicKey(derived.private_key.public_key(&secp));
        Ok(Address::p2pkh(public_key, self.network))
    }

    /// Derives the Nostr keys for a DNS-Nostr label.
    pub fn nostr_keys(&self, label: &Label) -> Result<Keys, WalletError> {
        let secp = Secp256k1::new();
        let path = nostr_key_path(self.network, label);
        let derived = self.master_key.derive_priv(&secp, &path)?;
        let secret_key = SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|_| WalletError::InvalidNostrSecretKey)?;
        Ok(Keys::new(secret_key))
    }
}

impl fmt::Debug for DnsNostrWallet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DnsNostrWallet")
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

fn init_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS dns_nostr_wallet (
            id INTEGER PRIMARY KEY CHECK (id = 0),
            mnemonic TEXT NOT NULL,
            network TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

fn has_wallet(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM dns_nostr_wallet WHERE id = 0",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn network_to_string(network: Network) -> &'static str {
    match network {
        Network::Bitcoin => "mainnet",
        Network::Testnet => "testnet",
        Network::Regtest => "regtest",
        Network::Signet => "signet",
        _ => "regtest",
    }
}

fn parse_network(name: &str) -> Result<Network, WalletError> {
    match name {
        "mainnet" => Ok(Network::Bitcoin),
        "testnet" => Ok(Network::Testnet),
        "regtest" => Ok(Network::Regtest),
        "signet" => Ok(Network::Signet),
        other => Err(WalletError::InvalidNetwork(other.to_string())),
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod test_dns_nostr_wallet {
    use super::*;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";
    const MAINNET_NAME_TOKEN_0: &str = "1M9PnNAN8XcKsrVtGPGSh8wi6n3eJyTXqe";
    const REGTEST_NAME_TOKEN_0: &str = "mtZEqSJy671ckaEjDptKD6xu6yzRsxcZsH";
    const MAINNET_ALICE_PUBLIC: &str =
        "0bc855f8de0ced298f6481e60445f0f6128092e2285e3f110f5caefe3c693fa4";

    fn sample_mnemonic() -> Mnemonic {
        Mnemonic::parse(MNEMONIC).unwrap()
    }

    #[test]
    fn debug_does_not_leak_mnemonic_or_keys() {
        let mut conn = Connection::open_in_memory().unwrap();
        let wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();
        let printed = format!("{:?}", wallet);
        assert!(!printed.contains("abandon"), "leaked: {printed}");
        assert!(!printed.contains("about"), "leaked: {printed}");
        assert!(!printed.contains("xprv"), "leaked: {printed}");
    }

    #[test]
    fn create_fails_when_wallet_already_exists() {
        let mut conn = Connection::open_in_memory().unwrap();
        DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();

        let duplicate = DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn);
        assert_eq!(duplicate.err(), Some(WalletError::WalletAlreadyExists));
    }

    #[test]
    fn load_returns_none_when_empty() {
        let mut conn = Connection::open_in_memory().unwrap();
        let loaded = DnsNostrWallet::load(&mut conn).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn reveal_next_address_advances_and_persists_to_sqlite() {
        let mut conn = Connection::open_in_memory().unwrap();
        let mut wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();

        let addr0 = wallet.reveal_next_address(&mut conn).unwrap();
        assert_eq!(addr0.index, 0);
        assert_eq!(addr0.address.to_string(), MAINNET_EXTERNAL_0);
        assert_eq!(addr0.chain, StandardChain::External);

        let addr1 = wallet.reveal_next_address(&mut conn).unwrap();
        assert_eq!(addr1.index, 1);
        assert_eq!(addr1.address.to_string(), MAINNET_EXTERNAL_1);

        // Load wallet in a new handle to prove persistence
        let mut loaded = DnsNostrWallet::load(&mut conn)
            .unwrap()
            .expect("wallet should load");

        let addr2 = loaded.reveal_next_address(&mut conn).unwrap();
        assert_eq!(addr2.index, 2);
    }

    #[test]
    fn peek_address_does_not_advance_persisted_index() {
        let mut conn = Connection::open_in_memory().unwrap();
        let mut wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();

        let peeked = wallet.peek_address(1).unwrap();
        assert_eq!(peeked.index, 1);
        assert_eq!(peeked.address.to_string(), MAINNET_EXTERNAL_1);

        let revealed0 = wallet.reveal_next_address(&mut conn).unwrap();
        assert_eq!(revealed0.index, 0);
        assert_eq!(revealed0.address.to_string(), MAINNET_EXTERNAL_0);
    }

    #[test]
    fn name_token_address_matches_known_vector_mainnet() {
        let mut conn = Connection::open_in_memory().unwrap();
        let wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();
        let address = wallet.name_token_address(0).unwrap();
        assert_eq!(address.to_string(), MAINNET_NAME_TOKEN_0);
    }

    #[test]
    fn name_token_address_matches_known_vector_regtest() {
        let mut conn = Connection::open_in_memory().unwrap();
        let wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Regtest, &mut conn).unwrap();
        let address = wallet.name_token_address(0).unwrap();
        assert_eq!(address.to_string(), REGTEST_NAME_TOKEN_0);
    }

    #[test]
    fn nostr_keys_matches_known_vector() {
        let mut conn = Connection::open_in_memory().unwrap();
        let wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Bitcoin, &mut conn).unwrap();
        let label = Label::new("alice").unwrap();
        let keys = wallet.nostr_keys(&label).unwrap();
        assert_eq!(keys.public_key().to_hex(), MAINNET_ALICE_PUBLIC);
    }

    #[test]
    fn hash31_of_alice_matches_expected() {
        let label = Label::new("alice").unwrap();
        assert_eq!(hash31(&label), 1_026_581_541);
    }

    #[test]
    fn standard_address_matches_regtest_vector() {
        let mut conn = Connection::open_in_memory().unwrap();
        let mut wallet =
            DnsNostrWallet::create(&sample_mnemonic(), Network::Regtest, &mut conn).unwrap();
        let addr = wallet.reveal_next_address(&mut conn).unwrap();
        assert_eq!(addr.address.to_string(), REGTEST_EXTERNAL_0);
    }
}
