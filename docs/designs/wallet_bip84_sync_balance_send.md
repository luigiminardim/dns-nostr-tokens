# Wallet BIP 84 Blockchain Sync, Balance, and Send (Issue #35)

* **Author**: Luigi Minardi
* **Status**: Implemented
* **Date**: 2026-11-24

--------------------------------------------------------------------------------

## Overview

`dns_nostr_wallet` currently generates or imports a BIP 39 mnemonic, stores wallet state in SQLite, and derives BIP 84 P2WPKH receive addresses (`dns_nostr_wallet wallet address`), but it has no connection to a Bitcoin node. Users cannot see incoming payments, check their spendable balance, or send Bitcoin.

This design connects `dns_nostr_wallet` to a local Bitcoin Core JSON-RPC node across all five `wallet` subcommands (`create`, `import`, `address`, `balance`, `send`), aligning its BDK dependencies with `dns_nostr_server` (the workspace's Bitcoin-watching DNS authority). In the DNS-Nostr domain ([`docs/domain.md`](../domain.md)), **Standard UTXOs** are ordinary BIP 84 P2WPKH outputs used for payments and fees, while **Token UTXOs** are Name-Token inscription outputs isolated on a separate BIP 44 custom chain (`m/44'/coin'/0'/0x6E616D65/i`). This design enables Standard UTXO balance tracking and outbound payments, which later issues will use to fund Name-Token mint, update, and revoke transactions.

## The Problem

A wallet that only derives offline addresses cannot participate in Bitcoin transactions:

1. **No chain awareness**: `dns_nostr_wallet` never queries Bitcoin Core, so it cannot discover received Standard UTXOs, detect chain reorganizations, or reconcile evicted wallet transactions.
2. **No balance visibility**: Users have no way to check how much Bitcoin is available to spend before paying fees or transferring funds.
3. **No outbound transfers**: Users cannot build, sign, or broadcast Bitcoin transactions from the CLI.
4. **Conflicting initialization needs**: A newly created wallet has no prior on-chain history, so scanning from block `0` on `wallet create` would download and scan every historical block on mainnet needlessly. Conversely, an imported mnemonic may own historical outputs and must scan from block `0`. Writing `wallet.sqlite` before RPC initialization succeeds would persist a wallet whose generated 12-word mnemonic was never displayed to the user and block retries behind `WalletAlreadyExists`.
5. **Cross-network and spendability hazards**: Pointing a wallet configured for one Bitcoin network (for example `mainnet`) at a node running another network (for example `regtest`) would silently corrupt wallet state or broadcast to the wrong chain. Spending unconfirmed external outputs or immature coinbase outputs risks double-spend or consensus rejections, while ignoring evicted wallet-originated transactions leaves stale unconfirmed spends in SQLite.

## Proposed Solution

Connect `dns_nostr_wallet` to Bitcoin Core over JSON-RPC behind a synchronous domain port (`BlockchainPort`), domain service (`SyncService`), and RPC adapter (`RpcBlockchainAdapter`), keeping all five `wallet` subcommands (`create`, `import`, `address`, `balance`, `send`) chain-aware while avoiding global mempool downloads. `wallet create` anchors at the node's active tip via a bounded backward header walk without downloading historical blocks; `wallet import` scans confirmed blocks from height `0` in memory before committing atomically; and `address`, `balance`, and `send` incrementally sync confirmed blocks (including reorgs) from the persisted SQLite checkpoint and check only the wallet's own unconfirmed transactions for mempool eviction via `SyncService`. SQLite creation becomes lazy inside the repository adapter (`SqliteNameTokenWalletRepositoryAdapter`) so failed initialization leaves no database file behind, while `KeyDerivationPolicy`'s existing BIP 84 descriptors and BDK's built-in signing remain intact.

## Goals

* Upgrade `dns_nostr_wallet` to `bdk_wallet 2.3` (`features = ["rusqlite"]`), hoist `bdk_bitcoind_rpc = "0.22.0"` into root workspace dependencies, and drop the unused direct `rusqlite` dependency from `dns_nostr_wallet/Cargo.toml` once `main.rs` drops `rusqlite::Connection`, unifying `dns_nostr_wallet` and `dns_nostr_server` on a single `bdk_chain 0.23.3` / `bdk_core 0.6.3` / `rusqlite 0.31.0` dependency graph.
* Configure and validate Bitcoin Core RPC connection parameters in `dns_nostr_wallet` via `DNS_NOSTR_BITCOIN_RPC_URL`, `DNS_NOSTR_BITCOIN_RPC_USER`, and `DNS_NOSTR_BITCOIN_RPC_PASSWORD` (with password redaction in `Debug` output).
* Synchronize against Bitcoin Core behind a synchronous `BlockchainPort`, `SyncService`, and `RpcBlockchainAdapter` on all five `wallet` subcommands with uniform behavior across networks (`regtest`, `testnet`, `mainnet`): tip-anchoring on `wallet create`, height-`0` confirmed-block scan on `wallet import`, and checkpoint-resumed confirmed-block sync plus targeted eviction checks on the wallet's own unconfirmed transactions (`get_mempool_entry`) on `wallet address`, `wallet balance`, and `wallet send`.
* Make `SqliteNameTokenWalletRepositoryAdapter` lazy on creation (`0600` on Unix) and non-creating on `load()`, while opening existing database files read-write without `SQLITE_OPEN_CREATE` so SQLite can perform automatic hot-journal crash recovery.
* Enforce network and genesis-hash agreement across `DNS_NOSTR_NETWORK` and the connected Bitcoin Core node (`get_block_hash(0)`) on `create`/`import`, and three-way agreement across `DNS_NOSTR_NETWORK`, `wallet.sqlite`, and the node on `address`/`balance`/`send`.
* Emit bare values on `stdout`: `wallet address` prints `<address>`, `wallet balance` prints `<decimal_btc>` (decimal BTC with trailing fractional zeros trimmed, e.g., `0`, `1.5`, never scientific notation), and `wallet send <address> <btc>` prints `<txid>`. Send all errors to `stderr`.
* Report and spend `confirmed` plus wallet-originated `trusted_pending` (change) outputs while excluding immature coinbase outputs (`< 100` confirmations); external deposits are not tracked until confirmed.
* Estimate fees automatically via `estimatesmartfee(2, ECONOMICAL)` with fallback to `getmempoolinfo` minimum fees and a `1 sat/vB` floor, without CLI flags.
* Keep `KeyDerivationPolicy`'s infallible `Bip84<Xpriv>` descriptor templates unchanged, rely on `bdk_wallet::ChangeSet` persisting public descriptors only in SQLite's `bdk_*` tables, restore in-memory signers via `Wallet::load().descriptor(...).extract_keys()`, and sign PSBTs with BDK's `Wallet::sign`.
* Cover sync, reorgs, self-tx eviction, UTXO eligibility, fee fallbacks, post-reload signing, lazy SQLite behavior, and multi-network derivation in offline unit tests (`just test`), and verify receive, balance, send, change, historical import recovery, and cross-network genesis rejection in regtest E2E tests (`just test e2e`).

## Non-Goals

* Scanning, reporting, or spending Token UTXOs on chain `m/44'/coin'/0'/0x6E616D65/i`.
* CLI flags on `wallet send` (`--fee-rate`, `--yes`, `--dry-run`), fee bumping (RBF / CPFP), send-max, or manual coin control.
* Global mempool scanning (`Emitter::mempool()`) or visibility of unconfirmed external deposits.
* Supporting pruned Bitcoin Core nodes or Initial Block Download (IBD) RPC checks.
* TLS (`https://`) transport for Bitcoin Core JSON-RPC (like `dns_nostr_server`, config validation accepts `https://`, while `bitcoincore-rpc 0.19.0` supports plaintext `http://` only).
* Alternative Bitcoin backends (Electrum, Esplora, or BIP 158 compact block filters).
* Adding an asynchronous runtime (`tokio`) to `dns_nostr_wallet`.
* Multi-wallet support within a single `wallet.sqlite` file.

## Critical User Journeys (CUJs)

### CUJ 1: Create a Wallet, Receive Funds, and Check Balance

* **Given**: No `wallet.sqlite` exists in `DNS_NOSTR_WALLET_DIR`, and a synced Bitcoin Core node matching `DNS_NOSTR_NETWORK` (default `regtest`) is reachable via `DNS_NOSTR_BITCOIN_RPC_*`.
* **When**: The user runs `wallet create`, runs `wallet address`, funds the printed BIP 84 address from an external wallet, mines 1 block, and runs `wallet balance`.
* **Then**:
  * `wallet create` anchors at the node's current tip without scanning historical blocks, writes `wallet.sqlite` (mode `0600` on Unix), and prints the 12-word mnemonic.
  * `wallet address` syncs confirmed blocks and prints a bare BIP 84 P2WPKH address (`bcrt1q...` on regtest).
  * Before confirmation, `wallet balance` prints `0` because unconfirmed external deposits are not tracked until confirmed.
  * After 1 confirmation, `wallet balance` prints the exact funded amount in decimal BTC (for example `1.5`).

### CUJ 2: Import an Existing Mnemonic and Recover Historical Funds

* **Given**: A BIP 39 mnemonic that already received confirmed payments on the configured network, and no `wallet.sqlite` in `DNS_NOSTR_WALLET_DIR`.
* **When**: The user runs `wallet import [WORDS...]` (passing words as arguments or via stdin) and then `wallet balance`.
* **Then**:
  * `wallet import` scans confirmed blocks from height `0` to the chain tip, commits the mnemonic and synced wallet state atomically to `wallet.sqlite`, and prints `Wallet imported.`.
  * `wallet balance` prints the recovered confirmed balance in decimal BTC.

### CUJ 3: Send Bitcoin and Spend Unconfirmed Change

* **Given**: An initialized wallet with a confirmed Standard UTXO balance (for example `1.0` BTC) and a valid recipient address for `DNS_NOSTR_NETWORK`.
* **When**: The user runs `wallet send <recipient_address> 0.25`, then runs `wallet balance` and a second `wallet send <recipient_address_2> 0.1` before the first transaction confirms.
* **Then**:
  * The first `wallet send` estimates a fee rate at or above the node's mempool floor, builds, signs, and broadcasts the transaction, persists it as unconfirmed in SQLite, and prints only `<txid>`.
  * `wallet balance` reflects `1.0 - 0.25 - fee` because the wallet's own unconfirmed change output is `trusted_pending`.
  * The second `wallet send` succeeds using the `trusted_pending` change output and prints its `<txid>`.

### CUJ 4: Reject Misconfigured Network or Genesis Mismatch

* **Given**: A local `regtest` Bitcoin Core node at `DNS_NOSTR_BITCOIN_RPC_URL` and `DNS_NOSTR_NETWORK=mainnet`.
* **When**:
  * No `wallet.sqlite` exists and the user runs `wallet create` or `wallet import [WORDS...]`; or
  * An initialized `mainnet` `wallet.sqlite` exists and the user runs `wallet address`, `wallet balance`, or `wallet send <mainnet_address> <btc>`.
* **Then**: The command exits non-zero, writes an error identifying the expected and actual genesis hashes to `stderr`, and does not create or modify `wallet.sqlite`. *(If an initialized `regtest` `wallet.sqlite` exists when `DNS_NOSTR_NETWORK=mainnet`, `address`, `balance`, and `send` fail earlier with a `NetworkMismatch` error before contacting the node.)*

--------------------------------------------------------------------------------

## Technical Plan (Architecture)

### 1. Layering and Dependency Direction

`dns_nostr_wallet` keeps its inward dependency rule (`adapters` → `application` → `domain`), with `src/main.rs` as a thin composition root. All Bitcoin Core RPC crates (`bdk_bitcoind_rpc` and its re-exported `bitcoincore_rpc`) remain confined to `adapters/`. `domain/` defines the synchronous `BlockchainPort`, domain `SyncService`, and keeps `bdk_wallet::Wallet` and `master_key` private inside `DnsNostrWallet`.

All structs and files strictly follow the architectural role postfix convention:
- **Use Cases** in `application/`: `CreateWalletUsecase` (`create_wallet_usecase.rs`), `ImportWalletUsecase` (`import_wallet_usecase.rs`), `GetReceiveAddressUsecase` (`get_receive_address_usecase.rs`), `GetBalanceUsecase` (`get_balance_usecase.rs`), and `SendBitcoinUsecase` (`send_bitcoin_usecase.rs`).
- **Domain Services** in `domain/`: `SyncService` (`sync_service.rs`) encapsulating wallet blockchain synchronization and eviction coordination.
- **Domain Ports** in `domain/`: `BlockchainPort` (`blockchain_port.rs`), `NameTokenWalletRepositoryPort` (`name_token_wallet_repository_port.rs`).
- **Adapters** in `adapters/`: `RpcBlockchainAdapter` (`rpc_blockchain_adapter.rs`), `SqliteNameTokenWalletRepositoryAdapter` (`sqlite_name_token_wallet_repository_adapter.rs`).
- **Controllers** in `adapters/`: `CliController` (`cli_controller.rs`).

```text
                         src/main.rs (Composition Root)
              wires EnvironmentConfig, SqliteNameTokenWalletRepositoryAdapter,
              RpcBlockchainAdapter, SyncService, five use cases, CliController
                                       │
       ┌───────────────────┬───────────┴───────────┬──────────────────────────┐
       ▼                   ▼                       ▼                          ▼
adapters/            adapters/               adapters/                  adapters/
environment_config   cli_controller.rs       sqlite_name_token_wallet_  rpc_blockchain_
.rs                  (CliController)         repository_adapter.rs      adapter.rs (new)
(EnvironmentConfig)        │                 (SqliteNameTokenWallet     (RpcBlockchain
                           │                  RepositoryAdapter)         Adapter + Emitter)
                           ▼                       │ implements               │ implements
                     application/                  │                          │
                     ├── create_wallet_usecase.rs  │                          │
                     ├── import_wallet_usecase.rs  │                          │
                     ├── get_receive_address_      │                          │
                     │   usecase.rs                │                          │
                     ├── get_balance_usecase.rs    │                          │
                     └── send_bitcoin_usecase.rs   │                          │
                           │                       │                          │
                           └───────────────────────┴──────────────────────────┘
                                                   ▼
                     domain/
                     ├── dns_nostr_wallet.rs                  (DnsNostrWallet entity)
                     ├── sync_service.rs (new)                (SyncService domain service)
                     ├── key_derivation_policy.rs             (KeyDerivationPolicy)
                     ├── name_token_wallet_repository_port.rs (NameTokenWalletRepositoryPort)
                     └── blockchain_port.rs (new)             (BlockchainPort + BlockchainError)
```

### 2. Unified Workspace BDK Dependencies

`dns_nostr_wallet/Cargo.toml` upgrades `bdk_wallet` from `"1.0"` (locked to `1.2.0`, `bdk_core 0.4.1`) to `"2.3"` (`features = ["rusqlite"]`), matching workspace `bdk_chain 0.23.3`, `bdk_core 0.6.3`, and `rusqlite 0.31.0`. `bdk_bitcoind_rpc = "0.22.0"` (which re-exports `bitcoincore-rpc 0.19.0`) is hoisted into root `Cargo.toml` `[workspace.dependencies]` and consumed by both `dns_nostr_server` and `dns_nostr_wallet`. Once `dns_nostr_wallet/src/main.rs` removes `use rusqlite::Connection;` in Phase 4, the unused direct `rusqlite` dependency is removed from `dns_nostr_wallet/Cargo.toml` (`bdk_chain 0.23.3`'s `rusqlite` feature already enables `bundled`). Both crates compile against one `bdk_chain` / `bdk_core` / `rusqlite` graph, allowing `bdk_bitcoind_rpc::Emitter` in `adapters/` to feed `DnsNostrWallet` directly without checkpoint conversion adapters.

### 3. Domain Model and Port Contracts

* **`BlockchainPort`** (`domain/blockchain_port.rs`): A synchronous domain port implemented by `RpcBlockchainAdapter` in `adapters/` and by an in-memory fake in offline tests. It exposes four operations:
  * **Tip-anchor query (`fetch_tip_anchor`)**: Takes `expected_genesis: BlockHash`, verifies the node's genesis hash matches, walks backward from the active tip via `previous_block_hash` up to a shallow-reorg window (`10` headers total, stopping at height `0`) so every block in the window belongs to one cryptographically linked chain, and returns `Vec<BlockId>` in ascending height order without downloading full blocks.
  * **Confirmed-block and self-tx eviction sync (`sync`)**: Verifies the node's genesis hash, streams confirmed blocks (including reorgs) from the wallet's persisted checkpoint via `bdk_bitcoind_rpc::Emitter::next_block()`, and then checks only the wallet's remaining unconfirmed transactions (`wallet.unconfirmed_txids()`) against Bitcoin Core's mempool via `get_mempool_entry` so evicted self-sent transactions are reconciled without scanning the global mempool.
  * **Fee-rate estimation (`estimate_fee_rate`)**: Resolves a transaction fee rate from Bitcoin Core RPC with automatic fallbacks (`Technical Plan §6`).
  * **Transaction broadcast (`broadcast`)**: Submits a signed raw transaction via `sendrawtransaction` and returns its `Txid`.
  * **`BlockchainError`**: Distinguishes genesis mismatch (`expected`, `actual`), RPC transport/protocol errors (`Rpc`), wallet chain-apply errors (`ChainUpdate`), and broadcast rejections (`Broadcast`).
* **`SyncService`** (`domain/sync_service.rs`): A domain service coordinating synchronization between `DnsNostrWallet` and `BlockchainPort`. It encapsulates tip anchoring for new wallets and incremental block streaming plus self-tx mempool eviction checks for existing wallets.
* **`NameTokenWalletRepositoryPort`** (`domain/name_token_wallet_repository_port.rs`): Retains its existing three-method contract (`create`, `save`, `load`) with no `exists()` expansion. `SqliteNameTokenWalletRepositoryAdapter` becomes lazily path-backed (moving directory creation, file open, and Unix `0600` permissions out of `main.rs`) and shares one unified SQL path across path-backed and `in_memory()` modes:
  * `load()` never creates directories, creates `wallet.sqlite`, runs schema DDL, or writes data. When `!db_path.exists()`, `load()` returns `Ok(None)` without touching the filesystem. When `wallet.sqlite` exists, `load()` opens it with `SQLITE_OPEN_READ_WRITE` (explicitly omitting `SQLITE_OPEN_CREATE`) so SQLite can complete automatic hot-journal crash recovery if a prior write was interrupted, executes only `SELECT` queries inside a `DEFERRED` transaction calling `ChangeSet::from_sqlite(&tx)` (dropping the transaction without commit), maps missing-table SQLite errors on an existing DB/connection to `CorruptedState` (without probing `sqlite_master` for `bdk_%`), and validates the 4-way `(mnemonic_row, changeset.is_empty())` invariant.
  * `create()` creates `wallet_dir` if missing (guarding non-empty parent paths), creates `wallet.sqlite` atomically with `0600` permissions on Unix, initializes both `dns_nostr_wallet` and BDK tables via `Self::init_tables(&tx)`, re-checks non-existence inside the write transaction, and commits the mnemonic and initial BDK state atomically.
  * `save()` returns `CorruptedState` if `!db_path.exists()`, opens with `SQLITE_OPEN_READ_WRITE` (no `SQLITE_OPEN_CREATE`), verifies the mnemonic row exists (`CorruptedState` if absent), persists the staged `ChangeSet`, commits, and clears staged changes.
* **`DnsNostrWallet`** (`domain/dns_nostr_wallet.rs`): Keeps `bdk_wallet` and `master_key` private and exposes domain operations for network/genesis inspection, checkpoint and unconfirmed-txid retrieval, tip anchoring, block connection, evicted-tx reconciliation, trusted spendable balance lookup, transaction building and signing, and recording freshly broadcast transactions.

### 4. Command Execution Flows

```text
wallet create:
  repository.load() == None ─▶ generate mnemonic ─▶ DnsNostrWallet::new (in-memory)
    ─▶ sync_service.tip_anchor(&mut wallet, blockchain) [verify genesis + up to 10 headers via BlockchainPort]
    ─▶ repository.create(&mnemonic, &mut wallet) [create 0600 wallet.sqlite + single SQLite tx]
    ─▶ stdout: mnemonic

wallet import:
  parse_mnemonic ─▶ repository.load() == None ─▶ DnsNostrWallet::new (in-memory, height 0)
    ─▶ sync_service.sync(&mut wallet, blockchain) [verify genesis + confirmed blocks 0..tip via BlockchainPort]
    ─▶ repository.create(&mnemonic, &mut wallet) [create 0600 wallet.sqlite + single SQLite tx]
    ─▶ stdout: "Wallet imported."

wallet address / wallet balance:
  repository.load() ─▶ verify wallet.network() == config.network
    ─▶ sync_service.sync(&mut wallet, blockchain) [verify genesis + incremental confirmed blocks + self-tx eviction check]
    ─▶ (address only: wallet.reveal_next_bitcoin_address())
    ─▶ repository.save(&mut wallet)
    ─▶ stdout: <address> | <decimal_btc>

wallet send <address> <btc>:
  validate address & network ─▶ parse fixed-point BTC (>0, <=8 decimals, >= recipient dust threshold)
    ─▶ repository.load() ─▶ verify wallet.network() == config.network
    ─▶ sync_service.sync(&mut wallet, blockchain) [verify genesis + incremental confirmed blocks + self-tx eviction check]
    ─▶ repository.save(&mut wallet)                       [pre-broadcast save: preserve sync progress]
    ─▶ fee_rate = blockchain.estimate_fee_rate()
    ─▶ tx = wallet.build_and_sign_transaction(recipient, amount, fee_rate)
    ─▶ txid = blockchain.broadcast(&tx)
    ─▶ wallet.record_broadcasted_transaction(tx, now)
    ─▶ repository.save(&mut wallet)                       [post-broadcast save: record spent inputs + change]
    ─▶ stdout: <txid>
```

> **Operator precondition for `wallet send`**: Because `--dry-run`, `--yes`, and `--fee-rate` are omitted from `wallet send <address> <btc>`, verify `<address>` and `<btc>` before invoking the command—signing and broadcast execute immediately with no confirmation prompt.

Splitting `SendBitcoinUsecase` into two repository saves guarantees:

1. If `InsufficientFunds`, fee estimation, or `broadcast` fails, the pre-broadcast save has already persisted sync progress, and the uncommitted internal change address revealed while building the transaction is discarded rather than persisted as an unused change address in SQLite.
2. Only after `broadcast` succeeds does the post-broadcast save persist the unconfirmed transaction so consecutive `wallet balance` and `wallet send` commands immediately see spent inputs and `trusted_pending` change. If the post-broadcast save fails, `SendBitcoinUsecase` returns `SendBitcoinUsecaseError::BroadcastSucceededButPersistFailed { txid, reason }` so the caller sees the `<txid>` on `stderr`.

### 5. BDK Persistence, UTXO Eligibility, and Signing

* **Public-descriptor SQLite persistence with in-memory BDK signers**: `KeyDerivationPolicy` retains its existing infallible `Bip84<Xpriv>` descriptor templates (`bitcoin_external_descriptor`, `bitcoin_internal_descriptor`). `bdk_wallet::ChangeSet` serializes only `Descriptor<DescriptorPublicKey>` into SQLite's `bdk_*` tables; private signers (`KeyMap`) are never persisted in `bdk_*` tables. During `SqliteNameTokenWalletRepositoryAdapter::load`, `bdk_wallet::Wallet::load()` attaches the templates derived from the stored mnemonic's `master_key` via `.descriptor(External, ...).descriptor(Internal, ...).extract_keys()`, which both restores in-memory signers and rejects databases whose stored BDK descriptors do not match the stored mnemonic.
* **UTXO eligibility**: `wallet balance` and `wallet send` coin selection include `confirmed` and wallet-originated `trusted_pending` (change) outputs while excluding immature coinbase outputs (`< 100` confirmations) and any unconfirmed external outputs (external unconfirmed deposits are not ingested under targeted eviction sync). When spendable funds are insufficient, `InsufficientFunds` reports `needed`, `available`, and `immature` so the user sees whether unmatured coinbase rewards account for the shortfall.
* **PSBT signing**: `DnsNostrWallet::build_and_sign_transaction` builds a PSBT with BDK's `TxBuilder`, signs and finalizes it in memory via `bdk_wallet.sign(&mut psbt, SignOptions::default())`, requires all PSBT inputs to finalize before extracting the signed transaction, and rejects signing or finalization failures explicitly.

### 6. Fee Estimation and `BTC/kvB` Ceiling Conversion

`RpcBlockchainAdapter::estimate_fee_rate()` resolves a fee rate without CLI flags:

1. Query Bitcoin Core's `estimatesmartfee` (target `2` blocks, `ECONOMICAL` mode) and `getmempoolinfo`. Transport, authentication, or JSON deserialization errors propagate immediately as `BlockchainError::Rpc` (never silently downgraded to `1 sat/vB`).
2. `bitcoincore-rpc-json 0.19.0` deserializes `EstimateSmartFeeResult.fee_rate` (`Option<Amount>`) and `GetMempoolInfoResult::{mempool_min_fee, min_relay_tx_fee}` (`Amount`) directly as `bitcoin::Amount` values denominated in `BTC/kvB`. Convert each `sat/kvB` amount to `sat/kwu` using integer ceiling division by `4` so fractional relay floors (for example, `1,001 sat/kvB -> 251 sat/kwu` rather than truncating to `250 sat/kwu`) never fall below the node's rejection threshold.
3. Compute `node_min_fee` as the maximum of `mempool_min_fee` and `min_relay_tx_fee` from `getmempoolinfo`.
4. Return the maximum of the smart-fee estimate (falling back to `node_min_fee` when unavailable, as on regtest), `node_min_fee`, and the `1 sat/vB` (`250 sat/kwu`) floor. No wallet-side upper fee cap is enforced.

### 7. Network/Genesis Verification and RPC Method Selection

Every command enforces network and genesis-hash consistency uniformly across `regtest`, `testnet`, and `mainnet`:

1. `wallet create` and `wallet import` verify two sources before creating `wallet.sqlite`: `DNS_NOSTR_NETWORK` against the connected Bitcoin Core node's genesis hash (`get_block_hash(0)`).
2. `wallet address`, `wallet balance`, and `wallet send` enforce three-way agreement across `DNS_NOSTR_NETWORK`, the network stored in `wallet.sqlite`, and the connected node's genesis hash (`get_block_hash(0)`).

`RpcBlockchainAdapter` uses only the minimal RPC methods needed for these operations: `get_block_hash(0)` (genesis verification), `get_best_block_hash()` and `get_block_header_info()` (tip anchoring), `Emitter::next_block()` (`get_block_hash` / `get_block` for confirmed blocks), `get_mempool_entry()` (targeted eviction checks for the wallet's own unconfirmed transactions), `estimate_smart_fee()` and `get_mempool_info()` (fee estimation), and `send_raw_transaction()` (broadcast). `getblockchaininfo` is not called because pruned-node and IBD checks are out of scope (`Non-Goals`, Alternative 12).

### 8. Environment Config, CLI Invocation, and Single-Process Assumption

* **Existing and new environment variables (`adapters/environment_config.rs`)**:
  * `DNS_NOSTR_NETWORK`: `regtest` (default), `testnet` (Bitcoin testnet3), or `mainnet`.
  * `DNS_NOSTR_WALLET_DIR`: `./.data/dns-nostr-wallet` (default); database file is `<wallet_dir>/wallet.sqlite`.
  * `DNS_NOSTR_BITCOIN_RPC_URL`: `http://127.0.0.1:18443` (default); validated as `http://` or `https://` with a non-empty, whitespace-free authority (matching `dns_nostr_server`). **Operator preconditions**: point this URL at a synced, unpruned Bitcoin Core node over loopback (`127.0.0.1`) or an externally encrypted tunnel (`bitcoincore-rpc 0.19.0` does not implement TLS, so `https://` URLs fail to connect at runtime).
  * `DNS_NOSTR_BITCOIN_RPC_USER`: `rpcuser` (default); trimmed and required to be non-empty.
  * `DNS_NOSTR_BITCOIN_RPC_PASSWORD`: `rpcpassword` (default); required to be non-empty and non-whitespace-only, preserved without trimming; redacted as `"[REDACTED]"` in custom `fmt::Debug`.
* **CLI invocation**: The binary exposes a `wallet` subcommand group (`dns_nostr_wallet wallet create`, `dns_nostr_wallet wallet import [WORDS...]` accepting positional words or stdin, `dns_nostr_wallet wallet address`, `dns_nostr_wallet wallet balance`, `dns_nostr_wallet wallet send <address> <btc>`). `just wallet <subcommand>` wraps `cargo run -p dns_nostr_wallet -- wallet <subcommand>`.
* **Single-process CLI assumption**: `dns_nostr_wallet` assumes CLI commands are not executed concurrently against the same `wallet.sqlite` file.

--------------------------------------------------------------------------------

## Alternatives Considered

### Alternative 1: BIP 44 P2PKH for Standard Wallet Funds

* **Pros**: Matches the original Issue #35 draft and shares the P2PKH address format used for Token UTXOs.
* **Cons**: Larger transaction weight and higher fees than native SegWit (P2WPKH); diverges from the BIP 84 descriptors already implemented in `KeyDerivationPolicy`.
* **Reason for Rejection**: Standard wallet funds gain lower transaction weight and keep compatibility with the existing BIP 84 P2WPKH derivation in `KeyDerivationPolicy` (`m/84'/coin'/0'/{0,1}/*`), while Token UTXOs remain isolated on `m/44'/coin'/0'/0x6E616D65/i`.

### Alternative 2: Rewriting `KeyDerivationPolicy` to `Bip84Public` Watch-Only Descriptors with Manual Signing

* **Pros**: Prevents passing `Xpriv` into `bdk_wallet::Wallet` even in memory.
* **Cons**: Replaces two 1-line infallible `Bip84<Xpriv>` templates with fallible `Secp256k1` derivation and manual `psbt.sign(&self.master_key, &secp)` + `finalize_psbt` logic, without improving on-disk security because `bdk_wallet::ChangeSet` already persists only `Descriptor<DescriptorPublicKey>` (never private `KeyMap` signers) in SQLite's `bdk_*` tables while `wallet.sqlite` already stores the mnemonic and `DnsNostrWallet` already holds `master_key` in memory.
* **Reason for Rejection**: Keeping the existing `Bip84<Xpriv>` templates with `Wallet::load().descriptor(...).extract_keys()` and `Wallet::sign(...)` keeps SQLite public-descriptor-only, verifies persisted descriptors match the stored mnemonic on load, and removes manual key-derivation and PSBT-signing boilerplate.

### Alternative 3: Scanning Token UTXOs (`0x6E616D65`) in Issue #35

* **Pros**: Would track both Standard UTXOs and Token UTXOs in one sync pass from the start.
* **Cons**: Introduces Name-Token scanning and persistence before token mint/update/revoke operations are designed, expanding Issue #35 scope without user-facing token commands.
* **Reason for Rejection**: Out of scope until token operations are built; BDK's BIP 84 keychains already prevent standard coin selection from touching Token UTXOs.

### Alternative 4: Keeping `wallet create`, `wallet import`, or `wallet address` Offline

* **Pros**: Allows generating a mnemonic or receive address without a running Bitcoin Core node.
* **Cons**: Delays discovery of network/genesis misconfigurations until `balance` or `send`, prevents tip-anchoring on `create`, and lets `address` hand out an already-used index if the chain has advanced since the last command.
* **Reason for Rejection**: Failing fast on genesis mismatch and synchronizing confirmed blocks on every command avoids silent cross-network corruption and address reuse, at the trade-off of requiring a reachable Bitcoin Core node for all subcommands.

### Alternative 5: Scanning from Height `0` on `wallet create`

* **Pros**: Uses a single initialization code path for both `wallet create` and `wallet import`.
* **Cons**: Downloads and scans hundreds of thousands of historical blocks (over `1 TB` of hex-encoded RPC payloads on mainnet) for a freshly generated 128-bit random mnemonic that has no prior on-chain history.
* **Reason for Rejection**: Anchoring at the current chain tip avoids an unnecessary full-chain download on `wallet create` while preserving height-`0` recovery for `wallet import`.

### Alternative 6: Renaming `wallet address` to `wallet bitcoin-address`

* **Pros**: Explicitly distinguishes standard Bitcoin receive addresses from future Name-Token addresses.
* **Cons**: Breaks existing CLI usage (`just wallet address`) and adds verbosity to the most common wallet command.
* **Reason for Rejection**: Retaining `wallet address` preserves backward compatibility and keeps the CLI concise without pre-committing to future Name-Token command naming.

### Alternative 7: Satoshi CLI Amounts (`wallet send <address> <sats>`)

* **Pros**: Avoids decimal parsing entirely by accepting integer satoshis.
* **Cons**: Forces users to convert between BTC and satoshis manually and mismatches `just fund <BTC> <Address>` (the local regtest funding task) and Bitcoin Core CLI conventions.
* **Reason for Rejection**: Decimal BTC provides uniform units across `wallet balance`, `wallet send <address> <btc>`, and `just fund`, while fixed-point parsing in `bitcoin::Amount` avoids floating-point precision loss.

### Alternative 8: `--fee-rate`, `--yes`, and `--dry-run` Flags on `wallet send`

* **Pros**: Gives users a fee preview, confirmation prompt, or manual fee override before broadcasting on mainnet.
* **Cons**: Expands CLI surface, parsing, and interactive prompt handling beyond the minimal scriptable workflow targeted by Issue #35.
* **Reason for Rejection**: Omitting optional flags keeps `wallet send <address> <btc>` minimal and scriptable for Issue #35; interactive previews and fee overrides can be added in a follow-up issue if mainnet UX requires them.

### Alternative 9: Keeping `bdk_wallet 1.2.0` (`bdk_core 0.4.1`) Alongside `bdk_bitcoind_rpc 0.22.0` (`bdk_core 0.6.3`)

* **Pros**: Avoids upgrading `bdk_wallet` in `dns_nostr_wallet/Cargo.toml`.
* **Cons**: Fails to compile when passing `CheckPoint` or `BlockId` directly between `bdk_bitcoind_rpc::Emitter` and `bdk_wallet`, forcing duplicate BDK dependency trees and manual checkpoint translation adapters.
* **Reason for Rejection**: Upgrading `dns_nostr_wallet` to `bdk_wallet 2.3` (`features = ["rusqlite"]`) unifies the workspace on `bdk_chain 0.23.3` and `bdk_core 0.6.3` with zero cross-version translation adapters.

### Alternative 10: Exclusive `.lock` File or SQLite Locking for Parallel CLI Runs

* **Pros**: Serializes concurrent CLI processes across sync, coin selection, broadcast, and persistence.
* **Cons**: External `.lock` files add OS-specific advisory locking code; SQLite-only optimistic version checks cannot prevent two parallel `wallet send` processes from broadcasting conflicting transactions before persistence; holding a SQLite write lock across RPC I/O blocks concurrent read-only queries.
* **Reason for Rejection**: Parallel CLI execution against one `wallet.sqlite` is outside the single-process CLI model; avoiding extra locking machinery keeps persistence minimal while documenting the concurrency boundary in `Risks & Limits`.

### Alternative 11: Global Mempool Scanning (`Emitter::mempool()`)

* **Pros**: Automatically discovers unconfirmed external deposits (`untrusted_pending`) and delegates mempool eviction detection to `bdk_bitcoind_rpc::Emitter`.
* **Cons**: Because `dns_nostr_wallet` is a stateless CLI, every `address`, `balance`, `send`, and `import` run constructs a fresh `Emitter` and calls `getrawtransaction` for every transaction in Bitcoin Core's global mempool (`O(global_mempool_size)` sequential RPCs and memory), even though `wallet balance` and `wallet send` exclude `untrusted_pending` outputs anyway.
* **Reason for Rejection**: Checking only the wallet's own unconfirmed transactions via `get_mempool_entry` (`O(wallet_unconfirmed)` RPCs; zero RPCs when no self-sent tx is unconfirmed) keeps CLI latency predictable across all networks while still reconciling evicted self-sent transactions.

### Alternative 12: Calling `getblockchaininfo` for IBD and Pruned-Node Checks

* **Pros**: Fails fast when the connected node is pruned or still in Initial Block Download (IBD).
* **Cons**: Pruned nodes are out of scope for Issue #35, and Bitcoin Core reports `initialblockdownload: true` on paused regtest chains whose tip timestamp is older than 24 hours, requiring network-divergent checks that contradict uniform chain handling.
* **Reason for Rejection**: Requiring a synced, unpruned node as a documented operator precondition (`Technical Plan §8`) keeps RPC logic uniform across networks and defers pruned/IBD handling until needed.

### Alternative 13: Resumable Staging Import, `--birthday-height`, `scantxoutset`, Logarithmic Anchor Tails, and Internal Fee-Rate Caps

* **Pros**: Accelerates historical mainnet recovery, survives deep reorgs after `wallet create` without falling back to height `0`, and caps worst-case fee overpayment below Bitcoin Core's `10,000 sat/vB` `maxfeerate`.
* **Cons**: Staging files (`wallet.sqlite.importing`) add crash-cleanup lifecycles; `--birthday-height` and `DNS_NOSTR_MAX_FEE_RATE_SAT_VB` expand CLI/config surface beyond Issue #35's minimal scope; `scantxoutset` blocks Bitcoin Core RPC workers and misses transaction history; logarithmic tails add extra header-walk complexity.
* **Reason for Rejection**: Deferred to keep Issue #35's CLI surface, persistence lifecycle, and RPC adapter minimal; each trade-off is documented explicitly in `Risks & Limits`.

--------------------------------------------------------------------------------

## Non-Functional Requirements

### Security and Privacy

* **Secret redaction**: `EnvironmentConfig` redacts `bitcoin_rpc_password` as `"[REDACTED]"` in `fmt::Debug` (`Technical Plan §8`). `DnsNostrWallet` retains its custom `fmt::Debug` printing only `network` and redacting `master_key` and `bdk_wallet` descriptors.
* **File permissions and public-descriptor BDK persistence**: `wallet.sqlite` is created atomically with mode `0600` on Unix (via `OpenOptionsExt::mode(0o600)` before opening in SQLite) to avoid a default-umask permission window. BDK's SQLite `ChangeSet` persists public descriptors only (`Technical Plan §5`).
* **CLI argument hygiene**: Passing seed words as positional arguments (`wallet import <words...>`) exposes the mnemonic to shell history and process listings (`/proc/<pid>/cmdline`); operators on shared hosts should pipe the mnemonic over `stdin` instead.

### Performance and Latency

* **`wallet create`**: Issues a fixed upper bound of RPC calls independent of chain height (genesis check, best block hash, and up to `10` `getblockheader` lookups) and downloads zero full blocks.
* **Incremental sync and self-tx eviction**: `wallet address`, `wallet balance`, and `wallet send` download only newly confirmed blocks since the persisted checkpoint and execute `O(wallet_unconfirmed)` `get_mempool_entry` RPCs (`0` mempool RPCs when no self-sent transaction is unconfirmed).
* **`wallet import`**: Performs a uniform height-`0` confirmed-block scan across all networks, scaling linearly with chain height and targeting a local unpruned Bitcoin Core node.

### Upgrade and Rollback

* **No legacy SQLite migration**: Because `dns_nostr_wallet` is pre-release, upgrading `bdk_wallet` from `1.2.0` to `2.3` omits legacy SQLite schema migration shims. If an existing local regtest `wallet.sqlite` is incompatible with `bdk_wallet 2.3`, deleting `wallet.sqlite` and re-running `wallet import` recovers confirmed UTXO state from the backed-up mnemonic.

--------------------------------------------------------------------------------

## Risks & Limits

| Risk / Limit | Impact | Mitigation / Accepted Boundary |
| --- | --- | --- |
| **Immediate broadcast with no wallet-side fee ceiling** | `wallet send` signs and broadcasts immediately with no `--dry-run`, `--yes`, `--fee-rate`, or wallet-side maximum fee cap; an anomalous RPC fee estimate up to Bitcoin Core's `10,000 sat/vB` default `maxfeerate` can burn `0.014+ BTC` on a single 141 vB spend (and more on multi-input spends). | Relies on Bitcoin Core's `sendrawtransaction` default `maxfeerate` (`0.10 BTC/kvB`); operator precondition documented in `Technical Plan §4`. |
| **Ambiguous broadcast timeout** | If `sendrawtransaction` times out or drops the TCP connection after Bitcoin Core has already accepted and relayed the transaction, `SendBitcoinUsecase` skips `record_broadcasted_transaction` and exits non-zero without printing `<txid>`; blindly retrying `wallet send` risks a duplicate payment. | Before retrying a failed `wallet send`, run `wallet balance` (after confirmation) or inspect the node's mempool to verify whether the first transaction was broadcast. |
| **No unconfirmed external deposit visibility** | Without global `Emitter::mempool()` scanning, external incoming payments do not appear in `wallet balance` or `InsufficientFunds` errors until 1 confirmation. | Accepted trade-off (`Alternative 11`) to eliminate `O(global_mempool_size)` RPC fan-out across all networks; wallet-originated unconfirmed change (`trusted_pending`) remains immediately spendable. |
| **>10-block reorg fallback on freshly created wallets** | `fetch_tip_anchor` anchors only 10 headers onto height `0`; if a reorg deeper than 10 blocks occurs immediately after `wallet create`, `Emitter` walks back to height `0` and triggers a full-chain scan. | Accepted boundary for Issue #35 (`Alternative 13`); shallow reorgs (`<= 10` blocks) recover incrementally. |
| **Non-resumable full scan, unpruned-node requirement, and 25-address gap limit** | Interrupting a height-`0` `wallet import` discards scan progress; pruned nodes cannot serve historical blocks; BDK's default 25-address lookahead gap limit misses historical outputs beyond 25 consecutive unused addresses (including wallets where `wallet address` was invoked `> 25` consecutive times without receiving funds). | Atomic single-commit import avoids partial-import DB states; requires an unpruned local node and addresses within BDK's 25-address lookahead. |
| **`LocalChain` SQLite growth after mainnet height-`0` import** | Every scanned block checkpoint is persisted in `bdk_blocks`, increasing `load()` time and DB size on long chains. | Accepted limit for pre-release local-node usage. |
| **No IBD detection** | Skipping `getblockchaininfo` (`Technical Plan §7`, `Alternative 12`) means pointing `wallet create` at a node still in Initial Block Download (IBD) anchors at a stale intermediate height (forcing a later rescan), and `balance`/`send` see stale UTXOs. | Operator must point `dns_nostr_wallet` at a synced Bitcoin Core node (`Technical Plan §8`). |
| **25-tx mempool ancestor limit** | Chaining more than 25 consecutive unconfirmed sends hits Bitcoin Core's default mempool ancestor/descendant limit (`too-long-mempool-chain`). | Consecutive unconfirmed sends beyond 25 must wait for a confirmation. |
| **Post-broadcast persistence failure** | If SQLite fails to commit after `sendrawtransaction` succeeds, local state lags behind the chain until the next confirmed block sync. | `SendBitcoinUsecaseError::BroadcastSucceededButPersistFailed { txid, reason }` prints the broadcast `<txid>` to `stderr`; the next sync after confirmation reconciles local state. |
| **Single-process concurrency** | Running two CLI commands simultaneously against one `wallet.sqlite` can fail with `SQLITE_BUSY` or broadcast conflicting double-spends from the same UTXOs. | Documented single-process CLI boundary (`Technical Plan §8`, `Alternative 10`); no concurrent invocations supported. |
| **Plaintext mnemonic at rest and cleartext HTTP RPC** | `wallet.sqlite` stores the mnemonic unencrypted (`0600` permissions apply only on Unix); `DNS_NOSTR_BITCOIN_RPC_URL` accepts `https://` (matching `dns_nostr_server`), but `bitcoincore-rpc 0.19.0` lacks TLS and fails to connect over `https://`. | Atomic `0600` creation on Unix; use `http://` over loopback (`127.0.0.1`) or an externally encrypted tunnel (`Technical Plan §8`). |

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is partitioned into five incremental phases. Each phase leaves the workspace buildable and testable before the next phase starts.

### Phase 1: Workspace BDK Alignment and Lazy SQLite Repository

#### 1. `Cargo.toml`, `dns_nostr_server/Cargo.toml`, `dns_nostr_wallet/Cargo.toml`, and `Cargo.lock` (Modify)

* **Type / Scope**: Workspace and crate dependency manifests + lockfile.
* **Proposed Changes**:
  * In root `Cargo.toml`, add `bdk_bitcoind_rpc = "0.22.0"` to `[workspace.dependencies]`.
  * In `dns_nostr_server/Cargo.toml`, switch `bdk_bitcoind_rpc = "0.22.0"` to `bdk_bitcoind_rpc.workspace = true`.
  * In `dns_nostr_wallet/Cargo.toml`, upgrade `bdk_wallet` from `"1.0"` to `"2.3"` (`features = ["rusqlite"]`, aligning the crate onto `bdk_chain 0.23.3`, `bdk_core 0.6.3`, and `bdk_wallet::rusqlite` `0.31.x`) and add `bdk_bitcoind_rpc.workspace = true`. Retain `rusqlite = { version = "0.31.0", features = ["bundled"] }` temporarily through Phases 1–3 until Phase 4 removes `use rusqlite::Connection;` from `dns_nostr_wallet/src/main.rs`.
  * Update `Cargo.lock` via Cargo resolution and confirm via `cargo tree -d` that only one version each of `bdk_chain` (`0.23.3`), `bdk_core` (`0.6.3`), and `rusqlite` (`0.31.0`) appears.

#### 2. `dns_nostr_wallet/src/adapters/sqlite_name_token_wallet_repository_adapter.rs` (Modify)

* **Type / Scope**: SQLite persistence adapter and unit tests.
* **Proposed Changes**:
  * Refactor internal storage from `connection: Mutex<Connection>` into a private storage enum supporting both lazy path-backed operation (`pub fn open(db_path: PathBuf) -> Self`) and injected-connection testing (`pub fn new(connection: Connection) -> Self`, `pub fn in_memory() -> Result<Self, bdk_wallet::rusqlite::Error>`), using `bdk_wallet::rusqlite` types exclusively.
  * Implement `NameTokenWalletRepositoryPort`.
  * Unify SQL operations behind private shared helpers (`load_from_conn`, `create_in_conn`, `save_in_conn`) so path-backed and injected-connection modes run the same queries:
    * In `in_memory()`: open an in-memory `Connection` and run `Self::init_tables(&tx)` once in a committed transaction so an empty in-memory DB has initialized tables and returns `Ok(None)` on `load()`.
    * In `load()`:
      * For path-backed storage: if `!db_path.exists()`, return `Ok(None)` immediately without creating `wallet_dir` or `wallet.sqlite`; otherwise open with `Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)` (omitting `SQLITE_OPEN_CREATE` so SQLite can recover hot journals without creating missing files).
      * In `load_from_conn(conn)`: start a `DEFERRED` transaction (`conn.transaction_with_behavior(TransactionBehavior::Deferred)`), execute only `SELECT` queries (`Self::select_mnemonic(&tx)` and `ChangeSet::from_sqlite(&tx)`), and drop `tx` without calling `commit()` (never calling `Self::init_tables`). Map SQLite missing-table errors (`no such table`) on an existing database/connection to `WalletRepositoryError::CorruptedState(...)` so partial-schema corruption tests continue to return `CorruptedState` without `sqlite_master` `bdk_%` wildcard probing.
      * When `(Some(mnemonic_str), false)` is matched, derive `master_key` from `mnemonic` and `network`, then reconstruct `bdk_wallet` via `bdk_wallet::Wallet::load().descriptor(KeychainKind::External, Some(KeyDerivationPolicy::bitcoin_external_descriptor(&master_key))).descriptor(KeychainKind::Internal, Some(KeyDerivationPolicy::bitcoin_internal_descriptor(&master_key))).extract_keys().load_wallet_no_persist(changeset)` (mapping mismatch/load errors to `WalletRepositoryError::CorruptedState`).
    * In `create()`:
      * For path-backed storage: guard non-empty `db_path.parent()` (`parent.filter(|p| !p.as_os_str().is_empty())`) before `fs::create_dir_all(parent)`, create the file atomically with `0o600` on Unix (`OpenOptions::new().create(true).write(true).mode(0o600).open(&db_path)`), and open read-write in SQLite.
      * In `create_in_conn(conn, mnemonic, wallet)`: start a transaction, call `Self::init_tables(&tx)` (creating both `dns_nostr_wallet` and BDK tables), re-check existing mnemonic/changeset (`WalletAlreadyExists` if either is populated), insert mnemonic, persist `wallet.staged()` via `changeset.persist_to_sqlite(&tx)`, commit, and call `wallet.take_staged()`.
    * In `save()`:
      * For path-backed storage: return `WalletRepositoryError::CorruptedState` if `!db_path.exists()`, then open with `Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)` (no `SQLITE_OPEN_CREATE`).
      * In `save_in_conn(conn, wallet)`: verify the `dns_nostr_wallet` mnemonic row exists (`CorruptedState` if absent), persist `wallet.staged()` via `changeset.persist_to_sqlite(&tx)`, commit, and call `wallet.take_staged()`.
  * Update existing test `load_detects_corrupted_state_when_stored_mnemonic_is_invalid` (which currently locks `repo.connection` directly) to work with the refactored storage enum.
  * Add co-located unit tests verifying: path-backed `load()` on a missing path returns `Ok(None)` without creating `wallet_dir` or `wallet.sqlite`; path-backed `load()` on a cleanly committed wallet leaves `wallet.sqlite` byte-for-byte unchanged; and path-backed `create()` sets `0600` permissions on Unix and rejects duplicate creation without altering `wallet.sqlite`.

* **Phase 1 Checkpoint**: `cargo test -p dns_nostr_wallet` compiles and passes all existing and new Phase 1 wallet unit tests on `bdk_wallet 2.3` with lazy SQLite persistence.

---

### Phase 2: `BlockchainPort`, `SyncService`, Shared Offline Test Double, and `DnsNostrWallet` Sync/Balance/Signing

#### 3. `dns_nostr_wallet/src/domain/blockchain_port.rs` (Create)

* **Type / Scope**: Domain port trait, error enum, and shared `#[cfg(test)]` fake.
* **Proposed Changes**:
  * Define synchronous `BlockchainPort` trait:
    * `fn fetch_tip_anchor(&self, expected_genesis: BlockHash) -> Result<Vec<BlockId>, BlockchainError>;`
    * `fn sync(&self, wallet: &mut DnsNostrWallet) -> Result<(), BlockchainError>;`
    * `fn estimate_fee_rate(&self) -> Result<FeeRate, BlockchainError>;`
    * `fn broadcast(&self, tx: &Transaction) -> Result<Txid, BlockchainError>;`
  * Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum BlockchainError`:
    * `GenesisMismatch { expected: BlockHash, actual: BlockHash }` (`Display` must include both `expected` and `actual` hashes)
    * `Rpc(String)`
    * `ChainUpdate(String)`
    * `Broadcast(String)`
    Implement `Display` and `std::error::Error`.
  * Define a shared `#[cfg(test)] pub struct FakeBlockchain` implementing `BlockchainPort` using `Mutex`-backed interior mutability (so `&self` methods behind `Arc<dyn BlockchainPort>` can record calls and mutate scripted state), with configurable hooks: call counters for `fetch_tip_anchor`/`sync`/`broadcast`, scripted return values/errors, fee-rate value, block/eviction injection closures for `sync`, and captured broadcast `Vec<Transaction>`, so all application and CLI test modules reuse one fake.

#### 4. `dns_nostr_wallet/src/domain/sync_service.rs` (Create)

* **Type / Scope**: Domain service and unit tests.
* **Proposed Changes**:
  * Define `pub struct SyncService;` with:
    * `pub fn tip_anchor(&self, wallet: &mut DnsNostrWallet, blockchain: &dyn BlockchainPort) -> Result<(), SyncServiceError>`: queries `blockchain.fetch_tip_anchor(wallet.genesis_hash())` and applies the block IDs to the wallet via `wallet.apply_tip_anchor(block_ids)`.
    * `pub fn sync(&self, wallet: &mut DnsNostrWallet, blockchain: &dyn BlockchainPort) -> Result<(), SyncServiceError>`: executes `blockchain.sync(wallet)`.
  * Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum SyncServiceError { Blockchain(BlockchainError), Wallet(String) }` implementing `Display` and `std::error::Error`.
  * Add unit tests using `FakeBlockchain` verifying tip-anchor application and incremental sync error mapping.

#### 5. `dns_nostr_wallet/src/domain/dns_nostr_wallet.rs` (Modify)

* **Type / Scope**: Domain entity and unit tests.
* **Proposed Changes**:
  * Retain `#[allow(dead_code)]` on `master_key` (BDK signs via extracted in-memory signers; `master_key` remains for future Name-Token / Nostr key derivation).
  * Add accessors:
    * `pub fn network(&self) -> Network` (`self.bdk_wallet.network()`)
    * `pub fn genesis_hash(&self) -> BlockHash` (`self.bdk_wallet.local_chain().genesis_hash()`)
    * `pub fn latest_checkpoint(&self) -> CheckPoint` (`self.bdk_wallet.latest_checkpoint()`)
    * `pub fn unconfirmed_txids(&self) -> Vec<Txid>` (filter `self.bdk_wallet.transactions()` on `tx.chain_position.is_unconfirmed()` and collect `tx.tx_node.txid`)
    * `pub fn spendable_balance(&self) -> Amount` (`self.bdk_wallet.balance().trusted_spendable()`, equal to `confirmed + trusted_pending`)
  * Add chain update methods:
    * `pub fn apply_tip_anchor(&mut self, block_ids: Vec<BlockId>) -> Result<(), WalletError>`: treat an empty `block_ids` slice as a no-op; start from `self.bdk_wallet.latest_checkpoint()` (containing `(0, self.genesis_hash())`), validate any `height == 0` entry matches `self.genesis_hash()` (returning `WalletError::ChainUpdate` on mismatch), insert all non-zero `BlockId` entries in ascending height order, and apply `bdk_wallet::Update { chain: Some(cp), ..Default::default() }`, mapping errors to `WalletError::ChainUpdate`.
    * `pub fn apply_block_connected_to(&mut self, block: &Block, height: u32, connected_to: BlockId) -> Result<(), WalletError>`: call `self.bdk_wallet.apply_block_connected_to(block, height, connected_to)` and map `ApplyHeaderError` to `WalletError::ChainUpdate(error.to_string())`.
    * `pub fn apply_evicted_txs(&mut self, evicted: Vec<(Txid, u64)>)`: call `self.bdk_wallet.apply_evicted_txs(evicted)` (where `u64` is the Unix eviction timestamp in seconds).
    * `pub fn record_broadcasted_transaction(&mut self, tx: Transaction, seen_at: u64)`: call `self.bdk_wallet.apply_unconfirmed_txs([(tx, seen_at)])`.
  * Add `pub fn build_and_sign_transaction(&mut self, recipient: Address, amount: Amount, fee_rate: FeeRate) -> Result<Transaction, WalletError>`:
    * Collect unspendable outpoints from `self.bdk_wallet.list_unspent()` as defense-in-depth alongside BDK's maturity filtering: exclude any unconfirmed output on `KeychainKind::External` and any coinbase output (`self.bdk_wallet.get_tx(utxo.outpoint.txid)` where `wallet_tx.tx_node.tx.is_coinbase()`) that is unconfirmed or has fewer than `100` confirmations relative to `self.bdk_wallet.latest_checkpoint().height()` and `anchor.block_id.height`.
    * Build PSBT via `self.bdk_wallet.build_tx()`, chaining `add_recipient(recipient.script_pubkey(), amount)`, `fee_rate(fee_rate)`, `unspendable(excluded)`, and `finish()`.
    * Map `CreateTxError::CoinSelection(InsufficientFunds { needed, available })` to `WalletError::InsufficientFunds { needed, available, immature: self.bdk_wallet.balance().immature }`, and map all other `CreateTxError` variants to `WalletError::TxBuild(error.to_string())`.
    * Sign and finalize via `self.bdk_wallet.sign(&mut psbt, SignOptions::default())`, mapping `Err(error)` or `Ok(false)` (incomplete finalization) to `WalletError::Signing(String)`.
    * Extract the signed transaction via `psbt.extract_tx()`, mapping errors to `WalletError::Finalization(error.to_string())`.
  * Extend `WalletError` with `ChainUpdate(String)`, `InsufficientFunds { needed: Amount, available: Amount, immature: Amount }`, `TxBuild(String)`, `Signing(String)`, and `Finalization(String)`.
  * Add co-located unit tests covering: tip anchoring at `tip_height == 0`, `< 10`, `>= 100`, empty input, and height-`0` genesis mismatch; block sync + reorg rollback; `apply_evicted_txs` removing a recorded unconfirmed spend; immature coinbase exclusion vs. `trusted_pending` change inclusion; and transaction signing.

#### 6. `dns_nostr_wallet/src/adapters/sqlite_name_token_wallet_repository_adapter.rs` (Modify)

* **Type / Scope**: Persistence adapter unit tests.
* **Proposed Changes**:
  * Add co-located tests verifying: (a) a wallet saved and reloaded via `SqliteNameTokenWalletRepositoryAdapter::save` + `load` restores in-memory signers via `extract_keys()` and successfully builds and signs a transaction; and (b) loading a database whose stored BDK descriptors do not match the stored mnemonic returns `WalletRepositoryError::CorruptedState`.

#### 7. `dns_nostr_wallet/src/domain/mod.rs` (Modify)

* **Type / Scope**: Module declaration.
* **Proposed Changes**: Add `pub mod blockchain_port;` and `pub mod sync_service;`.

* **Phase 2 Checkpoint**: `cargo test -p dns_nostr_wallet` passes all domain entity, tip anchoring, reorg, self-tx eviction, UTXO eligibility, and post-reload signing unit tests.

---

### Phase 3: Bitcoin Core RPC Adapter and Environment Configuration

#### 8. `dns_nostr_wallet/src/adapters/rpc_blockchain_adapter.rs` (Create)

* **Type / Scope**: Infrastructure adapter and unit tests.
* **Proposed Changes**:
  * Import Bitcoin Core RPC types from `bdk_bitcoind_rpc::bitcoincore_rpc` (matching `dns_nostr_server/src/blockchain_watcher.rs`).
  * Define `RpcBlockchainAdapter` holding `rpc_url: String`, `rpc_user: String`, and `rpc_password: String`, with custom `fmt::Debug` redacting `rpc_password` as `"[REDACTED]"`, `pub fn new(rpc_url: impl Into<String>, rpc_user: impl Into<String>, rpc_password: impl Into<String>) -> Self`, and private methods `fn client(&self) -> Result<Client, BlockchainError>` and `fn verify_genesis(client: &Client, expected: BlockHash) -> Result<(), BlockchainError>` (calling `client.get_block_hash(0)`).
  * Implement `BlockchainPort`:
    * `fetch_tip_anchor`: call `Self::verify_genesis(&client, expected_genesis)?`; query `client.get_best_block_hash()?` and walk backward for at most `MAX_ANCHOR_HEADERS = 10` total headers via `client.get_block_header_info(&hash)?` (stopping early when `height == 0`, mapping `u32::try_from(header.height)` failures or `previous_block_hash == None` while `height > 0` to `BlockchainError::Rpc`), reverse into ascending height order, and return `Ok(block_ids)`.
    * `sync`: call `Self::verify_genesis(&client, wallet.genesis_hash())?`; construct `Emitter::new(&client, wallet.latest_checkpoint(), 0 /* start_height */, std::iter::empty::<Arc<Transaction>>())` (allowing height-`0` import scans while tip-anchored wallets resume from `latest_checkpoint()`); drain `emitter.next_block()?` into `wallet.apply_block_connected_to(...)` (mapping `WalletError` to `BlockchainError::ChainUpdate`); then read `wallet.unconfirmed_txids()` and, for each txid, call `client.get_mempool_entry(&txid)`:
      * `Ok(_)` → still in mempool.
      * Bitcoin Core JSON-RPC error code `-5` (`RPC_INVALID_ADDRESS_OR_KEY`, transaction not in mempool) → collect `(txid, unix_now)` for eviction.
      * Any other RPC error (transport, authentication, server error) → propagate immediately as `BlockchainError::Rpc`.
      * If any txids were evicted, call `wallet.apply_evicted_txs(evicted)`.
    * `estimate_fee_rate`: call `client.estimate_smart_fee(2, Some(EstimateMode::Economical))` and `client.get_mempool_info()` (propagating RPC/JSON errors as `BlockchainError::Rpc`). Convert `smart_fee: Option<FeeRate>` from `EstimateSmartFeeResult.fee_rate`, consume `GetMempoolInfoResult::{mempool_min_fee, min_relay_tx_fee}` directly as `bitcoin::Amount` values in `BTC/kvB`, convert each `Amount` to `sat/kwu` via integer ceiling division (`FeeRate::from_sat_per_kwu(amount.to_sat().div_ceil(4))`), compute `node_min_fee = mempool_min_fee.max(min_relay_tx_fee)` and `floor = FeeRate::from_sat_per_kwu(250)` (`1 sat/vB`), and return `smart_fee.unwrap_or(node_min_fee).max(node_min_fee).max(floor)`.
    * `broadcast`: call `client.send_raw_transaction(tx)`, mapping errors to `BlockchainError::Broadcast`.
  * Add co-located unit tests for `BTC/kvB` conversion (both fractional precision `1.5 sat/vB -> 375 sat/kwu` and ceiling rounding `1,501 sat/kvB -> 376 sat/kwu`), fee fallback precedence, `-5` mempool-eviction error classification vs. transport error propagation, password redaction in `Debug`, and genesis-mismatch error formatting.

#### 9. `dns_nostr_wallet/src/adapters/environment_config.rs` (Modify)

* **Type / Scope**: Configuration adapter and unit tests.
* **Proposed Changes**:
  * Add `BITCOIN_RPC_URL_VARIABLE` (`DNS_NOSTR_BITCOIN_RPC_URL`), `BITCOIN_RPC_USER_VARIABLE` (`DNS_NOSTR_BITCOIN_RPC_USER`), and `BITCOIN_RPC_PASSWORD_VARIABLE` (`DNS_NOSTR_BITCOIN_RPC_PASSWORD`) and corresponding fields on `EnvironmentConfig`, scoping single-use default constants (`"http://127.0.0.1:18443"`, `"rpcuser"`, `"rpcpassword"`) inside `from_lookup`.
  * Extend `ConfigError` with `EmptyValue { variable: &'static str }` and `InvalidUrl { variable: &'static str, value: String, expected_schemes: &'static str }`, validating `DNS_NOSTR_BITCOIN_RPC_URL` (`http://` or `https://` + non-empty, whitespace-free authority, matching `dns_nostr_server/src/environment_config.rs`), trimmed non-empty `DNS_NOSTR_BITCOIN_RPC_USER`, and non-empty, non-whitespace-only `DNS_NOSTR_BITCOIN_RPC_PASSWORD` preserved without trimming.
  * Replace `#[derive(Debug)]` on `EnvironmentConfig` with custom `fmt::Debug` redacting `bitcoin_rpc_password` as `"[REDACTED]"`.
  * Add unit tests covering defaults, overrides, invalid URLs, empty/whitespace user and password rejection, preserved password whitespace, and password redaction.

#### 10. `dns_nostr_wallet/src/adapters/mod.rs` (Modify)

* **Type / Scope**: Module declaration.
* **Proposed Changes**: Add `pub mod rpc_blockchain_adapter;`.

* **Phase 3 Checkpoint**: `cargo test -p dns_nostr_wallet` passes all RPC adapter and environment config validation unit tests.

---

### Phase 4: Application Use Cases, CLI Controller, and Composition Root

#### 11. `dns_nostr_wallet/src/application/create_wallet_usecase.rs` (Modify / Rename)

* **Type / Scope**: Application use case and unit tests.
* **Proposed Changes**:
  * Rename module file from `create_wallet.rs` to `create_wallet_usecase.rs`.
  * Update struct to `CreateWalletUsecase::new(network: Network, repository: Arc<dyn NameTokenWalletRepositoryPort>, blockchain: Arc<dyn BlockchainPort>, sync_service: Arc<SyncService>)`.
  * Define `CreateWalletUsecaseError` (`#[derive(Debug, Clone, PartialEq, Eq)]`) with `Blockchain(BlockchainError)`, `Sync(SyncServiceError)`, `WalletAlreadyExists`, and `Wallet(String)`.
  * In `execute()`: check `self.repository.load()?.is_some()` (`WalletAlreadyExists`) before any RPC call, generate the 12-word mnemonic, initialize in-memory `DnsNostrWallet`, call `self.sync_service.tip_anchor(&mut wallet, &*self.blockchain)?`, and commit via `self.repository.create(&mnemonic, &mut wallet)?`.
  * Update co-located tests using `FakeBlockchain`, verifying RPC failure does not persist a wallet (`repository.load()` remains `Ok(None)`) and duplicate creation performs zero RPC calls.

#### 12. `dns_nostr_wallet/src/application/import_wallet_usecase.rs` (Modify / Rename)

* **Type / Scope**: Application use case and unit tests.
* **Proposed Changes**:
  * Rename module file from `import_wallet.rs` to `import_wallet_usecase.rs`.
  * Update struct to `ImportWalletUsecase::new(network: Network, repository: Arc<dyn NameTokenWalletRepositoryPort>, blockchain: Arc<dyn BlockchainPort>, sync_service: Arc<SyncService>)`.
  * Define `ImportWalletUsecaseError` (`#[derive(Debug, Clone, PartialEq, Eq)]`) with `Blockchain(BlockchainError)`, `Sync(SyncServiceError)`, `InvalidMnemonic(String)`, `WalletAlreadyExists`, and `Wallet(String)`.
  * In `execute(phrase)`: parse the mnemonic, check `self.repository.load()?.is_some()` (`WalletAlreadyExists`) before any RPC call, build in-memory `DnsNostrWallet` (starting at height `0`), call `self.sync_service.sync(&mut wallet, &*self.blockchain)?`, and commit via `self.repository.create(&mnemonic, &mut wallet)?`.
  * Update co-located tests using `FakeBlockchain` covering: invalid mnemonic performs `0` RPC calls; duplicate import returns `WalletAlreadyExists` with `0` RPC calls; RPC `sync` failure returns `ImportWalletUsecaseError::Sync` without persisting (`repository.load()` remains `Ok(None)`); and historical confirmed UTXOs synced during import persist atomically.

#### 13. `dns_nostr_wallet/src/application/get_receive_address_usecase.rs` (Modify / Rename)

* **Type / Scope**: Application use case and unit tests.
* **Proposed Changes**:
  * Rename module file from `get_receive_address.rs` to `get_receive_address_usecase.rs`.
  * Update struct to `GetReceiveAddressUsecase::new(network: Network, repository: Arc<dyn NameTokenWalletRepositoryPort>, blockchain: Arc<dyn BlockchainPort>, sync_service: Arc<SyncService>)`.
  * Extend `GetReceiveAddressUsecaseError` (`#[derive(Debug, Clone, PartialEq, Eq)]`) with `NetworkMismatch { configured: Network, stored: Network }`, `Blockchain(BlockchainError)`, and `Sync(SyncServiceError)`.
  * In `execute()`: load wallet (`WalletNotFound` if `None`), verify `wallet.network() == self.network`, call `self.sync_service.sync(&mut wallet, &*self.blockchain)?`, reveal next BIP 84 receive address, and persist via `self.repository.save(&mut wallet)?`.
  * Add offline unit tests using `FakeBlockchain` covering: `mainnet` (`bc1q...`), `testnet` (`tb1q...`), and `regtest` (`bcrt1q...`) prefixes; `NetworkMismatch` rejection with `0` RPC calls; and `sync` failure propagating `GetReceiveAddressUsecaseError` without advancing or persisting the receive address index.

#### 14. `dns_nostr_wallet/src/application/get_balance_usecase.rs` (Create)

* **Type / Scope**: Application use case and unit tests.
* **Proposed Changes**:
  * Define `GetBalanceUsecase::new(network: Network, repository: Arc<dyn NameTokenWalletRepositoryPort>, blockchain: Arc<dyn BlockchainPort>, sync_service: Arc<SyncService>)` and `execute(&self) -> Result<Amount, GetBalanceUsecaseError>`.
  * Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum GetBalanceUsecaseError { WalletNotFound, NetworkMismatch { configured: Network, stored: Network }, Blockchain(BlockchainError), Sync(SyncServiceError), Wallet(String) }` with `Display`, `Error`, and `From` conversions (mapping `WalletRepositoryError` into `Wallet(String)`, matching `GetReceiveAddressUsecaseError`).
  * In `execute()`: load wallet, verify network, run `self.sync_service.sync(&mut wallet, &*self.blockchain)?`, persist via `self.repository.save(&mut wallet)?`, and return `Ok(wallet.spendable_balance())`.
  * Add co-located offline unit tests verifying: `confirmed + trusted_pending` inclusion, immature coinbase exclusion, `repository.save()` persisting sync progress, `WalletNotFound`, `NetworkMismatch`, and `Blockchain`/`Sync` error propagation.

#### 15. `dns_nostr_wallet/src/application/send_bitcoin_usecase.rs` (Create)

* **Type / Scope**: Application use case and unit tests.
* **Proposed Changes**:
  * Define `SendBitcoinUsecase::new(network: Network, repository: Arc<dyn NameTokenWalletRepositoryPort>, blockchain: Arc<dyn BlockchainPort>, sync_service: Arc<SyncService>)` and `execute(&self, address: &str, amount_btc: &str) -> Result<Txid, SendBitcoinUsecaseError>`.
  * Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum SendBitcoinUsecaseError`:
    * `InvalidAddress(String)`
    * `AddressNetworkMismatch { expected: Network }`
    * `InvalidAmount(String)`
    * `DustAmount { amount: Amount, minimum: Amount }`
    * `WalletNotFound`
    * `NetworkMismatch { configured: Network, stored: Network }`
    * `InsufficientFunds { needed: Amount, available: Amount, immature: Amount }` (`Display` includes `needed`, `available`, and `immature`)
    * `Blockchain(BlockchainError)`
    * `Sync(SyncServiceError)`
    * `Wallet(String)` (maps `WalletRepositoryError` on `load` and pre-broadcast `save`, plus non-`InsufficientFunds` `WalletError` variants)
    * `BroadcastSucceededButPersistFailed { txid: Txid, reason: String }` (`Display` includes `txid` and `reason`)
  * Execution sequence:
    1. Before loading the wallet or making RPC calls: parse `address` with `Address::from_str` (`InvalidAddress`) and check `require_network(self.network)` (`AddressNetworkMismatch`); parse `amount_btc` with `Amount::from_str_in(amount_btc, Denomination::Bitcoin)` (rejecting negative values, non-numeric input, and `> 8` decimal places) and reject `amount == Amount::ZERO` (`InvalidAmount`); reject `amount < recipient.script_pubkey().minimal_non_dust()` (`DustAmount`).
    2. Load wallet (`WalletNotFound`) and verify `wallet.network() == self.network` (`NetworkMismatch`).
    3. Run `self.sync_service.sync(&mut wallet, &*self.blockchain)?` and pre-broadcast save (`self.repository.save(&mut wallet)?`).
    4. Resolve `fee_rate = self.blockchain.estimate_fee_rate()?`.
    5. Build and sign via `wallet.build_and_sign_transaction(recipient, amount, fee_rate)?` (mapping `WalletError::InsufficientFunds` to `SendBitcoinUsecaseError::InsufficientFunds`).
    6. Broadcast via `let txid = self.blockchain.broadcast(&tx)?;`.
    7. Record the transaction via `wallet.record_broadcasted_transaction(tx, unix_now)` (current Unix timestamp in seconds) and execute post-broadcast save (`self.repository.save(&mut wallet)`), mapping failure to `SendBitcoinUsecaseError::BroadcastSucceededButPersistFailed { txid, reason }`.
  * Add co-located offline unit tests (with a co-located repository test wrapper that fails on the Nth save) covering: `InvalidAddress`, `AddressNetworkMismatch`, `InvalidAmount` (`0`, negative, `> 8` decimals, non-numeric), and `DustAmount` all performing `0` RPC calls; `WalletNotFound` and `NetworkMismatch`; `InsufficientFunds` with `immature` context; failed `InsufficientFunds` / `broadcast` preserving pre-broadcast sync state while discarding the uncommitted internal change address; post-broadcast save failure returning `BroadcastSucceededButPersistFailed` with `<txid>`; and consecutive unconfirmed change spending.

#### 16. `dns_nostr_wallet/src/application/mod.rs` (Modify)

* **Type / Scope**: Module declaration.
* **Proposed Changes**: Update module names to `pub mod create_wallet_usecase;`, `pub mod get_receive_address_usecase;`, `pub mod import_wallet_usecase;`, `pub mod get_balance_usecase;`, and `pub mod send_bitcoin_usecase;`.

#### 17. `dns_nostr_wallet/src/adapters/cli_controller.rs` (Modify / Rename)

* **Type / Scope**: CLI adapter and unit tests.
* **Proposed Changes**:
  * Rename module file from `cli.rs` to `cli_controller.rs`.
  * Add `WalletCommand::Balance` and `WalletCommand::Send { address: String, amount_btc: String }` (positional arguments only; no optional flags).
  * Wire `GetBalanceUsecase` and `SendBitcoinUsecase` into `CliController`.
  * Format `Balance` as `amount.to_string_in(Denomination::Bitcoin)` and `Send` as `txid.to_string()`.
  * Add `GetBalance(GetBalanceUsecaseError)` and `SendBitcoin(SendBitcoinUsecaseError)` to `CliControllerError`.
  * Update co-located unit tests using `FakeBlockchain` and add CLI parsing and execution tests for `balance`, `send`, `testnet`/`mainnet` address prefixes, and unknown flag rejection.

#### 18. `dns_nostr_wallet/src/main.rs` and `dns_nostr_wallet/Cargo.toml` (Modify)

* **Type / Scope**: Composition root and crate manifest cleanup.
* **Proposed Changes**:
  * In `dns_nostr_wallet/src/main.rs`, remove eager `fs::create_dir_all`, `Connection::open`, `fs::set_permissions`, `use rusqlite::Connection;`, and `use std::fs;`.
  * Construct `repository = Arc::new(SqliteNameTokenWalletRepositoryAdapter::open(config.db_path()))`, `blockchain = Arc::new(RpcBlockchainAdapter::new(&config.bitcoin_rpc_url, &config.bitcoin_rpc_user, &config.bitcoin_rpc_password))`, and `sync_service = Arc::new(SyncService)`.
  * Construct `CreateWalletUsecase`, `ImportWalletUsecase`, `GetReceiveAddressUsecase`, `GetBalanceUsecase`, and `SendBitcoinUsecase` with `config.network`, `Arc::clone(&repository)`, `Arc::clone(&blockchain)`, and `Arc::clone(&sync_service)`, then inject all five use cases into `CliController::new(...)`.
  * In `dns_nostr_wallet/Cargo.toml`, remove the now-unused direct `rusqlite = { version = "0.31.0", features = ["bundled"] }` dependency (`bdk_wallet::rusqlite` and `bdk_chain 0.23.3`'s `rusqlite/bundled` feature provide it).

* **Phase 4 Checkpoint**: `cargo build --workspace && just test` passes all offline unit tests across every crate with zero warnings.

---

### Phase 5: End-to-End Regtest Suite and Documentation / Directory Contracts

#### 19. `end_to_end/src/bitcoin_token_tx.rs` (Modify)

* **Type / Scope**: E2E Bitcoin regtest harness.
* **Proposed Changes**:
  * Add `pub fn fund_address(&self, address: &Address, amount: Amount) -> Result<Txid, String>` calling `self.wallet_client.send_to_address(address, amount, None, None, None, None, None, None)` without mining, and refactor `RegtestTokenClient::mint_token` to reuse `self.fund_address(...)`.
  * Change `fn mine_one_block(&self)` to `pub fn mine_one_block(&self)`.
  * Raise `min_spendable_balance` in `RegtestTokenClient::new` from `0.01 BTC` (`1_000_000` sats) to `5.0 BTC` (`Amount::from_btc(5.0)`) so repeated runs against persistent regtest Docker volumes always have sufficient miner balance for both server and wallet scenarios.

#### 20. `end_to_end/src/main.rs` (Modify)

* **Type / Scope**: E2E binary composition root.
* **Proposed Changes**: Pass `&config` and `&bitcoin_client` into `wallet_scenarios::run_all(&config, &bitcoin_client)` and update the final summary `println!` to `println!("All 13 end-to-end scenarios passed.");` (5 server + 8 wallet scenarios).

#### 21. `end_to_end/src/wallet_scenarios.rs` (Modify)

* **Type / Scope**: Black-box E2E test suite.
* **Proposed Changes**:
  * Forward `DNS_NOSTR_BITCOIN_RPC_URL`, `DNS_NOSTR_BITCOIN_RPC_USER`, and `DNS_NOSTR_BITCOIN_RPC_PASSWORD` from `E2eConfig` into every `run_wallet` invocation.
  * In `network_env_overrides_and_invalid_network`: replace the `testnet`/`mainnet` address-prefix checks (now covered by offline unit tests in `get_receive_address_usecase.rs` and `cli_controller.rs`) with a cross-network genesis-mismatch scenario running `wallet create` with `DNS_NOSTR_NETWORK=mainnet` against the regtest node, asserting non-zero exit, expected/actual genesis hashes on `stderr`, and no `wallet.sqlite` created, while retaining the invalid `"dogecoin"` network assertion.
  * Keep `BIP39_VECTOR` (the existing fixed mnemonic used by the deterministic `import_args_and_address_auto_advance` scenario) unfunded so that scenario stays deterministic across reused regtest Docker volumes.
  * Add three regtest scenarios using wallets created via `wallet create`, each in its own independent `TempWalletDir`:
    1. **Receive & balance**: Fund a freshly created wallet's address with `1.5` BTC via `bitcoin_client.fund_address`, assert `wallet balance` prints `0` before confirmation, mine 1 block via `bitcoin_client.mine_one_block`, and assert `wallet balance` prints `1.5`.
    2. **Send & consecutive unconfirmed change spend**: Create and fund an independent sender wallet with `1.0` BTC (confirmed with 1 block) and create a recipient wallet in a separate `TempWalletDir`; send `0.25` BTC to the recipient's address, verify stdout contains only `<txid>`, assert the sender's `wallet balance` drops below `0.75` before confirmation (reflecting `trusted_pending` change minus fee), send a second `0.1` BTC to a second address of the recipient wallet before confirmation, query and record the sender's post-second-send `trusted_pending` `wallet balance`, mine 1 block, verify the recipient's `wallet balance` equals `0.35`, and verify the sender's confirmed `wallet balance` equals the recorded post-second-send balance.
    3. **Historical import recovery**: Import the mnemonic of a freshly created, funded (`0.5` BTC), and confirmed wallet into a new `TempWalletDir` and verify `wallet balance` prints `0.5`.

#### 22. Documentation and Directory Contracts (Modify)

* **Type / Scope**: Project documentation, domain model, and EGM directory contracts.
* **Proposed Changes**:
  * `AGENTS.md`: Update the top work-in-progress blockquote and Architecture wallet note (limiting exclusions to Name-Token operations), add `dns_nostr_wallet ──RPC──▶ Bitcoin Node (regtest)` to the Architecture ASCII diagram, update the `just wallet` command list (`create | import [WORDS...] | address | balance | send <address> <btc>`), document the architectural role postfix convention in Code Style, and add `DNS_NOSTR_BITCOIN_RPC_*` to `dns_nostr_wallet` environment variables.
  * `README.md`: Update human-facing summary of wallet capabilities.
  * `docs/domain.md`: Add `BlockchainPort` and `SyncService` entries; update `DnsNostrWallet`, `NameTokenWalletRepositoryPort`, and `Wallet use cases` (naming `CreateWalletUsecase`, `ImportWalletUsecase`, `GetReceiveAddressUsecase`, `GetBalanceUsecase`, and `SendBitcoinUsecase`) entries.
  * `dns_nostr_wallet/README.md`: Update System Overview, dependency list, and environment variables.
  * `dns_nostr_wallet/src/README.md`: Update `main.rs` wiring summary, Invariants, and Side Effects (moving directory creation and `0o600` permissions out of `main.rs`).
  * `dns_nostr_wallet/src/domain/README.md`: Document `blockchain_port.rs`, `sync_service.rs`, `name_token_wallet_repository_port.rs`, and updated `DnsNostrWallet` interfaces and invariants.
  * `dns_nostr_wallet/src/application/README.md`: Document `create_wallet_usecase.rs`, `import_wallet_usecase.rs`, `get_receive_address_usecase.rs`, `get_balance_usecase.rs`, and `send_bitcoin_usecase.rs`.
  * `dns_nostr_wallet/src/adapters/README.md`: Document `rpc_blockchain_adapter.rs`, `sqlite_name_token_wallet_repository_adapter.rs`, `cli_controller.rs`, and updated `environment_config.rs`.
  * `end_to_end/README.md` and `end_to_end/src/README.md`: Update wallet scenario count (8 scenarios), `wallet_scenarios::run_all` signature, and new `RegtestTokenClient` public methods (`fund_address`, `mine_one_block`).

### Verification Plan

* **Compilation/Build**: `cargo build --workspace` succeeds with zero warnings; `cargo tree -d` / `Cargo.lock` inspection confirms a single version each of `bdk_chain` (`0.23.3`), `bdk_core` (`0.6.3`), and `rusqlite` (`0.31.0`).
* **Unit Tests**: `just test` (`cargo test --workspace`) passes all offline unit tests across `name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`, and `end_to_end` (the workspace's domain, server, wallet, and E2E crates).
* **Full-Stack E2E Tests**: `just test e2e` starts regtest Bitcoin Core (`28.1-alpine`) and Nostr relay, builds `dns_nostr_server` and `dns_nostr_wallet`, and passes all 13 server and wallet E2E scenarios (including receive, pre-confirmation `0` balance, confirmed balance, consecutive unconfirmed sends spending `trusted_pending` change, historical `wallet import` recovery, and cross-network genesis-mismatch rejection without creating `wallet.sqlite`).

--------------------------------------------------------------------------------

*Drafted with the Elephant-Goldfish Model (EGM)*

### Verification Plan

* **Compilation/Build**: `cargo build --workspace` succeeds with zero warnings; `cargo tree -d` / `Cargo.lock` inspection confirms a single version each of `bdk_chain` (`0.23.3`), `bdk_core` (`0.6.3`), and `rusqlite` (`0.31.0`).
* **Unit Tests**: `just test` (`cargo test --workspace`) passes all offline unit tests across `name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`, and `end_to_end` (the workspace's domain, server, wallet, and E2E crates).
* **Full-Stack E2E Tests**: `just test e2e` starts regtest Bitcoin Core (`28.1-alpine`) and Nostr relay, builds `dns_nostr_server` and `dns_nostr_wallet`, and passes all 13 server and wallet E2E scenarios (including receive, pre-confirmation `0` balance, confirmed balance, consecutive unconfirmed sends spending `trusted_pending` change, historical `wallet import` recovery, and cross-network genesis-mismatch rejection without creating `wallet.sqlite`).

--------------------------------------------------------------------------------

*Drafted with the Elephant-Goldfish Model (EGM)*
