# domain

## Overview
Wallet entity. `DnsNostrWallet` embeds a BDK `PersistedWallet<Connection>` (BIP 84 receive/change keychains) and owns the protocol-specific derivation of Name-Token addresses and per-label Nostr keys. Depends only on `bitcoin`, `bdk_wallet`, `bip39`, `nostr_sdk`, `rusqlite` and `dns_nostr_token::label::Label`.

## Interface
- [`mod.rs`](./mod.rs): declares `pub mod dns_nostr_wallet`.
- [`dns_nostr_wallet.rs`](./dns_nostr_wallet.rs):
  - `DnsNostrWallet::create(&Mnemonic, Network, &mut Connection)`, `DnsNostrWallet::load(&mut Connection) -> Result<Option<Self>, WalletError>`
  - `network()`, `reveal_next_address(&mut Connection) -> AddressInfo`, `peek_address(index) -> AddressInfo`
  - `name_token_address(index) -> Address` (P2PKH), `nostr_keys(&Label) -> Keys`
  - `AddressInfo { index, address, chain }`, `StandardChain { External, Change }`
  - `WalletError { WalletAlreadyExists, BdkCreate, BdkLoad, Persist, Sqlite, Bip32, InvalidNostrSecretKey, InvalidNetwork }`

## Invariants
- Standard addresses use BIP 84 descriptors (`Bip84`, `KeychainKind::External`/`Internal`); `reveal_next_address` and `peek_address` only use the external keychain.
- Name-Token and Nostr keys use path `m/44'/coin'/0'/0x6E616D65/index` (`NAME_TOKEN_PURPOSE`, `NAME_TOKEN_CHAIN`, the ASCII bytes of "name"); coin type is `0'` on `Network::Bitcoin`, `1'` otherwise (`coin_type`).
- A Nostr key's index is `hash31(label)`: first 4 big-endian bytes of the double SHA-256 of the label, masked with `NON_HARDENED_MASK` (non-hardened).
- Only one wallet row exists: table `dns_nostr_wallet` has `id INTEGER PRIMARY KEY CHECK (id = 0)`; `create` returns `WalletAlreadyExists` if a row is present (or BDK data already exists) and never overwrites.
- The master key, seed and mnemonic are private; `Debug` for `DnsNostrWallet` prints only `network`.
- Mnemonic is used with an empty passphrase (`NO_PASSPHRASE`).
- `network_to_string`/`parse_network` accept `mainnet`, `testnet`, `regtest`, `signet`; any other stored value yields `InvalidNetwork`.

## Side Effects
- `create`/`load` run `CREATE TABLE IF NOT EXISTS dns_nostr_wallet (id, mnemonic, network)`.
- `create` writes BDK wallet data and inserts the mnemonic **in plaintext** with the network into the SQLite database.
- `reveal_next_address` persists the incremented keychain index via `PersistedWallet::persist`.
- `load` reads the mnemonic and network rows and the BDK state.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
