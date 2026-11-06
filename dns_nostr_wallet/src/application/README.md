# application

## Overview
Use cases, one per file, each with its own error enum. They orchestrate `DnsNostrWallet` against a caller-supplied `rusqlite::Connection` (the `bdk_wallet::rusqlite` re-export) and map `WalletError` into use-case errors carrying the message string.

## Interface
- [`create_wallet.rs`](./create_wallet.rs): `CreateWallet::new(Network)`, `execute(&mut Connection) -> Result<Mnemonic, CreateWalletError>`; `CreateWalletError { WalletAlreadyExists, Wallet(String) }`.
- [`import_wallet.rs`](./import_wallet.rs): `ImportWallet::new(Network)`, `execute(&mut Connection, &str) -> Result<(), ImportWalletError>`; `ImportWalletError { InvalidMnemonic(String), WalletAlreadyExists, Wallet(String) }`.
- [`get_receive_address.rs`](./get_receive_address.rs): `GetReceiveAddress::new()`, `execute(&mut Connection, Option<u32>) -> Result<AddressInfo, GetReceiveAddressError>`; `GetReceiveAddressError { WalletNotFound, Wallet(String) }`.
- [`mod.rs`](./mod.rs) declares the three modules.

## Invariants
- `CreateWallet` generates a 12-word mnemonic (`MNEMONIC_WORD_COUNT`).
- `ImportWallet` (`parse_mnemonic`) lowercases words and collapses whitespace before `Mnemonic::parse`.
- `WalletError::WalletAlreadyExists` maps to the dedicated variant in create/import; all other errors become `Wallet(String)`.
- `GetReceiveAddress` with `index: None` reveals the next address and persists it; with `Some(i)` it only peeks and does not advance the keychain; it returns `WalletNotFound` if no wallet is stored.

## Side Effects
- `CreateWallet` and `ImportWallet` write the wallet to SQLite via `DnsNostrWallet::create`.
- `GetReceiveAddress` with no index persists the advanced external index.
- `CreateWallet` consumes OS randomness (`Mnemonic::generate`).

## Verification
- Run: `cargo test -p dns_nostr_wallet`
