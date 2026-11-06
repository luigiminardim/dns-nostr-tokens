# Product Requirements Document (PRD)

## Title & Status
**Title:** Refactor `Wallet` to `DnsNostrWallet` with `bdk_wallet` and SQLite Persistence  
**Status:** Draft  
**Reference Issue:** #34 (Foundation for #35 and #36)

---

## Problem Statement & Motivation
Currently, `dns_nostr_wallet` manages key derivation through an independent domain service (`KeyService`), wraps the mnemonic in an anemic entity (`Wallet`), and persists seeds in a plain text file (`seed.txt`) via `FileMnemonicRepository`.

While this satisfied initial seed creation and static BIP 84 address derivation, it introduces several limitations:
1. **Redundant Abstractions:** `KeyService` and `MnemonicRepository` add multiple layers of boilerplate without providing long-term value.
2. **Lack of Keychain Persistence:** Address indices are not persisted across CLI process invocations unless tracked manually by the user or an external store.
3. **Missing Bitcoin Primitives:** Upcoming milestones (Issue #35: send/receive sats, balance; Issue #36: token minting) require coin selection, change output handling, and PSBT transaction building. Re-implementing these from scratch is error-prone.

By embedding `bdk_wallet::Wallet` as a private attribute of `DnsNostrWallet` and utilizing SQLite (`rusqlite`) for persistence, we:
- Leverage battle-tested Bitcoin Dev Kit (BDK) primitives for BIP 84 address derivation and keychain index persistence.
- Naturally segregate standard BIP 84 UTXOs from custom Name-Token UTXOs (`0x6E616D65`), satisfying security requirement **FR-9**.
- Consolidate protocol-specific derivation logic directly inside `DnsNostrWallet`, eliminating `key_service.rs` and `mnemonic_repository.rs`.

---

## Target Audience
- **DNS-Nostr CLI Users:** Users managing wallet seeds and generating receive addresses.
- **Developers / Maintainers:** Working on upcoming wallet milestones (UTXO scanning, send/receive, token minting).

---

## Scope & Out of Scope

### In Scope
- **Dependency Update:** Add `bdk_wallet` (v1.0 with `rusqlite` and `keys-bip39` features) and `rusqlite` to `dns_nostr_wallet`.
- **Domain Refactoring:**
  - Rename `Wallet` entity to `DnsNostrWallet` (in `dns_nostr_wallet/src/domain/dns_nostr_wallet.rs`).
  - Embed `bdk_wallet::Wallet` as a private property.
  - Absorb `KeyService` logic (BIP 44 Name-Token path `0x6E616D65` and Nostr key derivation `hash31(label)`) directly into `DnsNostrWallet` as private helpers. Delete `key_service.rs`.
  - Delete `MnemonicRepository`, `FileMnemonicRepository`, and `InMemoryMnemonicRepository`.
- **Persistence:**
  - Persist private descriptors (`wpkh(xprv...)`) and wallet state in a local SQLite file (`wallet.sqlite` in the configured wallet directory).
  - Automatically persist keychain index advancements when revealing addresses.
- **Public API of `DnsNostrWallet`:**
  - `create(mnemonic, network, connection)`: Initializes a new wallet with descriptors and persists in SQLite.
  - `load(connection)`: Loads an existing wallet from SQLite.
  - `reveal_next_address(&mut self, connection)`: Advances and persists the external receive address index.
  - `peek_address(&self, index: u32)`: Inspeciona um endereço padrão sem avançar o índice.
  - `name_token_address(&self, index: u32)`: Deriva o endereço P2PKH no chain de Name-Tokens.
  - `nostr_keys(&self, label: &Label)`: Deriva o par de chaves Nostr via `hash31`.
  - All master keys, descriptors, and internal helpers remain strictly private.
- **Application & Adapters Update:**
  - Update `CreateWallet`, `ImportWallet`, and `GetReceiveAddress` use cases to work with `DnsNostrWallet` and SQLite connection/factory.
  - Update `CliController` and `main.rs` to open `wallet.sqlite` and pass the connection.
  - Update CLI tests to use in-memory SQLite connections (`Connection::open_in_memory()`).
- **Documentation:**
  - Update `AGENTS.md` and `docs/domain.md` to reflect the new architecture.

### Out of Scope
- Blockchain synchronization / UTXO scanning via RPC (Issue #35).
- Balance queries and send/receive Bitcoin transactions (Issue #35).
- On-chain token minting, updating, transferring, or revoking (Issues #36–#40).
- Publishing DNS records to Nostr relays (Issue #37).

---

## User Stories & Requirements

### US-1: Wallet Creation with BDK and SQLite
**As a** user,  
**I want to** run `wallet create`  
**So that** a new BIP39 seed phrase is generated, displayed for backup, and the wallet initialized and stored in `wallet.sqlite`.
- **Acceptance Criteria:**
  - Generates a valid 12-word BIP39 mnemonic.
  - Initializes `bdk_wallet::Wallet` with BIP 84 external (`/84'/coin'/0'/0/*`) and internal (`/84'/coin'/0'/1/*`) descriptors.
  - Persists wallet changesets into SQLite.
  - Re-running `create` when `wallet.sqlite` already contains a wallet returns an explicit error (`WalletAlreadyExists`).

### US-2: Wallet Import
**As a** user,  
**I want to** run `wallet import <words>`  
**So that** I can recover my existing wallet from an existing BIP39 mnemonic into `wallet.sqlite`.
- **Acceptance Criteria:**
  - Validates word count, dictionary, and checksum.
  - Initializes `DnsNostrWallet` and persists it into SQLite.
  - Re-running `import` when a wallet already exists returns `WalletAlreadyExists`.

### US-3: Receiving Addresses and Index Persistence
**As a** user,  
**I want to** run `wallet address`  
**So that** I get the next unused address, with the index automatically advancing across separate CLI commands.
- **Acceptance Criteria:**
  - Calling `wallet address` without `--index` reveals index 0 on the first call, index 1 on the second call, and so on.
  - Calling `wallet address --index <N>` peeks at index `N` without modifying the stored index.
  - Fails with `WalletNotFound` if `wallet.sqlite` does not exist or has not been initialized.

### US-4: DNS-Nostr Token & Nostr Key Derivation
**As a** DNS-Nostr protocol developer,  
**I want** `DnsNostrWallet` to provide `name_token_address(index)` and `nostr_keys(label)`  
**So that** Name-Token UTXOs and per-label Nostr keys are derived deterministically according to protocol specification.
- **Acceptance Criteria:**
  - `name_token_address(0)` on Mainnet returns `1M9PnNAN8XcKsrVtGPGSh8wi6n3eJyTXqe`.
  - `nostr_keys(&Label::new("alice")?)` on Mainnet returns public key `0bc855f8de0ced298f6481e60445f0f6128092e2285e3f110f5caefe3c693fa4`.

---

## Success Metrics (KPIs)
- **100% Test Pass Rate:** `cargo test --workspace` passes without warnings or regressions.
- **Codebase Simplification:**
  - Deletion of `key_service.rs` (absorbing ~400 lines into ~60 concise lines of domain logic in `dns_nostr_wallet.rs`).
  - Deletion of `mnemonic_repository.rs`, `file_mnemonic_repository.rs`, and `in_memory_mnemonic_repository.rs`.
- **Zero Secret Leaks:** Master private keys are never exposed in public interfaces or debug formatting.

---

## Technical Impact & Codebase Survey
- **`dns_nostr_wallet/Cargo.toml`:** Add `bdk_wallet` (with `rusqlite`, `keys-bip39`), `rusqlite`.
- **`dns_nostr_wallet/src/domain/`:**
  - Delete `key_service.rs` and `mnemonic_repository.rs`.
  - Replace `wallet.rs` with `dns_nostr_wallet.rs`.
- **`dns_nostr_wallet/src/application/`:**
  - Update `create_wallet.rs`, `import_wallet.rs`, `get_receive_address.rs`.
  - Delete `in_memory_mnemonic_repository.rs`.
- **`dns_nostr_wallet/src/adapters/`:**
  - Delete `file_mnemonic_repository.rs`.
  - Update `cli.rs` and `environment_config.rs` (manage SQLite db path, default `./.data/dns-nostr-wallet/wallet.sqlite`).
- **`dns_nostr_wallet/src/main.rs`:** Initialize SQLite connection and wire use cases.
