use bdk_wallet::template::Bip84;
use bdk_wallet::KeychainKind;
use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
use bitcoin::hashes::{sha256d, Hash};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::NetworkKind;
use dns_nostr_token::label::Label;
use nostr_sdk::{Keys, SecretKey};
use std::fmt;

/// Stateless policy encapsulating BIP 32 / BIP 44 / BIP 84 key derivation rules.
pub struct KeyDerivationPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDerivationError {
    InvalidChildIndex(u32),
    Bip32(bitcoin::bip32::Error),
    InvalidNostrSecretKey,
}

impl From<bitcoin::bip32::Error> for KeyDerivationError {
    fn from(error: bitcoin::bip32::Error) -> Self {
        Self::Bip32(error)
    }
}

impl fmt::Display for KeyDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChildIndex(index) => {
                write!(formatter, "child index {index} must be non-hardened (< 2^31)")
            }
            Self::Bip32(error) => write!(formatter, "BIP32 key derivation error: {error}"),
            Self::InvalidNostrSecretKey => write!(formatter, "derived an invalid Nostr secret key"),
        }
    }
}

impl std::error::Error for KeyDerivationError {}

impl KeyDerivationPolicy {
    /// Returns the BIP 84 external (receive) descriptor template (`m/84'/coin'/0'/0/*`).
    pub fn bitcoin_external_descriptor(master_key: &Xpriv) -> Bip84<Xpriv> {
        Bip84(*master_key, KeychainKind::External)
    }

    /// Returns the BIP 84 internal (change) descriptor template (`m/84'/coin'/0'/1/*`).
    pub fn bitcoin_internal_descriptor(master_key: &Xpriv) -> Bip84<Xpriv> {
        Bip84(*master_key, KeychainKind::Internal)
    }

    /// Derives the child `Xpriv` for a Name-Token UTXO at `m/44'/coin'/0'/0x6E616D65/index`.
    pub fn name_token_utxo_key(
        master_key: &Xpriv,
        index: u32,
    ) -> Result<Xpriv, KeyDerivationError> {
        let child_index = ChildNumber::from_normal_idx(index)
            .map_err(|_| KeyDerivationError::InvalidChildIndex(index))?;
        let coin_type = match master_key.network {
            NetworkKind::Main => 0,
            NetworkKind::Test => 1,
        };
        let path = Self::name_token_path(coin_type, child_index);
        let secp = Secp256k1::new();
        Ok(master_key.derive_priv(&secp, &path)?)
    }

    /// Derives the Nostr keypair for a DNS-Nostr label at `m/44'/1237'/0'/0x6E616D65/hash31(label)`.
    pub fn nostr_key(master_key: &Xpriv, label: &Label) -> Result<Keys, KeyDerivationError> {
        const NOSTR_COIN_TYPE: u32 = 1237;
        let child_index = ChildNumber::from_normal_idx(Self::hash31(label))
            .expect("hash31 masks to 31 bits (< 2^31)");
        let path = Self::name_token_path(NOSTR_COIN_TYPE, child_index);
        let secp = Secp256k1::new();
        let derived = master_key.derive_priv(&secp, &path)?;
        let secret_key = SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|_| KeyDerivationError::InvalidNostrSecretKey)?;
        Ok(Keys::new(secret_key))
    }

    /// Derivation index of a label: the first four bytes (big-endian) of the double
    /// SHA-256 of the label, truncated to 31 bits so the index is non-hardened.
    fn hash31(label: &Label) -> u32 {
        const NON_HARDENED_MASK: u32 = 0x7FFF_FFFF;
        let digest = sha256d::Hash::hash(label.as_bytes()).to_byte_array();
        let prefix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
        prefix & NON_HARDENED_MASK
    }

    fn name_token_path(coin_type: u32, index: ChildNumber) -> DerivationPath {
        const NAME_TOKEN_PURPOSE: u32 = 44;
        const FIRST_ACCOUNT: u32 = 0;
        // ASCII `"name"`: the chain level that keeps Name-Token UTXOs and their Nostr
        // keys invisible to standard wallets.
        const NAME_TOKEN_CHAIN: u32 = 0x6E61_6D65;

        DerivationPath::from(vec![
            ChildNumber::from_hardened_idx(NAME_TOKEN_PURPOSE).unwrap(),
            ChildNumber::from_hardened_idx(coin_type).unwrap(),
            ChildNumber::from_hardened_idx(FIRST_ACCOUNT).unwrap(),
            ChildNumber::from_normal_idx(NAME_TOKEN_CHAIN).unwrap(),
            index,
        ])
    }
}

#[cfg(test)]
mod test_key_derivation_policy {
    use super::*;
    use bip39::Mnemonic;
    use bitcoin::{Address, CompressedPublicKey, Network};

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_INTERNAL_0: &str = "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";
    const MAINNET_NAME_TOKEN_0: &str = "1M9PnNAN8XcKsrVtGPGSh8wi6n3eJyTXqe";
    const REGTEST_NAME_TOKEN_0: &str = "mtZEqSJy671ckaEjDptKD6xu6yzRsxcZsH";
    const ALICE_NOSTR_PUBLIC: &str =
        "c1d71f9bb7fcff517470d27828cf92182b57444636f117dd6a65a00b0a181f25";

    fn master_key(network: Network) -> Xpriv {
        let mnemonic = Mnemonic::parse(MNEMONIC).unwrap();
        let seed = mnemonic.to_seed("");
        Xpriv::new_master(network, &seed).unwrap()
    }

    fn p2pkh_address(xpriv: &Xpriv) -> Address {
        let secp = Secp256k1::new();
        let pubkey = CompressedPublicKey(xpriv.private_key.public_key(&secp));
        Address::p2pkh(pubkey, xpriv.network)
    }

    #[test]
    fn bitcoin_descriptors_match_mainnet_vectors() {
        let master = master_key(Network::Bitcoin);
        let mut wallet = bdk_wallet::Wallet::create(
            KeyDerivationPolicy::bitcoin_external_descriptor(&master),
            KeyDerivationPolicy::bitcoin_internal_descriptor(&master),
        )
        .network(Network::Bitcoin)
        .create_wallet_no_persist()
        .unwrap();

        let ext0 = wallet.reveal_next_address(KeychainKind::External);
        assert_eq!(ext0.address.to_string(), MAINNET_EXTERNAL_0);

        let int0 = wallet.reveal_next_address(KeychainKind::Internal);
        assert_eq!(int0.address.to_string(), MAINNET_INTERNAL_0);
    }

    #[test]
    fn bitcoin_descriptors_match_regtest_vector() {
        let master = master_key(Network::Regtest);
        let mut wallet = bdk_wallet::Wallet::create(
            KeyDerivationPolicy::bitcoin_external_descriptor(&master),
            KeyDerivationPolicy::bitcoin_internal_descriptor(&master),
        )
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

        let ext0 = wallet.reveal_next_address(KeychainKind::External);
        assert_eq!(ext0.address.to_string(), REGTEST_EXTERNAL_0);
    }

    #[test]
    fn name_token_utxo_key_matches_known_vector_mainnet() {
        let master = master_key(Network::Bitcoin);
        let derived = KeyDerivationPolicy::name_token_utxo_key(&master, 0).unwrap();
        assert_eq!(p2pkh_address(&derived).to_string(), MAINNET_NAME_TOKEN_0);
    }

    #[test]
    fn name_token_utxo_key_matches_known_vector_regtest() {
        let master = master_key(Network::Regtest);
        let derived = KeyDerivationPolicy::name_token_utxo_key(&master, 0).unwrap();
        assert_eq!(p2pkh_address(&derived).to_string(), REGTEST_NAME_TOKEN_0);
    }

    #[test]
    fn name_token_utxo_key_rejects_hardened_index() {
        let master = master_key(Network::Bitcoin);
        let err = KeyDerivationPolicy::name_token_utxo_key(&master, 0x8000_0000).unwrap_err();
        assert_eq!(err, KeyDerivationError::InvalidChildIndex(0x8000_0000));
    }

    #[test]
    fn nostr_key_matches_known_vector() {
        let master = master_key(Network::Bitcoin);
        let label = Label::new("alice").unwrap();
        let keys = KeyDerivationPolicy::nostr_key(&master, &label).unwrap();
        assert_eq!(keys.public_key().to_hex(), ALICE_NOSTR_PUBLIC);

        let regtest_master = master_key(Network::Regtest);
        let regtest_keys = KeyDerivationPolicy::nostr_key(&regtest_master, &label).unwrap();
        assert_eq!(regtest_keys.public_key().to_hex(), ALICE_NOSTR_PUBLIC);
    }

    #[test]
    fn hash31_of_alice_matches_expected() {
        let label = Label::new("alice").unwrap();
        assert_eq!(KeyDerivationPolicy::hash31(&label), 1_026_581_541);
    }
}
