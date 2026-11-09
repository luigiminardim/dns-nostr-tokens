# application

## Overview
Use cases, one per file, each with its own error enum. They orchestrate `DnsNostrWallet` against a repository implementing `NameTokenWalletRepository` and map `WalletError` and `WalletRepositoryError` into use-case errors carrying the message string.

## Interface
- [`create_wallet.rs`](./create_wallet.rs): `CreateWallet::new(Network, Arc<dyn NameTokenWalletRepository>)`, `execute(&self) -> Result<Mnemonic, CreateWalletError>`; `CreateWalletError { WalletAlreadyExists, Wallet(String) }`.
- [`import_wallet.rs`](./import_wallet.rs): `ImportWallet::new(Network, Arc<dyn NameTokenWalletRepository>)`, `execute(&self, &str) -> Result<(), ImportWalletError>`; `ImportWalletError { InvalidMnemonic(String), WalletAlreadyExists, Wallet(String) }`.
- [`get_receive_address.rs`](./get_receive_address.rs): `GetReceiveAddress::new(Arc<dyn NameTokenWalletRepository>)`, `execute(&self) -> Result<bdk_wallet::AddressInfo, GetReceiveAddressError>`; `GetReceiveAddressError { WalletNotFound, Wallet(String) }`.
- [`mod.rs`](./mod.rs) declares the three modules.

## Invariants
- `CreateWallet` generates a 12-word mnemonic (`MNEMONIC_WORD_COUNT`).
- `ImportWallet` (`parse_mnemonic`) lowercases words and collapses whitespace before `Mnemonic::parse`.
- `WalletRepositoryError::WalletAlreadyExists` maps to `WalletAlreadyExists` in create/import; all other repository and domain errors become `Wallet(String)`.
- `GetReceiveAddress` loads the wallet via `NameTokenWalletRepository::load` (returning `WalletNotFound` if `None`), reveals the next external BIP 84 address, and persists the staged `ChangeSet` via `NameTokenWalletRepository::save`.

## Side Effects
- `CreateWallet` and `ImportWallet` persist the mnemonic and initial wallet state via `NameTokenWalletRepository::create`.
- `GetReceiveAddress` persists the advanced external keychain index via `NameTokenWalletRepository::save`.
- `CreateWallet` consumes OS randomness (`Mnemonic::generate`).

## Verification
- Run: `cargo test -p dns_nostr_wallet`
