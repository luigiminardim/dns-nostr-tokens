# domain

## Overview
Pure in-memory wallet entity, stateless key derivation policy, and synchronous repository port. `DnsNostrWallet` wraps an in-memory `bdk_wallet::Wallet` (BIP 84 receive/change keychains) and `master_key: Xpriv`, staging state changes through `bdk_wallet::ChangeSet` without any SQL or `rusqlite` dependency. `KeyDerivationPolicy` encapsulates all BIP 32 / BIP 44 / BIP 84 key derivation rules operating on `master_key: &Xpriv`. `NameTokenWalletRepository` defines the synchronous persistence contract. Depends only on `bitcoin`, `bdk_wallet`, `bip39`, `nostr_sdk` and `dns_nostr_token::label::Label`.

## Interface
- [`mod.rs`](./mod.rs): declares `pub mod dns_nostr_wallet`, `pub mod key_derivation_policy`, `pub mod name_token_wallet_repository`.
- [`dns_nostr_wallet.rs`](./dns_nostr_wallet.rs):
  - `DnsNostrWallet::new(&Mnemonic, Network) -> Result<Self, WalletError>`
  - `DnsNostrWallet::from_loaded(bdk_wallet::Wallet, Xpriv) -> Self`
  - `reveal_next_bitcoin_address(&mut self) -> bdk_wallet::AddressInfo`
  - `staged(&self) -> Option<&bdk_wallet::ChangeSet>`, `take_staged(&mut self) -> Option<bdk_wallet::ChangeSet>`
  - `WalletError { BdkCreate(String), Bip32(bitcoin::bip32::Error) }`
- [`key_derivation_policy.rs`](./key_derivation_policy.rs):
  - `KeyDerivationPolicy::bitcoin_external_descriptor(&Xpriv) -> Bip84<Xpriv>`
  - `KeyDerivationPolicy::bitcoin_internal_descriptor(&Xpriv) -> Bip84<Xpriv>`
  - `KeyDerivationPolicy::name_token_utxo_key(&Xpriv, u32) -> Result<Xpriv, KeyDerivationError>`
  - `KeyDerivationPolicy::nostr_key(&Xpriv, &Label) -> Result<nostr_sdk::Keys, KeyDerivationError>`
  - `KeyDerivationError { InvalidChildIndex(u32), Bip32(bitcoin::bip32::Error), InvalidNostrSecretKey }`
- [`name_token_wallet_repository.rs`](./name_token_wallet_repository.rs):
  - `NameTokenWalletRepository` trait: `create(&self, &Mnemonic, &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>`, `save(&self, &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>`, `load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError>`
  - `WalletRepositoryError { WalletAlreadyExists, CorruptedState(String), Storage(String) }`

## Invariants
- Standard Bitcoin addresses use BIP 84 descriptors (`Bip84<Xpriv>`, `KeychainKind::External`/`Internal`); `reveal_next_bitcoin_address` reveals from the external keychain and stages the index update in `bdk_wallet`.
- Name-Token UTXO keys use path `m/44'/coin'/0'/0x6E616D65/index` (`NAME_TOKEN_PURPOSE`, `NAME_TOKEN_CHAIN`, the ASCII bytes of `"name"`), where `coin'` is `0'` on `NetworkKind::Main` and `1'` on `NetworkKind::Test`, and `index` must be non-hardened (`< 2^31`, otherwise `KeyDerivationError::InvalidChildIndex`).
- Nostr keys use path `m/44'/1237'/0'/0x6E616D65/hash31(label)` (`NOSTR_COIN_TYPE = 1237` per SLIP-0044 / NIP-06), where `KeyDerivationPolicy::hash31(label)` is the first 4 big-endian bytes of the double SHA-256 of the label, masked with `NON_HARDENED_MASK` (non-hardened).
- The master key, descriptors and seed are private; `Debug` for `DnsNostrWallet` prints only `network`.
- Mnemonic is used with an empty passphrase (`NO_PASSPHRASE`).

## Side Effects
- None. All operations in `domain` are pure and in-memory.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
