# Re-architect `dns_nostr_wallet` Domain, Key Derivation Policy, and Repository (Issue #34)

*   **Author:** Luigi Minardi
*   **Status:** Proposed
*   **Date:** 2026-11-08

--------------------------------------------------------------------------------

## Overview

This design refactors the `dns_nostr_wallet` crate (GitHub Issue #34) so that its domain layer adheres to Clean Architecture and Domain-Driven Design boundaries. Currently, `DnsNostrWallet` embeds `bdk_wallet::PersistedWallet<rusqlite::Connection>`, executes SQL statements inside domain methods, and mixes wallet state management with BIP 32 / BIP 44 / BIP 84 key derivation rules.

This change separates those responsibilities into three distinct components:
1.  **In-Memory Wallet Entity (`DnsNostrWallet`)**: Wraps an in-memory `bdk_wallet::Wallet` and `master_key: Xpriv`, stages state changes through `bdk_wallet::ChangeSet`, and has zero SQL or `rusqlite` dependencies.
2.  **Stateless `KeyDerivationPolicy` (`key_derivation_policy.rs`)**: Encapsulates all protocol key derivation rules (`bitcoin_external_descriptor`, `bitcoin_internal_descriptor`, `name_token_utxo_key`, and `nostr_key`) operating uniformly on `master_key: &Xpriv`.
3.  **Synchronous `NameTokenWalletRepository` Port & `SqliteNameTokenWalletRepository` Adapter**: Moves all SQLite persistence (the `Mnemonic` table and BDK's `ChangeSet` tables) into the adapters layer behind a synchronous repository trait. Writes execute inside a single `rusqlite::Transaction`, and the wallet's in-memory staged `ChangeSet` is only cleared (`take_staged()`) after the SQLite transaction commits, so a failed write neither partially mutates SQLite nor discards uncommitted in-memory state.

## The Problem

In the current `dns_nostr_wallet` implementation (`dns_nostr_wallet/src/domain/dns_nostr_wallet.rs`), the domain layer violates the Dependency Rule and mixes multiple responsibilities in a single file:

1.  **SQLite Leakage into the Domain Layer**: `DnsNostrWallet` imports `bdk_wallet::rusqlite::{params, Connection}`, embeds `PersistedWallet<Connection>`, creates the `dns_nostr_wallet` SQLite table directly, and requires `&mut Connection` on `create`, `load`, and `reveal_next_address`. Furthermore, `WalletError` exposes `Sqlite(rusqlite::Error)`. As a result, the domain model cannot be instantiated or tested without a SQLite connection, and application use cases (`CreateWallet`, `ImportWallet`, `GetReceiveAddress`) coupled to `&mut Connection` must be passed a shared `Arc<Mutex<Connection>>` from `CliController`.
2.  **Coupled Key Derivation Rules**: Protocol-specific derivation rules for BIP 84 descriptors (`m/84'/coin'/0'/{0,1}/*`), Name-Token UTXO keys (`m/44'/coin'/0'/0x6E616D65/i`), and per-label Nostr keys (`m/44'/1237'/0'/0x6E616D65/hash31(label)`) are embedded directly inside `dns_nostr_wallet.rs` alongside persistence logic instead of residing in a dedicated, independently testable domain policy.
3.  **Unnecessary Custom Wrapper Types and Unused API Surface**: `dns_nostr_wallet.rs` defines custom `AddressInfo` and `StandardChain` types that duplicate `bdk_wallet::AddressInfo` and `bdk_wallet::KeychainKind`, and exposes `peek_address` and `--index` on `wallet address`, diverging from `bdk_wallet::Wallet::reveal_next_address` semantics.
4.  **Non-Atomic Dual-Table Writes**: Currently, `DnsNostrWallet::create` writes BDK tables via `create_wallet(conn)` and then executes a separate `INSERT INTO dns_nostr_wallet` statement outside a shared transaction; if the second write fails, the BDK tables and the mnemonic table become inconsistent. To ensure atomic writes, `SqliteNameTokenWalletRepository` must execute all writes to the `dns_nostr_wallet` mnemonic table and BDK's `bdk_*` (`ChangeSet`) tables inside a single `rusqlite::Transaction`, and only clear the wallet's in-memory staged `ChangeSet` (`wallet.take_staged()`) after `Transaction::commit()` succeeds.

## Goals

*   Remove all `rusqlite` imports, SQL queries, and `PersistedWallet<Connection>` usage from `dns_nostr_wallet/src/domain/`.
*   Refactor the wallet entity in `dns_nostr_wallet/src/domain/` to hold an in-memory `bdk_wallet::Wallet` and `master_key: Xpriv`, stage mutations via `bdk_wallet::ChangeSet`, redact secret material in `fmt::Debug`, and expose `reveal_next_bitcoin_address(&mut self) -> bdk_wallet::AddressInfo`.
*   Remove unused methods and custom wrapper types from the wallet entity (`peek_address`, `network`, `name_token_address`, `nostr_keys`, `AddressInfo`, `StandardChain`).
*   Extract all derivation path construction and key derivation into a stateless `KeyDerivationPolicy` in `dns_nostr_wallet/src/domain/key_derivation_policy.rs` where all four methods accept `master_key: &Xpriv`:
    *   `bitcoin_external_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`
    *   `bitcoin_internal_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`
    *   `name_token_utxo_key(master_key: &Xpriv, index: u32) -> Result<Xpriv, KeyDerivationError>`
    *   `nostr_key(master_key: &Xpriv, label: &Label) -> Result<nostr_sdk::Keys, KeyDerivationError>`
*   Define a synchronous `NameTokenWalletRepository` trait in `dns_nostr_wallet/src/domain/` (`create`, `save`, `load`) and implement `SqliteNameTokenWalletRepository` in `dns_nostr_wallet/src/adapters/` with atomic single-transaction writes across the mnemonic table and BDK's `ChangeSet` tables, clearing the wallet's in-memory staged `ChangeSet` only after `rusqlite::Transaction::commit` succeeds.
*   Load public descriptors from BDK's `ChangeSet` on `load` (`bdk_wallet::Wallet::load().load_wallet_no_persist(changeset)`) while reconstructing `master_key: Xpriv` on the wallet entity from the stored `Mnemonic` and `ChangeSet` network, returning an explicit corruption error if the mnemonic row and BDK `ChangeSet` are out of sync.
*   Simplify `CreateWallet`, `ImportWallet`, `GetReceiveAddress`, and `CliController` to depend on `NameTokenWalletRepository` instead of `Arc<Mutex<Connection>>`, and remove `--index` from `WalletCommand::Address` and `index: Option<u32>` from `GetReceiveAddress::execute`.
*   Update `docs/domain.md`, `AGENTS.md`, `README.md`, and all `dns_nostr_wallet/**/README.md` directory contracts to match the refactored architecture.

## Non-Goals

*   Blockchain synchronization, UTXO scanning, balance queries, or transaction building and broadcasting (tracked in Issue #35).
*   On-chain Name-Token minting, updating, transferring, or revoking (tracked in Issues #36–#40).
*   Publishing DNS records to Nostr relays from the wallet (tracked in Issue #37).
*   Encrypted at-rest storage or OS-keyring integration for the stored BIP39 mnemonic.
*   Introducing an asynchronous runtime (`tokio` or `async-trait`) into `dns_nostr_wallet`.

## Open Questions

None. All architectural and interface questions were resolved during the brainstorm phase (`concepts_brief.md`).

## Critical User Journeys (CUJs)

### CUJ 1: Create a New Wallet (`wallet create`)

*   **Given**: No wallet exists in `wallet.sqlite` in the configured `DNS_NOSTR_WALLET_DIR` for the configured `DNS_NOSTR_NETWORK`.
*   **When**: The user runs `dns_nostr_wallet wallet create`.
*   **Then**: `CreateWallet` generates a 12-word BIP39 `Mnemonic`, initializes an in-memory wallet with `master_key: Xpriv` and BIP 84 descriptors from `KeyDerivationPolicy`, and calls `NameTokenWalletRepository::create`. `SqliteNameTokenWalletRepository` writes both the `Mnemonic` and the initial BDK `ChangeSet` (`wallet.staged()`) inside a single `rusqlite::Transaction`, commits the transaction, and only clears the in-memory staged `ChangeSet` (`wallet.take_staged()`) after commit succeeds. The CLI then prints the 12-word seed phrase. Running `wallet create` a second time returns `WalletAlreadyExists` without modifying the database.

### CUJ 2: Import an Existing Wallet (`wallet import`)

*   **Given**: No wallet exists in `wallet.sqlite`, and the user has a valid 12-to-24-word BIP39 seed phrase passed via CLI arguments or `stdin`.
*   **When**: The user runs `dns_nostr_wallet wallet import [WORDS...]`.
*   **Then**: `ImportWallet` normalizes whitespace and case, validates the `Mnemonic`, constructs the wallet entity in memory, and calls `NameTokenWalletRepository::create` to atomically store the `Mnemonic` and initial BDK `ChangeSet` in a single SQLite transaction before clearing the staged `ChangeSet`. If the phrase is invalid or the transaction fails, no database rows are committed and the error message never echoes the seed words.

### CUJ 3: Reveal the Next Bitcoin Receive Address (`wallet address`)

*   **Given**: A wallet has been created or imported in `wallet.sqlite`.
*   **When**: The user runs `dns_nostr_wallet wallet address`.
*   **Then**: `GetReceiveAddress` loads the wallet via `NameTokenWalletRepository::load` (which reads the `Mnemonic` and BDK `ChangeSet`, validates neither is missing while the other is present, loads public descriptors into `bdk_wallet::Wallet`, and derives `master_key: Xpriv`), calls `wallet.reveal_next_bitcoin_address()`, and persists the staged `ChangeSet` via `NameTokenWalletRepository::save` inside a `rusqlite::Transaction`, clearing the wallet's staged `ChangeSet` only after the transaction commits. The CLI prints the newly revealed BIP 84 P2WPKH receive address, and subsequent `wallet address` invocations advance the external keychain index (`0`, `1`, `2`, ...).

## Technical Plan (Architecture)

### Layer Boundaries and Dependency Direction

All source-code dependencies in `dns_nostr_wallet` point inward (`adapters` → `application` → `domain`), with `main.rs` acting as the composition root:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                         main.rs (Composition Root)                      │
│  Parses Cli & EnvironmentConfig, opens rusqlite::Connection, constructs │
│  SqliteNameTokenWalletRepository, Use Cases, and CliController          │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                                adapters/                                │
│  • cli.rs: Cli, Command, WalletCommand { Create, Import, Address },     │
│            CliController                                                │
│  • sqlite_name_token_wallet_repository.rs:                              │
│            SqliteNameTokenWalletRepository (implements                  │
│            NameTokenWalletRepository over rusqlite::Connection)         │
│  • environment_config.rs: EnvironmentConfig                             │
└──────────────────┬───────────────────────────────────┬──────────────────┘
                   │ calls                             │ implements
                   ▼                                   ▼
┌──────────────────────────────────────┐   ┌──────────────────────────────┐
│             application/             │   │           domain/            │
│  • CreateWallet                      │──▶│  • NameTokenWalletRepository │
│  • ImportWallet                      │   │    (trait + error enum)      │
│  • GetReceiveAddress                 │   │  • DnsNostrWallet            │
│                                      │──▶│    (in-memory entity)        │
└──────────────────────────────────────┘   │  • KeyDerivationPolicy       │
                                           │    (stateless BIP32/44/84)   │
                                           └──────────────────────────────┘
```

### 1. Stateless `KeyDerivationPolicy` (`dns_nostr_wallet/src/domain/key_derivation_policy.rs`)

All protocol-specific derivation constants, path builders, and key derivation functions move out of `dns_nostr_wallet.rs` into `key_derivation_policy.rs`. `KeyDerivationPolicy` is a zero-field struct whose methods operate on `master_key: &Xpriv`:

*   **Constants**:
    *   `NON_HARDENED_MASK: u32 = 0x7FFF_FFFF`
    *   `NAME_TOKEN_PURPOSE: u32 = 44`
    *   `NOSTR_COIN_TYPE: u32 = 1237` (SLIP-0044 / NIP-06 coin type for Nostr)
    *   `FIRST_ACCOUNT: u32 = 0`
    *   `NAME_TOKEN_CHAIN: u32 = 0x6E61_6D65` (ASCII `"name"`, isolating Name-Token UTXOs and Nostr keys from standard BIP 44 wallets)
*   **Methods**:
    *   `pub fn bitcoin_external_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`: Returns `Bip84(*master_key, KeychainKind::External)` (`m/84'/coin'/0'/0/*`).
    *   `pub fn bitcoin_internal_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`: Returns `Bip84(*master_key, KeychainKind::Internal)` (`m/84'/coin'/0'/1/*`).
    *   `pub fn name_token_utxo_key(master_key: &Xpriv, index: u32) -> Result<Xpriv, KeyDerivationError>`: Derives the child `Xpriv` at `m/44'/coin'/0'/0x6E616D65/index`, where `bitcoin_coin_type(master_key.network)` is `0` on `NetworkKind::Main` and `1` on `NetworkKind::Test`, and `index` is validated as a normal (non-hardened, `< 2^31`) child index.
    *   `pub fn nostr_key(master_key: &Xpriv, label: &Label) -> Result<nostr_sdk::Keys, KeyDerivationError>`: Computes `hash31(label)` (the first 4 big-endian bytes of `sha256d(label.as_bytes())` masked with `0x7FFF_FFFF`), derives the child `Xpriv` at `m/44'/1237'/0'/0x6E616D65/hash31(label)` using SLIP-0044 coin type `1237'`, converts `derived.private_key.secret_bytes()` into `nostr_sdk::SecretKey`, and returns `nostr_sdk::Keys::new(secret_key)`.
*   **Error Type**:
    *   `KeyDerivationError`:
        *   `InvalidChildIndex(u32)` (when `index >= 0x8000_0000`)
        *   `Bip32(bitcoin::bip32::Error)`
        *   `InvalidNostrSecretKey`

### 2. In-Memory `DnsNostrWallet` Entity (`dns_nostr_wallet/src/domain/dns_nostr_wallet.rs`)

`DnsNostrWallet` is refactored into a pure in-memory entity with zero `rusqlite` or SQL dependencies:

*   **Fields**:
    *   `bdk_wallet: bdk_wallet::Wallet`
    *   `master_key: Xpriv`
*   **Constructors and Methods**:
    *   `pub fn new(mnemonic: &Mnemonic, network: Network) -> Result<Self, WalletError>`: Derives `seed = mnemonic.to_seed("")` and `master_key = Xpriv::new_master(network, &seed)?`, constructs `bdk_wallet` via `bdk_wallet::Wallet::create(KeyDerivationPolicy::bitcoin_external_descriptor(&master_key), KeyDerivationPolicy::bitcoin_internal_descriptor(&master_key)).network(network).create_wallet_no_persist()`, and returns `Self { bdk_wallet, master_key }` with the initial creation `ChangeSet` staged inside `bdk_wallet`.
    *   `pub fn from_loaded(bdk_wallet: bdk_wallet::Wallet, master_key: Xpriv) -> Self`: Constructs `DnsNostrWallet` from a `bdk_wallet::Wallet` loaded from a persisted `ChangeSet` and the `master_key` derived from the stored `Mnemonic`.
    *   `pub fn reveal_next_bitcoin_address(&mut self) -> bdk_wallet::AddressInfo`: Calls `self.bdk_wallet.reveal_next_address(KeychainKind::External)`, which advances the external keychain index and stages the index update in `self.bdk_wallet`'s internal `ChangeSet`.
    *   `pub fn staged(&self) -> Option<&bdk_wallet::ChangeSet>`: Returns an immutable reference to the staged `ChangeSet` in `self.bdk_wallet` without draining it.
    *   `pub fn take_staged(&mut self) -> Option<bdk_wallet::ChangeSet>`: Takes and clears the staged `ChangeSet` from `self.bdk_wallet` after a repository transaction commits.
*   **Removed API Surface**:
    *   Removes `peek_address`, `network`, `name_token_address`, `nostr_keys`, custom `AddressInfo`, and `StandardChain`.
*   **Secret Redaction (`fmt::Debug`)**:
    *   `impl fmt::Debug for DnsNostrWallet` outputs `DnsNostrWallet { network: self.bdk_wallet.network(), .. }` via `finish_non_exhaustive()`, redacting `master_key` and `bdk_wallet`.
*   **Error Type**:
    *   `WalletError`:
        *   `BdkCreate(String)`
        *   `Bip32(bitcoin::bip32::Error)`

### 3. Synchronous `NameTokenWalletRepository` Port (`dns_nostr_wallet/src/domain/name_token_wallet_repository.rs`)

Defines the synchronous persistence boundary in `domain/`:

*   **Trait Definition**:
    *   `fn create(&self, mnemonic: &Mnemonic, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>`
    *   `fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>`
    *   `fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError>`
    *   Blanket implementation `impl<R: NameTokenWalletRepository + ?Sized> NameTokenWalletRepository for Arc<R>` so a single repository instance can be shared across `CreateWallet`, `ImportWallet`, and `GetReceiveAddress`.
*   **Error Type (`WalletRepositoryError`)**:
    *   `WalletAlreadyExists`
    *   `CorruptedState(String)`
    *   `Storage(String)`

### 4. `SqliteNameTokenWalletRepository` Adapter (`dns_nostr_wallet/src/adapters/sqlite_name_token_wallet_repository.rs`)

Implements `NameTokenWalletRepository` over a `rusqlite::Connection` encapsulated inside `Mutex<Connection>`:

*   **Schema**:
    *   Mnemonic table: `CREATE TABLE IF NOT EXISTS dns_nostr_wallet (id INTEGER PRIMARY KEY CHECK (id = 0), mnemonic TEXT NOT NULL)`
    *   BDK tables: initialized and managed by `bdk_wallet::ChangeSet::init_sqlite_tables(&tx)` and `changeset.persist_to_sqlite(&tx)` (`bdk_descriptor`, `bdk_network`, `bdk_keychain`, etc.). Because `bdk_network` already stores the wallet's `Network`, the `dns_nostr_wallet` table does not duplicate the `network` column.
*   **Atomic Write Flow (`create`)**:
    1.  Begins a `rusqlite::Transaction` (`conn.transaction()`).
    2.  Initializes `dns_nostr_wallet` and BDK tables (`ChangeSet::init_sqlite_tables(&tx)`).
    3.  Checks whether row `id = 0` exists in `dns_nostr_wallet` or whether `ChangeSet::from_sqlite(&tx)` is non-empty; if either exists, returns `Err(WalletRepositoryError::WalletAlreadyExists)` (rolling back the transaction on drop).
    4.  Inserts `(0, mnemonic.to_string())` into `dns_nostr_wallet`.
    5.  Reads `wallet.staged()` by reference and calls `changeset.persist_to_sqlite(&tx)`.
    6.  Commits the SQLite transaction (`tx.commit()`).
    7.  Calls `wallet.take_staged()` only after `tx.commit()` succeeds.
*   **Atomic Write Flow (`save`)**:
    1.  Reads `wallet.staged()`. If `None` (or empty), returns `Ok(())`.
    2.  Begins a `rusqlite::Transaction` (`conn.transaction()`).
    3.  Verifies that the wallet row `id = 0` exists in `dns_nostr_wallet` (returning `WalletRepositoryError::CorruptedState` if missing).
    4.  Calls `changeset.persist_to_sqlite(&tx)`.
    5.  Commits the SQLite transaction (`tx.commit()`).
    6.  Calls `wallet.take_staged()` only after `tx.commit()` succeeds. If `persist_to_sqlite` or `commit` fails, `tx` rolls back and `wallet.staged()` remains untouched in memory.
*   **Consistent Read Flow (`load`)**:
    1.  Begins a `rusqlite::Transaction` (`conn.transaction()`).
    2.  Initializes `dns_nostr_wallet` and BDK tables if absent.
    3.  Queries `SELECT mnemonic FROM dns_nostr_wallet WHERE id = 0` (`Option<String>`) and reads `let changeset = ChangeSet::from_sqlite(&tx)?`.
    4.  Validates dual-table consistency:
        *   If `mnemonic_row.is_none()` and `changeset.is_empty()`, returns `Ok(None)`.
        *   If `mnemonic_row.is_some()` and `changeset.is_empty()`, or `mnemonic_row.is_none()` and `!changeset.is_empty()`, returns `Err(WalletRepositoryError::CorruptedState(...))`.
    5.  Parses `Mnemonic::parse(&mnemonic_str)` (mapping parse failure to `CorruptedState`), extracts `network = changeset.network.ok_or_else(|| CorruptedState(...))?`, and loads `bdk_wallet` using `bdk_wallet::Wallet::load().load_wallet_no_persist(changeset)` (which loads the persisted public `xpub` descriptors without extracting private keys into BDK).
    6.  Derives `master_key = Xpriv::new_master(network, &mnemonic.to_seed(""))` and returns `Ok(Some(DnsNostrWallet::from_loaded(bdk_wallet, master_key)))`.

### 5. Application Use Cases and CLI Controller (`application/` & `adapters/cli.rs`)

*   **`CreateWallet<R: NameTokenWalletRepository>`**: Holds `repository: R` and `network: Network`. `execute(&self) -> Result<Mnemonic, CreateWalletError>` generates a 12-word `Mnemonic`, calls `DnsNostrWallet::new(&mnemonic, self.network)?`, persists via `self.repository.create(&mnemonic, &mut wallet)?`, and returns `Ok(mnemonic)`.
*   **`ImportWallet<R: NameTokenWalletRepository>`**: Holds `repository: R` and `network: Network`. `execute(&self, phrase: &str) -> Result<(), ImportWalletError>` normalizes and parses `phrase` into `Mnemonic`, calls `DnsNostrWallet::new(&mnemonic, self.network)?`, and persists via `self.repository.create(&mnemonic, &mut wallet)?`.
*   **`GetReceiveAddress<R: NameTokenWalletRepository>`**: Holds `repository: R`. `execute(&self) -> Result<bdk_wallet::AddressInfo, GetReceiveAddressError>` loads the wallet via `self.repository.load()?.ok_or(GetReceiveAddressError::WalletNotFound)?`, calls `let info = wallet.reveal_next_bitcoin_address()`, persists via `self.repository.save(&mut wallet)?`, and returns `Ok(info)`.
*   **`CliController`**: No longer holds `Arc<Mutex<Connection>>`. `WalletCommand::Address` removes the `--index` flag (`WalletCommand::Address` unit variant), and `CliController::address` calls `self.get_receive_address.execute()` with no arguments.
*   **`Cargo.toml`**: Removes `"keys-bip39"` from `bdk_wallet` features (since `KeyDerivationPolicy` passes `Xpriv` directly to `Bip84<Xpriv>` rather than `bip39::Mnemonic`).

## Alternatives Considered

### Alternative A: Embedding `bdk_wallet::PersistedWallet<rusqlite::Connection>` Directly in `DnsNostrWallet`

*   **Pros**: Avoids defining a separate `NameTokenWalletRepository` trait and adapter; uses BDK's built-in `create_wallet(conn)`, `load_wallet(conn)`, and `persist(conn)` methods directly inside the entity.
*   **Cons**: Forces the domain layer to depend on `rusqlite::Connection` and execute SQL statements directly; prevents testing `DnsNostrWallet` or use cases without a SQLite database; leaks `rusqlite::Error` into `WalletError`.
*   **Reason for Rejection**: Rejected by author: domain entities must not contain SQL code or depend directly on `rusqlite::Connection`; persistence must be abstracted behind a repository port in `domain/` and implemented in `adapters/`.

### Alternative B: Extracting Private Keys into `bdk_wallet::Wallet` on Every `load`

*   **Pros**: Populates `bdk_wallet::Wallet` with private key descriptors on every load via `.descriptor(KeychainKind::External, Some(Bip84(master_key, KeychainKind::External)))` so BDK's internal signers are immediately initialized.
*   **Cons**: Re-derives and loads private descriptors into BDK on every read even for read-only or address-revelation operations where public `xpub` descriptors in `ChangeSet` suffice.
*   **Reason for Rejection**: Rejected by author: keeping public descriptors in `bdk_wallet::Wallet` on `load` (`bdk_wallet::Wallet::load().load_wallet_no_persist(changeset)`) and holding `master_key: Xpriv` on `DnsNostrWallet` is sufficient until transaction signing is built.

### Alternative C: Making `NameTokenWalletRepository` and Wallet Use Cases `async`

*   **Pros**: Matches the `async` trait signature used by `NameTokenRepository` in the `name_token` crate (`dns_nostr_server`).
*   **Cons**: Adds `tokio` and `async-trait` (or `impl Future`) overhead to `dns_nostr_wallet/Cargo.toml` when all CLI commands, BDK operations, and `rusqlite` calls in `dns_nostr_wallet` are synchronous.
*   **Reason for Rejection**: Rejected by author: all CLI, BDK, and SQLite operations in `dns_nostr_wallet` are synchronous, so a synchronous repository port avoids unnecessary async runtime dependencies.

### Alternative D: Exposing Per-Index `bitcoin_external_key` and `bitcoin_internal_key` Functions on `KeyDerivationPolicy`

*   **Pros**: Makes all four methods on `KeyDerivationPolicy` return individual derived keys for a caller-supplied `index: u32`.
*   **Cons**: Conflicts with `bdk_wallet::Wallet::create`, which requires wildcard descriptor templates (`Bip84<Xpriv>` representing `m/84'/coin'/0'/0/*` and `m/84'/coin'/0'/1/*`) so BDK can track and advance keychain indices internally.
*   **Reason for Rejection**: Rejected by author: `bdk_wallet::Wallet` requires wildcard descriptors (`0/*` and `1/*`) to manage standard Bitcoin keychains and index advancement internally.

### Alternative E: Passing `Mnemonic` to `KeyDerivationPolicy`

*   **Pros**: Allows `KeyDerivationPolicy` to construct `Bip84(mnemonic, KeychainKind::External)` directly using BDK's `keys-bip39` feature.
*   **Cons**: Forces `KeyDerivationPolicy` to repeatedly convert `Mnemonic` to a seed and master `Xpriv` on every call (`name_token_utxo_key`, `nostr_key`), and couples `KeyDerivationPolicy` to `bip39::Mnemonic` instead of operating uniformly on `bitcoin::bip32::Xpriv`.
*   **Reason for Rejection**: Rejected by author: `KeyDerivationPolicy` operates uniformly on `master_key: &Xpriv`, and `name_token_utxo_key` returns `Xpriv`.

### Alternative F: Using BDK Descriptors Inside `bdk_wallet::Wallet` for Name-Token UTXOs

*   **Pros**: Tracks Name-Token UTXOs inside BDK's keychain tables alongside standard Bitcoin UTXOs.
*   **Cons**: `bdk_wallet::Wallet` supports at most two keychains (`KeychainKind::External` and `KeychainKind::Internal`), which are already dedicated to BIP 84 standard receive and change funds; additionally, Name-Token outputs prepend an `OP_FALSE OP_IF … OP_ENDIF` inscription envelope to the P2PKH script that standard BDK descriptor templates cannot express.
*   **Reason for Rejection**: Rejected by author: `bdk_wallet::Wallet` is limited to two keychains (`External` and `Internal`) used for BIP 84 funds, and Name-Token outputs use a custom inscription script envelope on chain `0x6E616D65`.

### Alternative G: Keeping Optional `index: Option<u32>` on `reveal_next_bitcoin_address` and `wallet address --index`

*   **Pros**: Preserves the ability to peek at an arbitrary external address index from the CLI without advancing the persisted keychain counter.
*   **Cons**: Diverges from `bdk_wallet::Wallet::reveal_next_address` semantics and maintains unused address-peeking API surface in the entity, use case, and CLI.
*   **Reason for Rejection**: Rejected by author: removing `--index` and `peek_address` aligns `DnsNostrWallet::reveal_next_bitcoin_address(&mut self)` with `bdk_wallet::Wallet::reveal_next_address` and eliminates unused API surface.

### Alternative H: Unifying `create` and `save` into One Method or Calling `take_staged()` Before `Transaction::commit()`

*   **Pros**: A single `save` method reduces the `NameTokenWalletRepository` trait from three methods to two; calling `wallet.take_staged()` before writing to SQLite avoids exposing both `staged()` and `take_staged()`.
*   **Cons**: Combining `create` and `save` prevents the repository from enforcing the `WalletAlreadyExists` invariant when `CreateWallet` or `ImportWallet` is run against an already-initialized database. Draining `take_staged()` before `Transaction::commit()` loses the in-memory staged `ChangeSet` if the SQLite write or commit fails.
*   **Reason for Rejection**: Rejected by author: separating `create` from `save` enforces `WalletAlreadyExists` at the repository boundary, and reading `wallet.staged()` before `tx.commit()` while calling `wallet.take_staged()` only after `tx.commit()` succeeds guarantees that failed transactions leave both SQLite and in-memory staged state intact.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security & Secret Redaction**:
    *   `DnsNostrWallet` implements a custom `fmt::Debug` that outputs only `network` and redacts `master_key` and `bdk_wallet` so private keys, descriptors, and seed material never appear in debug logs or panic messages.
    *   `ImportWallet` never echoes the user-supplied seed phrase in `ImportWalletError::InvalidMnemonic`.
    *   On Unix systems, `main.rs` sets `<wallet_dir>/wallet.sqlite` file permissions to `0o600`.
*   **Atomicity & Data Integrity**:
    *   All multi-table writes (`dns_nostr_wallet` and BDK's `bdk_*` tables) execute inside a single `rusqlite::Transaction`.
    *   `wallet.take_staged()` is called only after `Transaction::commit()` returns `Ok(())`.
    *   `SqliteNameTokenWalletRepository::load` detects partial/corrupted state (mnemonic row present without BDK `ChangeSet`, or BDK `ChangeSet` present without mnemonic row) and returns `Err(WalletRepositoryError::CorruptedState(...))` rather than silently returning `Ok(None)`.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `dns_nostr_wallet/Cargo.toml` (Modify)

*   **Type / Scope**: Crate Manifest (`Cargo.toml`)
*   **Rationale & Proposed Changes**: Remove `"keys-bip39"` from the `bdk_wallet` feature list (`bdk_wallet = { version = "1.0", features = ["rusqlite"] }`) because `KeyDerivationPolicy` constructs `Bip84<Xpriv>` from `master_key: &Xpriv` rather than passing `bip39::Mnemonic` into BDK descriptor templates.

#### 2. `dns_nostr_wallet/src/domain/mod.rs` (Modify)

*   **Type / Scope**: Domain Module Index
*   **Rationale & Proposed Changes**: Expose the new domain modules alongside `dns_nostr_wallet`:
    *   `pub mod dns_nostr_wallet;`
    *   `pub mod key_derivation_policy;`
    *   `pub mod name_token_wallet_repository;`

#### 3. `dns_nostr_wallet/src/domain/key_derivation_policy.rs` (Create)

*   **Type / Scope**: Domain Policy Module + Unit Tests (`mod test_key_derivation_policy`)
*   **Rationale & Proposed Changes**: Extract all BIP 32 / BIP 44 / BIP 84 derivation constants, path construction helpers, and key derivation logic from `dns_nostr_wallet.rs` into a stateless policy operating on `master_key: &Xpriv`.
    *   Define constants `NON_HARDENED_MASK` (`0x7FFF_FFFF`), `NAME_TOKEN_PURPOSE` (`44`), `NOSTR_COIN_TYPE` (`1237`), `FIRST_ACCOUNT` (`0`), and `NAME_TOKEN_CHAIN` (`0x6E61_6D65`).
    *   Define `KeyDerivationError` (`InvalidChildIndex(u32)`, `Bip32(bitcoin::bip32::Error)`, `InvalidNostrSecretKey`) implementing `Debug, Clone, PartialEq, Eq, fmt::Display, std::error::Error`.
    *   Define `KeyDerivationPolicy` with:
        *   `pub fn bitcoin_external_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`
        *   `pub fn bitcoin_internal_descriptor(master_key: &Xpriv) -> Bip84<Xpriv>`
        *   `pub fn name_token_utxo_key(master_key: &Xpriv, index: u32) -> Result<Xpriv, KeyDerivationError>`
        *   `pub fn nostr_key(master_key: &Xpriv, label: &Label) -> Result<nostr_sdk::Keys, KeyDerivationError>`
    *   Add unit tests in `mod test_key_derivation_policy` verifying:
        *   BIP 84 external and internal descriptors produce the known mainnet (`bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu`) and regtest (`bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk`) receive addresses when loaded into `bdk_wallet::Wallet`.
        *   `name_token_utxo_key` at index `0` derives the expected P2PKH public key / address on mainnet (`1M9PnNAN8XcKsrVtGPGSh8wi6n3eJyTXqe`) and regtest (`mtZEqSJy671ckaEjDptKD6xu6yzRsxcZsH`), and returns `Err(KeyDerivationError::InvalidChildIndex(index))` when `index >= 0x8000_0000`.
        *   `nostr_key` for label `"alice"` (`hash31("alice") == 1_026_581_541`) at `m/44'/1237'/0'/0x6E616D65/hash31("alice")` produces public key `c1d71f9bb7fcff517470d27828cf92182b57444636f117dd6a65a00b0a181f25`.

#### 4. `dns_nostr_wallet/src/domain/dns_nostr_wallet.rs` (Modify)

*   **Type / Scope**: Domain Entity Module + Unit Tests (`mod test_dns_nostr_wallet`)
*   **Rationale & Proposed Changes**: Remove all `rusqlite` imports, SQL queries, `PersistedWallet<Connection>`, `AddressInfo`, `StandardChain`, `peek_address`, `network`, `name_token_address`, and `nostr_keys` so `DnsNostrWallet` is a pure in-memory entity.
    *   Fields: `bdk_wallet: bdk_wallet::Wallet` and `master_key: Xpriv`.
    *   `WalletError` reduced to `BdkCreate(String)` and `Bip32(bitcoin::bip32::Error)` implementing `Debug, Clone, PartialEq, Eq, fmt::Display, std::error::Error`.
    *   Methods:
        *   `pub fn new(mnemonic: &Mnemonic, network: Network) -> Result<Self, WalletError>`
        *   `pub fn from_loaded(bdk_wallet: bdk_wallet::Wallet, master_key: Xpriv) -> Self`
        *   `pub fn reveal_next_bitcoin_address(&mut self) -> bdk_wallet::AddressInfo`
        *   `pub fn staged(&self) -> Option<&bdk_wallet::ChangeSet>`
        *   `pub fn take_staged(&mut self) -> Option<bdk_wallet::ChangeSet>`
    *   Custom `fmt::Debug` printing `DnsNostrWallet { network: self.bdk_wallet.network(), .. }`.
    *   Update `mod test_dns_nostr_wallet` to run completely in memory without SQLite: verify `Debug` secret redaction, initial `staged()` `ChangeSet` presence on `new`, `take_staged()` clearing the staged `ChangeSet`, and `reveal_next_bitcoin_address()` advancing indices (`0`, `1`, ...) on mainnet and regtest and staging a non-empty `ChangeSet`.

#### 5. `dns_nostr_wallet/src/domain/name_token_wallet_repository.rs` (Create)

*   **Type / Scope**: Domain Repository Port Module
*   **Rationale & Proposed Changes**: Define the synchronous repository abstraction and error type in the domain layer so application use cases do not depend on `rusqlite`.
    *   Define `WalletRepositoryError`:
        *   `WalletAlreadyExists`
        *   `CorruptedState(String)`
        *   `Storage(String)`
        implementing `Debug, Clone, PartialEq, Eq, fmt::Display, std::error::Error`.
    *   Define `pub trait NameTokenWalletRepository`:
        *   `fn create(&self, mnemonic: &Mnemonic, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>;`
        *   `fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError>;`
        *   `fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError>;`
    *   Implement `NameTokenWalletRepository` for `std::sync::Arc<R>` where `R: NameTokenWalletRepository + ?Sized`.

#### 6. `dns_nostr_wallet/src/adapters/mod.rs` (Modify)

*   **Type / Scope**: Adapters Module Index
*   **Rationale & Proposed Changes**: Register `pub mod sqlite_name_token_wallet_repository;` alongside `cli` and `environment_config`.

#### 7. `dns_nostr_wallet/src/adapters/sqlite_name_token_wallet_repository.rs` (Create)

*   **Type / Scope**: SQLite Repository Adapter + Unit Tests (`mod test_sqlite_name_token_wallet_repository`)
*   **Rationale & Proposed Changes**: Implement `NameTokenWalletRepository` backed by `Mutex<rusqlite::Connection>` with atomic single-transaction writes and post-commit `take_staged()`.
    *   Constructors:
        *   `pub fn new(connection: Connection) -> Self`
        *   `pub fn in_memory() -> Result<Self, rusqlite::Error>`
    *   Implement `NameTokenWalletRepository for SqliteNameTokenWalletRepository`:
        *   `create`: opens `conn.transaction()`, creates `dns_nostr_wallet (id INTEGER PRIMARY KEY CHECK (id = 0), mnemonic TEXT NOT NULL)` and initializes BDK tables (`ChangeSet::init_sqlite_tables(&tx)`), checks if a row in `dns_nostr_wallet` or non-empty BDK `ChangeSet` already exists (returning `Err(WalletRepositoryError::WalletAlreadyExists)`), inserts `(0, mnemonic.to_string())`, persists `wallet.staged()` via `changeset.persist_to_sqlite(&tx)`, commits `tx.commit()`, and only then calls `wallet.take_staged()`.
        *   `save`: inspects `wallet.staged()`; if `Some(changeset)` and `!changeset.is_empty()`, opens `conn.transaction()`, verifies row `id = 0` exists in `dns_nostr_wallet` (returning `Err(WalletRepositoryError::CorruptedState(...))` if absent), calls `changeset.persist_to_sqlite(&tx)`, commits `tx.commit()`, and only then calls `wallet.take_staged()`.
        *   `load`: opens `conn.transaction()`, initializes tables if absent, queries `SELECT mnemonic FROM dns_nostr_wallet WHERE id = 0` and `ChangeSet::from_sqlite(&tx)`:
            *   `(None, cs)` with `cs.is_empty()` -> `Ok(None)`
            *   `(Some(_), cs)` with `cs.is_empty()` or `(None, cs)` with `!cs.is_empty()` -> `Err(WalletRepositoryError::CorruptedState(...))`
            *   `(Some(mnemonic_str), cs)` -> parses `Mnemonic`, reads `cs.network`, calls `bdk_wallet::Wallet::load().load_wallet_no_persist(cs)` (loading public descriptors only), derives `master_key = Xpriv::new_master(network, &mnemonic.to_seed(""))`, and returns `Ok(Some(DnsNostrWallet::from_loaded(bdk_wallet, master_key)))`.
    *   Add unit tests in `mod test_sqlite_name_token_wallet_repository` covering:
        *   `load` returns `Ok(None)` on an empty database.
        *   `create` persists both mnemonic and initial `ChangeSet`, clears `wallet.staged()`, and rejects a second `create` with `WalletRepositoryError::WalletAlreadyExists` while leaving the existing wallet intact.
        *   `save` persists revealed address index updates across `load` calls and clears `wallet.staged()` after commit.
        *   If a write fails before commit (e.g., `create` fails with `WalletAlreadyExists` or `save` fails on an uninitialized database), `wallet.staged()` remains `Some(...)` in memory and the database is rolled back.
        *   `load` returns `Err(WalletRepositoryError::CorruptedState(_))` when `dns_nostr_wallet` has a row but `bdk_*` tables are empty, when `bdk_*` tables are populated but `dns_nostr_wallet` is empty, or when the stored mnemonic string is invalid.

#### 8. `dns_nostr_wallet/src/application/create_wallet.rs` (Modify)

*   **Type / Scope**: Application Use Case + Unit Tests (`mod test_create_wallet`)
*   **Rationale & Proposed Changes**: Decouple `CreateWallet` from `rusqlite::Connection` by parameterizing over `R: NameTokenWalletRepository`.
    *   `pub struct CreateWallet<R> { network: Network, repository: R }`
    *   `pub fn new(network: Network, repository: R) -> Self`
    *   `pub fn execute(&self) -> Result<Mnemonic, CreateWalletError>`: generates a 12-word `Mnemonic`, constructs `let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?`, calls `self.repository.create(&mnemonic, &mut wallet)?`, and returns `Ok(mnemonic)`.
    *   Map both `WalletError` and `WalletRepositoryError` into `CreateWalletError` (`WalletRepositoryError::WalletAlreadyExists => CreateWalletError::WalletAlreadyExists`, others to `CreateWalletError::Wallet(err.to_string())`).
    *   Update `mod test_create_wallet` to use `SqliteNameTokenWalletRepository::in_memory()` (or a mock repository).

#### 9. `dns_nostr_wallet/src/application/import_wallet.rs` (Modify)

*   **Type / Scope**: Application Use Case + Unit Tests (`mod test_import_wallet`)
*   **Rationale & Proposed Changes**: Decouple `ImportWallet` from `rusqlite::Connection` by parameterizing over `R: NameTokenWalletRepository`.
    *   `pub struct ImportWallet<R> { network: Network, repository: R }`
    *   `pub fn new(network: Network, repository: R) -> Self`
    *   `pub fn execute(&self, phrase: &str) -> Result<(), ImportWalletError>`: normalizes and parses `phrase` into `Mnemonic`, constructs `let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?`, and calls `self.repository.create(&mnemonic, &mut wallet)?`.
    *   Map `WalletError` and `WalletRepositoryError` into `ImportWalletError`.
    *   Update `mod test_import_wallet` to use `SqliteNameTokenWalletRepository::in_memory()`.

#### 10. `dns_nostr_wallet/src/application/get_receive_address.rs` (Modify)

*   **Type / Scope**: Application Use Case + Unit Tests (`mod test_get_receive_address`)
*   **Rationale & Proposed Changes**: Decouple `GetReceiveAddress` from `rusqlite::Connection`, remove the `index: Option<u32>` parameter, and return `bdk_wallet::AddressInfo`.
    *   `pub struct GetReceiveAddress<R> { repository: R }`
    *   `pub fn new(repository: R) -> Self`
    *   `pub fn execute(&self) -> Result<bdk_wallet::AddressInfo, GetReceiveAddressError>`: calls `let mut wallet = self.repository.load()?.ok_or(GetReceiveAddressError::WalletNotFound)?`, calls `let info = wallet.reveal_next_bitcoin_address()`, calls `self.repository.save(&mut wallet)?`, and returns `Ok(info)`.
    *   Map `WalletRepositoryError` into `GetReceiveAddressError::Wallet(err.to_string())`.
    *   Update `mod test_get_receive_address` to test sequential address revelation and `WalletNotFound` without `index: Option<u32>`.

#### 11. `dns_nostr_wallet/src/adapters/cli.rs` (Modify)

*   **Type / Scope**: CLI Adapter + Unit Tests (`mod test_cli`)
*   **Rationale & Proposed Changes**: Remove `bdk_wallet::rusqlite::Connection`, `Arc<Mutex<Connection>>`, and the `--index` flag on `WalletCommand::Address`.
    *   Change `WalletCommand::Address { index: Option<u32> }` to a unit variant `WalletCommand::Address`.
    *   Parameterize `CliController<R>` over `R: NameTokenWalletRepository` (holding `CreateWallet<R>`, `ImportWallet<R>`, `GetReceiveAddress<R>`) with `CliController::new(create_wallet: CreateWallet<R>, import_wallet: ImportWallet<R>, get_receive_address: GetReceiveAddress<R>) -> Self`.
    *   Remove `--index` parsing/peeking tests from `mod test_cli` and update remaining CLI tests to construct `Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap())`.

#### 12. `dns_nostr_wallet/src/main.rs` (Modify)

*   **Type / Scope**: Binary Composition Root
*   **Rationale & Proposed Changes**: Construct `let repository = Arc::new(SqliteNameTokenWalletRepository::new(conn))` after opening `<wallet_dir>/wallet.sqlite` and setting `0o600` permissions, and wire `CreateWallet::new(config.network, Arc::clone(&repository))`, `ImportWallet::new(config.network, Arc::clone(&repository))`, and `GetReceiveAddress::new(repository)` into `CliController::new(...)`.

#### 13. `dns_nostr_wallet/README.md` (Modify)

*   **Type / Scope**: Crate EGM Contract Documentation
*   **Rationale & Proposed Changes**: Update `System Overview` and `Subdirectories & System Boundaries` to remove `[--index N]` from the CLI usage line and document `KeyDerivationPolicy`, `NameTokenWalletRepository`, and `SqliteNameTokenWalletRepository`.

#### 14. `dns_nostr_wallet/src/README.md` (Modify)

*   **Type / Scope**: Source Root EGM Contract Documentation
*   **Rationale & Proposed Changes**: Update `Interface` to reflect that `main.rs` constructs `SqliteNameTokenWalletRepository` and injects it into the use cases and `CliController`, and update `adapters` and `domain` summary lines.

#### 15. `dns_nostr_wallet/src/domain/README.md` (Modify)

*   **Type / Scope**: Domain Layer EGM Contract Documentation
*   **Rationale & Proposed Changes**: Document the three modules in `domain/` (`dns_nostr_wallet.rs`, `key_derivation_policy.rs`, `name_token_wallet_repository.rs`), their public types and method signatures, the removal of `rusqlite` and SQL side effects from `domain/`, and the new in-memory invariants.

#### 16. `dns_nostr_wallet/src/application/README.md` (Modify)

*   **Type / Scope**: Application Layer EGM Contract Documentation
*   **Rationale & Proposed Changes**: Update `Overview`, `Interface`, `Invariants`, and `Side Effects` to reflect that `CreateWallet<R>`, `ImportWallet<R>`, and `GetReceiveAddress<R>` depend on `R: NameTokenWalletRepository` rather than `&mut Connection`, and that `GetReceiveAddress::execute(&self)` takes no `index` parameter and returns `bdk_wallet::AddressInfo`.

#### 17. `dns_nostr_wallet/src/adapters/README.md` (Modify)

*   **Type / Scope**: Adapters Layer EGM Contract Documentation
*   **Rationale & Proposed Changes**: Add [`sqlite_name_token_wallet_repository.rs`](./sqlite_name_token_wallet_repository.rs) (`SqliteNameTokenWalletRepository`), document its atomic single-transaction persistence of `dns_nostr_wallet` and `bdk_*` tables and post-commit `take_staged()`, and update `cli.rs` to reflect the removal of `--index` and `Arc<Mutex<Connection>>`.

#### 18. `docs/domain.md` (Modify)

*   **Type / Scope**: Ubiquitous Language & Domain Model Documentation
*   **Rationale & Proposed Changes**: Update the `DnsNostrWallet` entry to describe the in-memory entity wrapping `bdk_wallet::Wallet` and `master_key: Xpriv`, add entries for `KeyDerivationPolicy` and `NameTokenWalletRepository`, and update the `hash31` file link to [`key_derivation_policy.rs`](../dns_nostr_wallet/src/domain/key_derivation_policy.rs).

#### 19. `AGENTS.md` (Modify)

*   **Type / Scope**: Agent Entry-Point Documentation
*   **Rationale & Proposed Changes**: Update the wallet CLI command in the `Commands` section from `cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address [--index N]` to `cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address`.

#### 20. `README.md` (Modify)

*   **Type / Scope**: Root Human-Facing Documentation
*   **Rationale & Proposed Changes**: Update the wallet CLI example in `Wallet CLI (work in progress)` from `cargo run -p dns_nostr_wallet -- wallet address --index 0` to `cargo run -p dns_nostr_wallet -- wallet address`.

### Verification Plan

*   **Compilation/Build**: `cargo build --workspace` must succeed with zero warnings.
*   **Unit & Integration Tests**: `cargo test --workspace` (and `cargo test -p dns_nostr_wallet`) must pass all co-located unit tests across `key_derivation_policy`, `dns_nostr_wallet`, `sqlite_name_token_wallet_repository`, `create_wallet`, `import_wallet`, `get_receive_address`, `cli`, and `environment_config`.
*   **Domain Purity Check**: Verify that `dns_nostr_wallet/src/domain/` contains zero occurrences of `rusqlite`, `Connection`, or SQL strings.

--------------------------------------------------------------------------------

<!-- Provenance: Indicates initial structural drafting via EGM. -->

*Drafted with EGM*

