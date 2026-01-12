use bdk::bitcoin::Network;
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::bitcoin::util::bip32::{DerivationPath, KeySource};
use bdk::keys::bip39::{Mnemonic, Language, WordCount};
use bdk::keys::{GeneratedKey, GeneratableKey, ExtendedKey, DerivableKey, DescriptorKey};
use bdk::keys::DescriptorKey::Secret;
use bdk::miniscript::miniscript::Segwitv0;
use std::str::FromStr;

fn main() {
    // Create a new secp context
    let secp = Secp256k1::new();

    // No password
    let password = None;

    // Generate a fresh mnemonic, and from there a privatekey
    let mnemonic: GeneratedKey<_, Segwitv0> = Mnemonic::generate((WordCount::Words12, Language::English)).unwrap();
    let mnemonic = mnemonic.into_key();
    let xkey: ExtendedKey = (mnemonic, password).into_extended_key().unwrap();
    let xprv = xkey.into_xprv(Network::Regtest).unwrap();

    // Create derived privkey from the above master privkey
    // We use the following derivation paths for receive and change keys
    // receive: "m/44h/1h/0h/0"
    // change: "m/44h/1h/0h/1"
    let mut keys = Vec::new();

    for path in ["m/44h/1h/0h/0", "m/44h/1h/0h/1"] {
        let deriv_path: DerivationPath = DerivationPath::from_str(path).unwrap();
        let derived_xprv = &xprv.derive_priv(&secp, &deriv_path).unwrap();
        let origin: KeySource = (xprv.fingerprint(&secp), deriv_path);
        let derived_xprv_desc_key: DescriptorKey<Segwitv0> = derived_xprv.into_descriptor_key(Some(origin), DerivationPath::default()).unwrap();

        // Wrap the derived key in the pkh() string to produce a descriptor string
        if let Secret(key, _, _) = derived_xprv_desc_key {
            let mut desc = "pkh(".to_string();
            desc.push_str(&key.to_string());
            desc.push_str(")");
            keys.push(desc);
        }
    }

    // Print the descriptors
    println!("RECEIVE_DESC={}", keys[0]);
    println!("CHANGE_DESC={}", keys[1]);
}