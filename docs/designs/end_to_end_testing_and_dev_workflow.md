# End-to-End Testing, `justfile` Orchestration, Server Environment Config & On-Demand Regtest

*   **Author:** Luigi Minardi
*   **Status:** Implemented
*   **Date:** 2026-11-24

--------------------------------------------------------------------------------

## Overview

DNS-Nostr Tokens implements the **Name-Token** protocol: unique names anchored on Bitcoin via `OP_FALSE OP_IF … OP_ENDIF` UTXO inscriptions (defined in [`docs/domain.md`](../domain.md)). In **DNS-Nostr**, a label's authoritative Token UTXO carries a 32-byte Nostr public key; `dns_nostr_server` indexes Bitcoin blocks into SQLite and resolves `<label>.nostr.dns.name.` by fetching the DNS zone file published as a Nostr text note by that key.

This design introduces automated black-box end-to-end verification for `dns_nostr_server` and `dns_nostr_wallet` alongside blocking sync and lifecycle fixes in `name_token` and `dns_nostr_server`. It also adds validated `DNS_NOSTR_*` environment-variable configuration to `dns_nostr_server`, replaces the custom `regtest_node/` Docker image with stock Bitcoin Core configured for on-demand block mining, and standardizes developer and coding-agent commands behind a root `justfile`.

Target users are developers and coding agents working in this repository. After this change, contributors run `just test` (or `cargo test --workspace`) for offline unit tests, `just serve` / `just wallet` / `just fund` for local development, and `just test e2e` to verify the Bitcoin regtest, Nostr relay, DNS server, and wallet CLI stack end-to-end in a single command (without manually running `just serve` in the background).

## The Problem

Five gaps currently block repeatable full-stack verification and local development in this repository:

1.  **No Automated End-to-End Verification:** `cargo test --workspace` executes only unit tests backed by mocks or in-memory stubs, and the existing `end_to_end_tests/` directory is empty. Regressions across Bitcoin UTXO inscriptions, block indexing, Nostr zone-file lookups, DNS authority responses, and `dns_nostr_wallet` CLI execution go undetected unless manually exercised with `bitcoin-cli`, Nostr clients, and `dig`.
2.  **Blocking Sync, Persistence, and Lifecycle Bugs in `dns_nostr_server` and `name_token`:**
    *   **Fresh-database infinite rescan:** On an empty SQLite database, `NameTokenService::tip()` returns `None`. `BlockchainWatcher::sync_blocks()` seeds `bdk_bitcoind_rpc::Emitter` at height `0`, so `Emitter` emits blocks starting at height `1`. Because `NameTokenService::apply_block_connected_to()` only initializes its in-memory `LocalChain` when `height == 0`, `self.chain` stays `None` for the entire server run, and `NameTokenService::new()` silently swallows `LocalChain::from_blocks` errors on restart. Every polling tick rescans from block `1`, duplicates rows in `chain_checkpoints`, and briefly rewinds updated or revoked tokens back to `Minted`.
    *   **Silent watcher death:** `BlockchainWatcher::spawn()` drops its `JoinHandle` and panics via `.expect()`/`.unwrap()` on transient Bitcoin RPC errors. Starting `dns_nostr_server` before Docker services are up, or restarting Docker containers, permanently kills block indexing while the UDP DNS listener stays bound and serves stale state.
    *   **Nostr client leakage and panic:** Each DNS lookup constructs a new `nostr_sdk::Client`, calls `.add_relay(...).await.unwrap()`, connects, and never shuts down—leaking WebSocket connections and background tasks per query and panicking on invalid relay URLs.
    *   **Startup, query, and reset fragility:** `SqliteNameTokenRepository::create()` opens `./data/name-tokens.sqlite` without creating parent directories, runs in default `DELETE`/`FULL` journal mode (causing slow per-block `fsync` stalls during initial sync), and lacks indexes on `name_tokens(label_hex)` and `name_tokens(last_txid, last_vout)`; `BlockchainWatcher` connects to `http://0.0.0.0:18443` instead of loopback; and wiping Docker regtest volumes leaves SQLite out of sync with the reset regtest chain history from block `1` onward.
3.  **Hardcoded Server Configuration and Fixed 10-Second Polling:** `dns_nostr_server` hardcodes its DNS bind address, zone origin, SQLite path, Bitcoin RPC URL/credentials, Nostr relay URL, and 10-second sync interval. Contributors cannot point the server at non-default endpoints or tighten the sync cadence without editing source code.
4.  **Custom Regtest Image and Background Auto-Mining:** `regtest_node/` maintains a custom Dockerfile that runs as `root` (`/root/.bitcoin`), mines a block every 10 minutes on a background timer, omits `-fallbackfee`, and fails when reloading the `miner` wallet if it is already loaded. Meanwhile, `docker compose up -d` returns before Bitcoin RPC (`:18443`) and Nostr relay (`:8080`) accept connections.
5.  **Unstandardized Developer and Agent Workflow:** Developers and agents must recall long `cargo run` and `docker compose exec bitcoin-cli` invocations, and `.agents/rules/agent-orchestration.md` requires only `cargo test --workspace` before finishing a task, allowing end-to-end regressions to pass unverified.

## Goals

*   Replace the empty `end_to_end_tests/` directory with a binary workspace member crate `end_to_end` (`src/main.rs`) whose offline unit tests run under `cargo test --workspace` / `just test` with zero network or Docker dependencies, while `cargo build --workspace` compiles all E2E code without feature flags.
*   Make `just test e2e` self-contained: `end_to_end` spawns the freshly built host `dns_nostr_server` binary against the shared SQLite database (`DNS_NOSTR_DB_PATH`, default `./data/name-tokens.sqlite`), waits for DNS readiness, runs all scenarios sequentially (fail-fast), and terminates the server cleanly on exit.
*   Verify five sequential `dns_nostr_server` scenarios against the managed `dns_nostr_server` process, live regtest Bitcoin node, and live Nostr relay using letter-prefixed unique timestamped labels, P2PKH-signed Token UTXOs matching [`docs/domain.md`](../domain.md), and apex `TXT` zone records holding the Nostr public key: `not found`, `minted`, `updated`, `first-is-root` (first confirmed mint beats a second competing mint), and `revoked`.
*   Verify black-box CLI execution of all currently implemented `dns_nostr_wallet` commands (`create`, `import` via CLI arguments and stdin, `address` auto-advance, `DNS_NOSTR_NETWORK` / `DNS_NOSTR_WALLET_DIR` overrides, error paths, and `0600` database permissions) in fresh temporary directories inside `end_to_end`.
*   Fix identified `name_token` and `dns_nostr_server` blockers: fresh-DB `self.chain == None` infinite rescan, root-cause startup missing-genesis detection in `NameTokenService::new()`, `chain_checkpoints` primary-key deduplication, SQLite WAL mode + parent-directory creation + `name_tokens` query indexes, `BlockchainWatcher` RPC error recovery + supervised `JoinHandle` fatal propagation, unconditional `NostrEventsRepository` client shutdown + invalid-URL handling, and portable `http://127.0.0.1:18443` RPC default.
*   Add validated `DNS_NOSTR_*` environment-variable configuration to `dns_nostr_server` (covering DNS bind address, zone origin, SQLite DB path, Bitcoin RPC URL/user/password, Nostr relay URL, and sync interval), mirroring `dns_nostr_wallet`'s `environment_config.rs` conventions, and reuse that single config parser inside `end_to_end`.
*   Remove `regtest_node/` and configure upstream `bitcoin/bitcoin:28.1-alpine` in `docker-compose.yml` with explicit regtest CLI flags (including `-fallbackfee=0.0002` and `-acceptnonstdtxn=1` so Bitcoin Core accepts non-standard inscribed scripts), `/home/bitcoin/.bitcoin` volume mount, loopback host-port bindings, service healthchecks, and preserved Docker volumes across stop/start cycles.
*   Provide a root `justfile` exposing four top-level recipes: `wallet` (prepending the `wallet` subcommand), `serve`, `fund <BTC> <Address>`, and `test [up|down|e2e]`.
*   Update `AGENTS.md`, `.agents/rules/agent-orchestration.md`, and affected per-directory `README.md` contracts (Elephant-Goldfish Model / EGM contracts documenting `Interface`, `Invariants`, `Side Effects`, and `Verification`) to document `just` as a prerequisite, sync all interfaces and invariants, and require `just test e2e` for changes touching server, wallet CLI, protocol, or infrastructure code.

## Non-Goals

*   Implementing Name-Token mint, update, transfer, or revoke operations, UTXO scanning, or Bitcoin send/receive inside `dns_nostr_wallet`.
*   Running Bitcoin chain reorganization (`invalidateblock`) tests in `end_to_end` (reorg handling stays covered by unit tests).
*   Verifying second-token promotion (falling back to a competing earlier-ignored mint after the root token is revoked) in the `first-is-root` scenario.
*   Containerizing `dns_nostr_server` in `docker-compose.yml` or creating a temporary per-run SQLite database inside `end_to_end` (`end_to_end` spawns the host-built `dns_nostr_server` binary against the shared `DNS_NOSTR_DB_PATH` SQLite database).
*   Adding CLI scenario filtering or continue-on-failure summary reporting to the `end_to_end` binary (scenarios execute sequentially and fail fast on the first error).
*   Adding Bitcoin ZMQ block-push notifications, custom multi-transaction block-packing harnesses, a new `NameTokenService` rollback-without-new-block API, DNS `NXDomain` → `ServFail` authority semantics changes, or a persistent `nostr_sdk::Client` connection pool.
*   Automatically wiping Docker volumes during `just test down`.

## Open Questions

None.

## Critical User Journeys (CUJs)

### CUJ 1: Offline Unit Verification (`just test` / `cargo test --workspace`)

*   **Given**: A contributor or coding agent has checked out the repository without starting Docker or `dns_nostr_server`.
*   **When**: They run `just test` (or `cargo test --workspace`).
*   **Then**: Cargo compiles all workspace crates—including the `end_to_end` binary crate and all of its scenario modules—and executes every unit test across `name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`, and `end_to_end` with zero network or Docker dependencies. `end_to_end`'s `main()` does not run.

### CUJ 2: On-Demand Regtest Infrastructure & Funding (`just test up`, `just fund`, `just test down`)

*   **Given**: Docker and `just` are installed.
*   **When**: The user runs `just test up`, then `just fund <BTC> <Address>`, and later `just test down`.
*   **Then**: `just test up` starts `bitcoin-core` (`bitcoin/bitcoin:28.1-alpine`) and `nostr-relay` (`scsibug/nostr-rs-relay:0.8.8`) and waits via `--wait` until both healthchecks pass. `just fund <BTC> <Address>` ensures the `miner` wallet is loaded or created, mines coinbase maturity blocks only when chain height is below `101`, delegates amount/address/balance checks to `bitcoin-cli -rpcwallet=miner sendtoaddress`, and mines 1 confirmation block. `just test down` stops and removes containers without deleting the `bitcoin-core` or `nostr-relay` Docker volumes.

### CUJ 3: Running `dns_nostr_server` with Environment Configuration (`just serve`)

*   **Given**: The user wants to run the DNS server locally with default or custom `DNS_NOSTR_*` environment variables (for example, a custom port, SQLite path, or shorter sync interval).
*   **When**: They run `just serve` (either before or after `just test up`).
*   **Then**: `dns_nostr_server` validates all `DNS_NOSTR_*` environment variables on startup, prints its resolved configuration with the Bitcoin RPC password redacted, creates the SQLite parent directory if absent, rejects pre-fix SQLite databases missing height `0` via `NameTokenService::new()`, binds UDP DNS, and runs the supervised `BlockchainWatcher`. Once Bitcoin Core is reachable, `BlockchainWatcher` verifies height-0 genesis alignment against SQLite on each sync; if Bitcoin Core is temporarily unreachable or restarted, the watcher logs the transient RPC error to `stderr` and retries on subsequent ticks instead of panicking.

### CUJ 4: Full-Stack End-to-End Verification (`just test e2e`)

*   **Given**: Docker and `just` are installed, and no conflicting process is bound to `DNS_NOSTR_BIND_ADDR` (default `:1053`).
*   **When**: A contributor or coding agent runs `just test e2e`.
*   **Then**: `just test e2e` ensures Docker services are up and healthy (`just test up`), builds the `dns_nostr_server` and `dns_nostr_wallet` binaries using the host Cargo cache, and runs `cargo run -p end_to_end`. `end_to_end`'s `main()` spawns the freshly built `dns_nostr_server` against the shared SQLite database (`DNS_NOSTR_DB_PATH`, default `./data/name-tokens.sqlite`), polls DNS readiness until the server answers `NXDomain` for an unminted label under `DNS_NOSTR_ZONE_ORIGIN` (failing immediately with server stderr if the child exits early, e.g., on a port conflict), executes all five DNS scenarios (`not found`, `minted`, `updated`, `first-is-root`, `revoked`) and all five `dns_nostr_wallet` black-box CLI scenarios sequentially (failing fast on the first error), and terminates `dns_nostr_server` cleanly before exiting. Re-running `just test e2e` against the persistent Docker volumes and shared SQLite database passes without label collisions.

### CUJ 5: Local Wallet CLI Usage (`just wallet`)

*   **Given**: A user wants to manage a local wallet without typing `cargo run -p dns_nostr_wallet -- wallet ...`.
*   **When**: They run `just wallet create`, `just wallet import [WORDS...]` (or pipe words via stdin), or `just wallet address`.
*   **Then**: `just wallet` prepends the `wallet` subcommand to `dns_nostr_wallet` with safe positional-argument forwarding while preserving standard input and `DNS_NOSTR_NETWORK` / `DNS_NOSTR_WALLET_DIR` environment variables, producing identical output and exit codes to invoking the binary directly.

## Technical Plan (Architecture)

### 1. System Topology & Dependency Direction

Source-code dependencies point strictly inward (`end_to_end` → `dns_nostr_server` / `dns_nostr_token` → `name_token`; `dns_nostr_server` → `dns_nostr_token` → `name_token`; `dns_nostr_wallet` → `dns_nostr_token`). No test helpers or infrastructure dependencies leak into `name_token` or `dns_nostr_token`, and `dns_nostr_server/src/main.rs` remains a thin composition root.

```text
                           ┌──────────────────────────────────────┐
                           │             justfile                 │
                           │  wallet | serve | fund | test [cmd]  │
                           └───┬──────────┬──────────┬────────────┘
                               │          │          │
             ┌─────────────────┘          │          └─────────────────┐
             ▼                            ▼                            ▼
┌────────────────────────┐   ┌─────────────────────────┐   ┌─────────────────────────┐
│  docker-compose.yml    │   │    dns_nostr_server     │   │       end_to_end        │
│ • bitcoin-core         │   │  (host binary; spawned  │   │  (workspace binary      │
│   bitcoin:28.1-alpine  │◄──┤   by `end_to_end` or    │◄──┤   crate: `src/main.rs`, │
│   127.0.0.1:18443      │   │   manual `just serve`)  │   │   fail-fast runner)     │
│ • nostr-relay          │   │ • EnvironmentConfig     │   │ • reuses server config  │
│   nostr-rs-relay:0.8.8 │◄──┤ • BlockchainWatcher     │   │ • ManagedServer (RAII,  │
│   127.0.0.1:8080       │   │ • SqliteNameTokenRepo   │   │   shared SQLite DB)     │
│   (persistent volumes) │   │   (shared SQLite DB)    │   │ • server_scenarios      │
└────────────────────────┘   │ • NostrEventsRepository │   │   - Bitcoin P2PKH Tx    │
             ▲               │ • NostrAuthority (:1053)│   │   - Nostr publisher     │
             │               └─────────────────────────┘   │   - UDP DNS client      │
             │                                             │ • wallet_scenarios      │
             │           RPC (:18443) & WS (:8080)         │   - spawns wallet bin   │
             └─────────────────────────────────────────────┴─────────────────────────┘
```

### 2. Core Sync, Persistence & Lifecycle Fixes (`name_token` and `dns_nostr_server`)

#### 2.1 Fresh-DB `LocalChain` Initialization, Root-Cause Startup Validation & SQLite Persistence

In `bdk_chain`, `LocalChain::from_blocks` requires height `0` to be present in the checkpoint map. Today, `BlockchainWatcher` seeds `Emitter` at height `0` so the first emitted block arrives at height `1` connected to height `0`; `NameTokenService::apply_block_connected_to()` ignores height `1` for chain initialization, leaving `self.chain` as `None` forever, while `NameTokenService::new()` silently swallows `MissingGenesisError` on startup.

*   **`NameTokenService` (`name_token`):**
    *   Change `NameTokenService::new()` to return `Result<Self, NameTokenServiceError>`. If persisted checkpoints exist in the repository but lack height `0` (`MissingGenesisError`), return `Err(NameTokenServiceError::MissingGenesis)` instead of silently falling back to `None`. This fixes the root cause uniformly for every `NameTokenRepository` implementation without SQLite-specific workaround methods.
    *   In `apply_block_connected_to()`, when `self.chain` is `None` and the first block arrives at `height > 0` with `connected_to.height == 0`, persist the genesis checkpoint `(connected_to.hash, 0)` before saving `(block_hash, height)` and initialize `self.chain` with both height `0` and `height`. Keep `height == 0` initialization unchanged, and leave `self.chain` as `None` when `height > 0 && connected_to.height > 0` (preserving existing standalone unit tests).
    *   Expose `genesis_hash(&self) -> Option<BlockHash>` for sync-time genesis verification.
*   **`SqliteNameTokenRepository` (`dns_nostr_server`):**
    *   Replace the hardcoded-path `create()` constructor with `open(db_path) -> Result<Self, SqliteRepositoryError>` (creating missing parent directories) and `in_memory()` for unit tests.
    *   Enable SQLite Write-Ahead Logging (`WAL`), `synchronous = NORMAL`, and a `busy_timeout` on open so 101-block initial regtest catch-up does not stall behind per-block `fsync` commits or block concurrent DNS reads.
    *   Enforce `height INTEGER PRIMARY KEY` on `chain_checkpoints` with upsert semantics on conflict, and add lookup indexes on `name_tokens(label_hex)` and `name_tokens(last_txid, last_vout)`. Do not embed permanent startup deduplication migrations or redundant unique indexes for pre-fix prototype databases; rely on `NameTokenService::new()`'s `MissingGenesis` startup failure and the one-time reset documented in Rollout & Rollback.

#### 2.2 Supervised `BlockchainWatcher` & Height-0 Genesis Guard

*   `BlockchainWatcher` accepts `Arc<NameTokenService>` and `BlockchainWatcherConfig` (RPC URL, credentials, sync interval; redacted `Debug`). It does not take SQLite paths or a separate shutdown channel.
*   Replace all `.expect()` and `.unwrap()` calls in RPC client creation and block sync with typed error handling. Transient RPC errors (Bitcoin Core not yet started, connection refused, container restart) are logged to `stderr` and retried after `sync_interval`.
*   On each sync tick, after fetching Bitcoin Core's block `0` hash, compare it against `name_token_service.genesis_hash()`. If they differ (for example, pointing a regtest SQLite DB at testnet or mainnet), the watcher returns `Err(WatcherFatalError::GenesisMismatch { stored, node })` and exits.
*   `BlockchainWatcher::spawn()` returns `JoinHandle<Result<(), WatcherFatalError>>`. `main.rs` uses `tokio::select!` across `server.block_until_done()` and the watcher `JoinHandle`, exiting non-zero on both a typed `GenesisMismatch` (printing both hashes and `rm <db_path>*` to `stderr`) and an unexpected watcher task panic (`JoinError`), closing the UDP socket cleanly.
*   Because Bitcoin Core regtest uses a constant genesis hash, wiping the regtest Docker volume (`docker compose down -v`) does not trip the height-0 guard; once new blocks are mined, `Emitter` + `NameTokenService` unwind old blocks and index the new chain. (See `## Risks & Mitigations` for the height-0 window before block `1` is mined.)

#### 2.3 `NostrEventsRepository` Lifecycle & Authority Diagnostics

*   In `NostrEventsRepository::get_last_text_note_from_pubkey`:
    *   Handle `add_relay` errors by logging to `stderr`, shutting down the client, and returning `None` (no `.unwrap()`).
    *   Tighten the Nostr `fetch_events` timeout to `2` seconds (below the E2E client's `3`-second UDP receive timeout).
    *   Execute `nostr_client.shutdown().await` unconditionally before returning—covering `fetch_events` errors, empty result sets, and successful lookups—eliminating per-query WebSocket and task leaks while staying resilient across `nostr-relay` container restarts.
*   In `NostrAuthority::get_zone_file`:
    *   Log concrete lookup failure causes to `stderr` (invalid DNS-Nostr name/label, absent or revoked token, missing Nostr text note) alongside existing zone-file parse errors, while keeping DNS `ResponseCode::NXDomain` behavior unchanged.

### 3. `dns_nostr_server` Environment Configuration

Create `dns_nostr_server/src/environment_config.rs` mirroring `dns_nostr_wallet/src/adapters/environment_config.rs`, and remove the non-standard `[lib] name = "lib"` override in `dns_nostr_server/Cargo.toml` so `end_to_end` imports and reuses `dns_nostr_server::environment_config::EnvironmentConfig` directly as the single source of truth.

*   Constructors: `EnvironmentConfig::from_environment()` and `EnvironmentConfig::from_lookup(impl Fn(&str) -> Option<String>)` for pure, env-free unit testing.
*   Explicit `ConfigError` enum implementing `Display` and `std::error::Error`.
*   Custom `fmt::Debug` implementation redacting `bitcoin_rpc_password` as `"[REDACTED]"`. `main.rs` prints the resolved configuration on startup and exits non-zero to `stderr` on invalid variables.

| Environment Variable | Field | Default | Validation |
| --- | --- | --- | --- |
| `DNS_NOSTR_BIND_ADDR` | `bind_addr: SocketAddr` | `0.0.0.0:1053` | Valid `SocketAddr` |
| `DNS_NOSTR_ZONE_ORIGIN` | `zone_origin: LowerName` | `nostr.dns.name.` | Trim whitespace; reject empty or root `"."`; append trailing `.` if absent; valid `LowerName` with `is_fqdn() && num_labels() >= 1` |
| `DNS_NOSTR_DB_PATH` | `db_path: PathBuf` | `./data/name-tokens.sqlite` | Non-empty after trimming whitespace |
| `DNS_NOSTR_BITCOIN_RPC_URL` | `bitcoin_rpc_url: String` | `http://127.0.0.1:18443` | Starts with `http://` or `https://` with non-empty host |
| `DNS_NOSTR_BITCOIN_RPC_USER` | `bitcoin_rpc_user: String` | `rpcuser` | Non-empty after trimming whitespace |
| `DNS_NOSTR_BITCOIN_RPC_PASSWORD` | `bitcoin_rpc_password: String` | `rpcpassword` | Non-empty after trimming whitespace (redacted in logs/`Debug`) |
| `DNS_NOSTR_NOSTR_RELAY_URL` | `nostr_relay_url: String` | `ws://localhost:8080` | Starts with `ws://` or `wss://` with non-empty host |
| `DNS_NOSTR_SYNC_INTERVAL_MS` | `sync_interval: Duration` | `1000` (1 s) | Integer `u64 >= 1` |

`end_to_end/src/config.rs` wraps `EnvironmentConfig` (consuming all 8 `DNS_NOSTR_*` settings, including `db_path` for the shared SQLite DB), maps wildcard bind hosts (`0.0.0.0` / `::`) to loopback (`127.0.0.1` / `::1`) for UDP client queries, derives polling timeouts, and locates the compiled `dns_nostr_server` and `dns_nostr_wallet` binaries.

### 4. On-Demand Regtest & Docker Compose

*   Delete `regtest_node/` (`Dockerfile`, `bitcoin.conf`, `entry_point.sh`, `README.md`).
*   Update `docker-compose.yml`:
    *   `bitcoin-core` uses `image: bitcoin/bitcoin:28.1-alpine`, loopback-bound ports `127.0.0.1:18443:18443` and `127.0.0.1:18444:18444`, volume `bitcoin-core:/home/bitcoin/.bitcoin` (matching the Alpine image's `bitcoin` user, UID `100` / GID `101`), and explicit regtest flags including `-fallbackfee=0.0002` and `-acceptnonstdtxn=1` (required because `<inscription> <P2PKH>` locking scripts are non-standard under Bitcoin Core mempool policy).
    *   `bitcoin-core` healthcheck runs `bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword getblockchaininfo` (`interval: 1s`, `timeout: 3s`, `retries: 30`).
    *   `nostr-relay` keeps `image: scsibug/nostr-rs-relay:0.8.8`, loopback-bound port `127.0.0.1:8080:8080`, volume `nostr-relay:/usr/src/app/db`, and adds a TCP readiness healthcheck using `/usr/bin/bash` (verified present in `scsibug/nostr-rs-relay:0.8.8` with `/dev/tcp` support): `["CMD", "/usr/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080"]` (`interval: 1s`, `timeout: 3s`, `retries: 30`), avoiding any custom Dockerfile.
    *   For contributors upgrading from pre-existing `regtest_node/` Docker volumes and pre-fix SQLite databases, see the one-time migration step in `## Non-Functional Requirements` (Rollout & Rollback).

### 5. Root `justfile` Orchestration

Add a root `justfile` exposing four top-level recipes (using `[positional-arguments]` on `wallet` and `fund` for safe argument quoting and shell-injection protection):

*   `wallet *args`: executes `cargo run -q -p dns_nostr_wallet -- wallet "$@"` while preserving stdin piping (for `just wallet import`) and `DNS_NOSTR_*` environment variables.
*   `serve *args`: executes `cargo run -p dns_nostr_server -- "$@"` (honoring all `DNS_NOSTR_*` environment variables).
*   `fund btc address`: POSIX-shell recipe (no `jq` or `bc` prerequisite) targeting `docker compose exec -T bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword`:
    1.  Ensures `"miner"` is loaded (inspects `listwallets`; if absent, checks `listwalletdir` and runs `loadwallet "miner"` when present on disk or `createwallet "miner"` when absent).
    2.  Derives a miner address unconditionally via `-rpcwallet=miner getnewaddress`.
    3.  If `getblockcount` is below `101`, mines `101 - height` maturity blocks to the miner address so coinbase outputs become spendable.
    4.  Delegates amount, address, and balance validation natively to `-rpcwallet=miner sendtoaddress "$2" "$1"`.
    5.  Mines 1 confirmation block to the miner address and prints the txid and confirmed height.
*   `test *args`: dispatches subcommands:
    *   `just test` (no args) → `cargo test --workspace` (offline unit tests).
    *   `just test up` → `docker compose up -d --wait`.
    *   `just test down` → `docker compose down` (preserves Docker volumes).
    *   `just test e2e` → runs `just test up`, builds `dns_nostr_server` and `dns_nostr_wallet`, and runs `exec cargo run -p end_to_end`.
    *   Any other argument prints usage to `stderr` and exits non-zero.

### 6. `end_to_end` Workspace Crate Design

Remove the empty `end_to_end_tests/` directory and add `end_to_end` to `[workspace] members` in root `Cargo.toml`.

#### 6.1 Binary Crate Layout & Lockfile Reuse

*   `end_to_end` is a standard binary crate (`src/main.rs` with private modules; no `src/lib.rs`, no `[[test]]` integration targets, and no `e2e` Cargo feature flag).
*   `cargo test --workspace` compiles `end_to_end` (including all scenario and harness modules) and runs its co-located offline `#[cfg(test)]` unit tests with zero infrastructure and without invoking `main()`. `cargo build --workspace` compiles the entire E2E binary offline without needing `--all-features`.
*   Dependencies reuse crates already locked in `Cargo.lock` (including `dns_nostr_server`, `dns_nostr_token`, `name_token`, `bitcoin`, `bitcoincore-rpc`, `hickory-proto`, `nostr-sdk`, `rand`, `tokio`, and dev-dependency `bdk_chain`).

#### 6.2 Shared E2E Harness Modules (`end_to_end/src/`)

*   `config.rs`: wraps `dns_nostr_server::environment_config::EnvironmentConfig` (including `db_path`, with redacted `Debug`), resolves wildcard bind hosts to loopback for UDP client queries, computes polling bounds from `sync_interval`, and resolves compiled workspace binaries (`dns_nostr_server` and `dns_nostr_wallet`) from the active target profile directory (handling both binary executables and `deps/` test executables).
*   `server_process.rs`: `ManagedServer` RAII guard that spawns the built `dns_nostr_server` binary inheriting all `DNS_NOSTR_*` environment variables (so it uses the shared `DNS_NOSTR_DB_PATH` SQLite database and configured `DNS_NOSTR_BIND_ADDR`), drains stdout/stderr in background threads into a bounded buffer (preventing pipe deadlocks while preserving full diagnostics), polls `DnsClient::preflight_check` until ready (up to `10` s) while calling `child.try_wait()` on every iteration to fail immediately if the server exits early (e.g., port conflict with `just serve` or `MissingGenesis`), and kills + reaps the child unconditionally in `Drop`.
*   `labels.rs`: generates unique letter-prefixed lowercase labels (`e2e-<prefix>-<unix_ts>-<rand_hex>`) validated via `dns_nostr_token::label::Label::new`.
*   `dns_client.rs`: async UDP DNS client returning `ResponseCode` and `TXT` answers over IPv4 or IPv6 loopback, plus `preflight_check` querying a random unminted label under `<zone_origin>` and asserting `ResponseCode::NXDomain` (used by `ManagedServer` to verify DNS readiness and matching `DNS_NOSTR_ZONE_ORIGIN`).
*   `nostr_publisher.rs`: publishes a signed Nostr `Kind::TextNote` containing an apex `TXT` zone file, asserts relay acceptance, and shuts down the client.
*   `bitcoin_token_tx.rs`: connects to Bitcoin Core regtest RPC (using a wallet-scoped `/wallet/miner` client for wallet calls), ensures `"miner"` is initialized with sufficient spendable funds under a bounded mining cap, and constructs, signs (real P2PKH over the full inscribed `scriptPubKey`), broadcasts, and mines:
    *   **Mint:** inscribes `OP_FALSE OP_IF "name" <label> OP_NOP "dns-nostr" <pubkey32> OP_ENDIF <P2PKH>` into output `0` of a newly funded ephemeral keypair.
    *   **Update:** spends the active Token UTXO at input `0` into output `0` (satisfying Name-Token **same-index continuity `0 -> 0`**, where the Nth input must map to the Nth output to update rather than revoke) carrying a new Nostr public key.
    *   **Revoke:** spends the active Token UTXO at input `0` into a plain uninscribed P2PKH output, appending `NameTokenEvent::Revoked`.

#### 6.3 Fail-Fast Runner & E2E Scenario Modules (`end_to_end/src/`)

*   **`main.rs`** (`#[tokio::main] async fn main() -> ExitCode`): thin linear composition root with suite-level setup and RAII teardown (no `PREFLIGHT` `OnceLock`, no `--test-threads=1`):
    1.  Loads `E2eConfig::from_environment()` once.
    2.  Starts `ManagedServer::start(&config).await` once and holds the RAII guard for the duration of the server scenarios.
    3.  Initializes `DnsClient` and `RegtestTokenClient` once.
    4.  Runs all five `server_scenarios` and all five `wallet_scenarios` sequentially, printing `[PASS] <scenario>` as each succeeds. On the first `Err(message)` (fail-fast), prints `[FAIL] <scenario>: <message>` plus any captured `dns_nostr_server` stderr logs, drops `ManagedServer` cleanly, and returns `ExitCode::FAILURE`. Returns `ExitCode::SUCCESS` after all 10 scenarios pass.
*   **`server_scenarios.rs`** (five sequential `async fn(...) -> Result<(), String>` scenarios returning full label/txid/height/pubkey/DNS diagnostics on error):
    1.  `not_found_returns_nxdomain`: queries an unminted unique label → asserts `ResponseCode::NXDomain`.
    2.  `minted_resolves_txt_pubkey`: publishes a zone file with `pubkey_a`, mints `<label>` → `pubkey_a`, mines 1 block, polls DNS → asserts `ResponseCode::NoError` and TXT equals `pubkey_a`.
    3.  `updated_rotates_to_new_nostr_pubkey`: mints `<label>` → `pubkey_a` and confirms DNS; publishes a zone file with `pubkey_b`, broadcasts and mines an Update tx (`0 -> 0`) pointing to `pubkey_b`, polls DNS → asserts `ResponseCode::NoError` and TXT transitions to `pubkey_b`.
    4.  `first_is_root_ignores_competing_mint`: under the **First-is-Root Uniqueness (First Confirmed Rule)**, publishes zone files for `pubkey_a` and `pubkey_b`, mints Token #1 (`pubkey_a`) and confirms DNS returns `pubkey_a`; mints competing Token #2 (`pubkey_b`) for the same `<label>`, then mints an unrelated sequential canary label in the next block and polls DNS until the canary resolves (proving `BlockchainWatcher` has indexed past Token #2's block without a blind sleep); queries `<label>` → asserts `ResponseCode::NoError` and TXT remains `pubkey_a`.
    5.  `revoked_returns_nxdomain`: mints `<label>` → `pubkey_a` and confirms DNS; broadcasts and mines a Revoke tx, polls DNS → asserts `ResponseCode::NXDomain`.
*   **`wallet_scenarios.rs`** (five black-box `fn() -> Result<(), String>` scenarios executing the compiled `dns_nostr_wallet` binary in auto-cleaned temporary `DNS_NOSTR_WALLET_DIR` directories):
    1.  `create_generates_seed_sets_0600_and_rejects_duplicate`: succeeds, prints a 12-word BIP39 seed phrase, sets `0600` permissions on `wallet.sqlite` on Unix, and rejects a second `create` without altering the DB.
    2.  `import_args_and_address_auto_advance`: imports a deterministic BIP39 test mnemonic, confirms BIP 84 regtest addresses advance `0 -> 1 -> 2` across separate process runs, and verifies seed words never appear in stdout/stderr.
    3.  `import_via_stdin_and_address`: pipes the mnemonic over stdin and confirms matching `address` output.
    4.  `network_env_overrides_and_invalid_network`: verifies `DNS_NOSTR_NETWORK=testnet`, `mainnet`, and invalid network error handling.
    5.  `error_paths_do_not_leak_seed_words`: verifies `address` with no wallet, invalid seed phrase (without echoing words), and unknown CLI flags exit non-zero.

## Alternatives Considered

### Alternative 1: Containerize `dns_nostr_server` in `docker-compose.yml` or Require an External `just serve`

*   **Pros**: A Docker Compose service brings up Bitcoin, Nostr, and DNS together via `docker compose up`, while requiring an external `just serve` avoids process management inside `end_to_end`.
*   **Cons**: Compiling Rust (`rusqlite` bundled C SQLite, `bitcoin`, `nostr-sdk`, `hickory-server`) inside Docker is slow (or requires maintaining a separate multi-GB BuildKit cache), risks testing stale images without `--build`, and conflicts on UDP `:1053` when running `just test up` alongside local `just serve`. Conversely, requiring a manual background `just serve` forces developers and agents to start and restart the server after every code edit.
*   **Reason for Rejection**: Developer-loop speed and ergonomics trade-off—spawning the freshly host-built `dns_nostr_server` binary directly from `end_to_end` against the shared SQLite DB gives fast incremental Cargo builds, always tests current code, avoids Dockerfile/cache overhead, and makes `just test e2e` a single self-contained command.

### Alternative 2: Wipe Docker Volumes on `just test down` (`docker compose down -v`)

*   **Pros**: Guarantees a clean regtest chain and empty Nostr relay on every stop/start cycle and avoids regtest 150-block subsidy halving accumulation.
*   **Cons**: Forces re-mining 101+ coinbase maturity blocks after every restart, destroys local development state, and leaves any persistent SQLite database out of sync with the wiped chain.
*   **Reason for Rejection**: Operational trade-off—preserving Docker volumes keeps local restarts fast and stateful, while unique random timestamped labels prevent cross-run collisions.

### Alternative 3: Keep `regtest_node/` Custom Dockerfile and 10-Minute Background Mining Loop

*   **Pros**: Avoids migrating volume paths (`/root/.bitcoin` vs. `/home/bitcoin/.bitcoin`) and produces blocks automatically during idle periods.
*   **Cons**: Requires maintaining a custom image running as `root`, stalls on-demand testing behind a 10-minute timer, omits `-fallbackfee`, and fails when reloading an already-loaded `miner` wallet.
*   **Reason for Rejection**: Maintainability and test determinism trade-off—upstream `bitcoin/bitcoin:28.1-alpine` with explicit regtest CLI flags and on-demand mining removes custom image maintenance and eliminates timer stalls.

### Alternative 4: Separate `expent` (Informal Shorthand for Spent) and `deleted` DNS Test Scenarios

*   **Pros**: Mirrors two informal descriptions of spending a Token UTXO without reinscription.
*   **Cons**: In [`docs/domain.md`](../domain.md), both cases execute the identical `NameTokenEvent::Revoked` transition, duplicating test coverage while leaving token updates untested.
*   **Reason for Rejection**: Protocol coverage trade-off—collapsing duplicate revocation cases into a single `revoked` test frees test budget for an `updated` scenario covering on-chain Nostr pubkey rotation.

### Alternative 5: Assert Second-Token Promotion After Root Revocation in `first-is-root`

*   **Pros**: Verifies fallback ownership when a root token is revoked while a competing mint for the same label exists.
*   **Cons**: Expands `first-is-root` beyond verifying the core First-is-Root Uniqueness conflict outcome, adding extra funding/revocation transactions and polling cycles.
*   **Reason for Rejection**: Scope trade-off—`first-is-root` focuses strictly on proving the first confirmed mint beats a competing mint; promotion edge cases remain covered by unit tests in `name_token`.

### Alternative 6: Include Bitcoin Chain Reorganization (`invalidateblock`) Tests in `end_to_end`

*   **Pros**: Exercises live Bitcoin Core reorg unwinding end-to-end against `BlockchainWatcher` and SQLite.
*   **Cons**: Mutates global regtest chain state across persistent volumes, increases runtime and flakiness risk, and duplicates deterministic reorg tests already in `name_token`.
*   **Reason for Rejection**: Determinism and scope trade-off—reorg logic stays verified deterministically in `name_token` unit tests without destabilizing shared persistent regtest state.

### Alternative 7: Query `CNAME` at the Zone Apex `<label>.nostr.dns.name.`

*   **Pros**: Uses a familiar DNS alias record type for quick inspection.
*   **Cons**: RFC 1034 forbids a `CNAME` record from coexisting with other records (such as `SOA` or `NS`) at a zone apex, causing standard DNS parsers and authorities to reject or misbehave on apex `CNAME` zones—even when minimal test zones omit an explicit `SOA` record.
*   **Reason for Rejection**: Protocol compliance trade-off—an apex `TXT` record holding the token's Nostr public key is valid at the zone apex under RFC 1034 and directly verifies the active pubkey.

### Alternative 8: Name the Funding Helper `just test transfer` or Use `<Address> <BTC>` Order

*   **Pros**: Groups funding under `just test` or mirrors `sendtoaddress <address> <amount>`.
*   **Cons**: Funding is used for manual wallet development (not only tests), and `just fund <BTC> <Address>` reads naturally as "fund `<BTC>` to `<Address>`".
*   **Reason for Rejection**: Ergonomics trade-off—exposing top-level `just fund <BTC> <Address>` avoids nesting a general dev helper under `test`.

### Alternative 9: Forward Raw Arguments in `just wallet`

*   **Pros**: Transparent pass-through if non-`wallet` top-level subcommands are added later.
*   **Cons**: Forces redundant `just wallet wallet create` invocations for every currently implemented command.
*   **Reason for Rejection**: Ergonomics trade-off—prepending `wallet` matches all implemented CLI commands cleanly (`just wallet create | import | address`).

### Alternative 10: Name the E2E Crate `end-to-end` or Keep `end_to_end_tests/`

*   **Pros**: Reuses the existing empty folder name or hyphenated crate naming.
*   **Cons**: Breaks consistency with the workspace's `snake_case` crate names (`name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`) and keeps a redundant `_tests` suffix.
*   **Reason for Rejection**: Consistency trade-off—naming the crate `end_to_end` aligns with all existing workspace members.

### Alternative 11: Use an `OP_TRUE` (Anyone-Can-Spend) Locking Suffix for Inscribed UTXOs in `end_to_end`

*   **Pros**: Eliminates legacy P2PKH sighash signing code in `end_to_end`.
*   **Cons**: Diverges from the P2PKH Token UTXO script shape defined in [`docs/domain.md`](../domain.md) and skips real signature validation over inscribed `scriptPubKey`s on regtest.
*   **Reason for Rejection**: Fidelity trade-off—E2E transactions must exercise real `<inscription> <P2PKH>` locking scripts signed by an ephemeral keypair to match the domain specification.

### Alternative 12: Use a Blind `sleep(2.5 * sync_interval)` or Custom Same-Block Multi-Tx Canary in `first-is-root`

*   **Pros**: A blind sleep requires no extra transaction; a same-block canary avoids mining an extra block.
*   **Cons**: A blind sleep silently false-passes if `just serve` lags or uses a different `DNS_NOSTR_SYNC_INTERVAL_MS`; packing two transactions into one regtest block requires custom raw-mempool orchestration.
*   **Reason for Rejection**: Correctness-vs-simplicity trade-off—mining an unrelated sequential canary label in the next block after competing Mint #2 reuses existing mint/poll helpers in ~3 lines and deterministically proves `BlockchainWatcher` indexed past Mint #2's block.

### Alternative 13: Place Wallet Black-Box CLI Tests in `dns_nostr_wallet/tests/`

*   **Pros**: Direct access to Cargo's `CARGO_BIN_EXE_dns_nostr_wallet` without an explicit build step in `just test e2e`.
*   **Cons**: Splits end-to-end tests across two crates and runs binary-spawning process tests during fast unit-level `cargo test --workspace`.
*   **Reason for Rejection**: Separation-of-concerns trade-off—consolidating black-box E2E tests in `end_to_end` keeps `cargo test --workspace` strictly unit-level.

### Alternative 14: Default `DNS_NOSTR_SYNC_INTERVAL_MS` to `500` ms or Override `10000` ms Only Inside `justfile`

*   **Pros**: `500` ms trims wait latency slightly; keeping `10000` ms in `dns_nostr_server` preserves the original polling rate for bare `cargo run`.
*   **Cons**: `500` ms doubles idle RPC traffic with tighter CPU margins; overriding only in `justfile` creates a default mismatch whenever `dns_nostr_server` is launched via `cargo run` and tested via `just test e2e`.
*   **Reason for Rejection**: Consistency trade-off—a single `1000` ms default defined once in `EnvironmentConfig` and shared by `end_to_end` avoids configuration drift.

### Alternative 15: Fail-Fast Exit on Regtest Height-1/Tip Mismatch, or Log-Only Pause Without Exiting on Genesis Mismatch

*   **Pros**: Failing on height-1/tip divergence catches wiped regtest volumes immediately even at height `0`; pausing sync without exiting keeps the process alive if the original volume is reattached.
*   **Cons**: Exiting on height-1/tip divergence breaks legitimate deep regtest reorgs (`invalidateblock`); keeping UDP `:1053` bound on a wrong-network genesis mismatch silently serves incompatible records.
*   **Reason for Rejection**: Correctness trade-off—failing fast only on missing height `0` (via `NameTokenService::new`) or height-0 genesis mismatch (via `BlockchainWatcher`) prevents wrong-network serving while preserving legitimate reorg recovery.

### Alternative 16: Keep `end_to_end` as Feature-Gated `[[test]]` Targets (with `OnceLock` Preflight) or Use a Temp SQLite DB

*   **Pros**: Cargo's `libtest` provides built-in per-test filtering and continue-on-failure summaries, and a temp SQLite DB isolates each E2E run from `./data/name-tokens.sqlite`.
*   **Cons**: `libtest` lacks suite-level setup/teardown hooks, forcing a `[features] e2e` gate, `--all-features` checks, `--test-threads=1`, and a `static PREFLIGHT: OnceLock` workaround to manage shared server lifecycle; a temp SQLite DB skips verifying persistent SQLite reload and incremental sync across runs.
*   **Reason for Rejection**: Simplicity and realism trade-off—making `end_to_end` a fail-fast binary crate (`src/main.rs`) eliminates the feature gate, `OnceLock`, and test-thread flags while giving clean RAII setup/teardown around `ManagedServer`, and using the shared `DNS_NOSTR_DB_PATH` SQLite database exercises persistent state across repeated runs.

## Risks & Mitigations

| Risk / Limit | Impact | Mitigation |
| --- | --- | --- |
| **Port conflict with an already-running `just serve`** | If a developer already has `just serve` bound to `DNS_NOSTR_BIND_ADDR` (default `:1053`), `end_to_end`'s spawned `dns_nostr_server` fails to bind UDP `:1053`. | `ManagedServer::start` checks `child.try_wait()` during readiness polling and fails immediately with the server's `stderr` plus guidance to stop `just serve` or override `DNS_NOSTR_BIND_ADDR` (this also prevents two concurrent `dns_nostr_server` writers against the same shared SQLite database). |
| **Post-volume-wipe height-0 ghost-token window** | After `docker compose down -v && just test up`, regtest sits at height `0` with no block `1` yet, so `Emitter` emits nothing and `dns_nostr_server` serves previously indexed tokens until block `1` is mined. | Document deleting `./data/name-tokens.sqlite*` whenever manually wiping Docker volumes; once any new block is mined on regtest, `Emitter` + `NameTokenService` automatically unwinds the old chain. (Adding a separate rollback-without-block domain API is explicitly deferred.) |
| **Regtest 150-block subsidy halving exhaustion** | Persistent regtest volumes halve block subsidy every 150 blocks (reaching `0` after ~4,950 blocks), eventually preventing `"miner"` from minting new spendable coins. | Cap top-up mining in `RegtestTokenClient` at `500` blocks per run and return an explicit error instructing the developer to run `docker compose down -v` and remove `./data/name-tokens.sqlite*`. |
| **Per-query Nostr WebSocket overhead & concurrency limit** | `NostrEventsRepository` opens and shuts down a `nostr_sdk::Client` per DNS lookup; high-rate UDP floods could exhaust sockets or add per-query handshake latency. | Acceptable for local development and single-threaded E2E; unconditional `shutdown().await` eliminates leaks and a `2`-second fetch timeout bounds query hangs (below E2E's `3`-second UDP timeout). Revisit with a shared client pool only if production load demands it. |
| **Default regtest RPC credentials (`rpcuser`/`rpcpassword`)** | Exposing Bitcoin Core RPC or Nostr relay on `0.0.0.0` exposes dev services to LAN/Wi-Fi hosts. | Bind Docker Compose host ports strictly to `127.0.0.1` (`127.0.0.1:18443`, `127.0.0.1:18444`, `127.0.0.1:8080`), redact `bitcoin_rpc_password` in logs/`Debug`, and support env overrides. |
| **Persistent SQLite and Nostr relay growth across E2E runs** | Repeated `just test e2e` runs accumulate tokens and Nostr events across persistent volumes. | Add SQLite indexes on `name_tokens(label_hex)` and `name_tokens(last_txid, last_vout)` and enable WAL mode so lookups remain indexed and non-blocking; developers can reset state at any time via `docker compose down -v` and `rm ./data/name-tokens.sqlite*`. |

## Non-Functional Requirements

*   **Security & Privacy**:
    *   `EnvironmentConfig`, `BlockchainWatcherConfig`, and `E2eConfig` redact `bitcoin_rpc_password` as `"[REDACTED]"` in custom `fmt::Debug` implementations and startup logs.
    *   `end_to_end` verifies that `dns_nostr_wallet` sets `0600` file permissions on `wallet.sqlite` on Unix and never echoes seed words on `import` errors or `address` output.
    *   Docker Compose binds `bitcoin-core` and `nostr-relay` host ports to `127.0.0.1` only; regtest RPC credentials remain local defaults overridable via `DNS_NOSTR_BITCOIN_RPC_USER` and `DNS_NOSTR_BITCOIN_RPC_PASSWORD`.
*   **Performance & Latency**:
    *   SQLite WAL mode (`journal_mode = WAL`, `synchronous = NORMAL`, `busy_timeout = 5000`) eliminates per-block `fsync` stalls during 101-block initial regtest sync, and indexes on `name_tokens(label_hex)` and `name_tokens(last_txid, last_vout)` prevent linear scans as persistent E2E runs accumulate tokens.
    *   Unconditional `nostr_client.shutdown().await` on every DNS lookup prevents WebSocket and Tokio task leaks, and a `2`-second Nostr fetch timeout bounds relay stalls.
    *   Target: complete `just test e2e` in roughly 10–15 seconds on a warm regtest chain under the `1000` ms default sync cadence while keeping `cargo test --workspace` offline.
*   **Rollout & Rollback**:
    *   *One-time local migration:* Contributors upgrading from `regtest_node/` (`bitcoin-core:/root/.bitcoin`, UID `0`) and pre-fix SQLite databases (missing height `0` in `chain_checkpoints`) must run `docker compose down -v` and remove `./data/name-tokens.sqlite*` once. If a pre-fix SQLite database missing height `0` is left in place, `NameTokenService::new()` returns `MissingGenesis` and `dns_nostr_server` exits non-zero naming the exact file path to remove.
    *   *Rollback:* Reverting the commit restores the previous behavior and requires deleting `./data/name-tokens.sqlite*` (including WAL sidecars) if rolling back past the `chain_checkpoints` primary-key and WAL changes.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Work is partitioned into five incremental phases. Each phase leaves the repository buildable and passes its verification checkpoint before starting the next phase.

### Phase 1: Core Domain Genesis Persistence & Startup Validation (`name_token`)

#### 1. `name_token/src/name_token_service.rs` (Modify)

*   **Scope**: Domain Service + Unit Tests (`mod test_name_token_service`)
*   **Changes**:
    *   Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum NameTokenServiceError { MissingGenesis }` implementing `Display` and `std::error::Error`.
    *   Change `NameTokenService::new(repository: Box<dyn NameTokenRepository>) -> Result<Self, NameTokenServiceError>`: when `repository.get_chain().await` is non-empty and `LocalChain::from_blocks(tree)` returns `Err(_)`, return `Err(NameTokenServiceError::MissingGenesis)` instead of silently falling back to `None`.
    *   In `apply_block_connected_to(&self, block: &Block, height: u32, connected_to: BlockId)`: when `self.chain` is `None` and `height > 0 && connected_to.height == 0`, persist `(connected_to.hash, 0, &[])` via `self.repository.save_block_updates` before persisting `(block_hash, height, &updates)`, then initialize `self.chain` with `LocalChain::from_blocks` containing height `0` and `height`. Keep `height == 0` (`LocalChain::from_genesis_hash`) and `height > 0 && connected_to.height > 0` (`None`) unchanged.
    *   Add `pub fn genesis_hash(&self) -> Option<BlockHash>` returning height-`0` checkpoint hash if `self.chain` is `Some`.
    *   In `mod test_name_token_service`: update `new_service()` to `.unwrap()`, add a private `struct SharedRepository(Arc<InMemoryNameTokenRepository>)` implementing `NameTokenRepository` (so `in_memory_name_token_repository.rs` remains untouched), and add unit tests covering:
        *   Applying block `1` connected to height `0` sets `tip()` to height `1`, sets `genesis_hash()` to `connected_to.hash`, and persists both height `0` and height `1`.
        *   Constructing a second `NameTokenService` over the same `SharedRepository` reloads `tip()` at height `1` and `genesis_hash()` at height `0`.
        *   Constructing `NameTokenService` over a repository containing checkpoints at height `> 0` without height `0` returns `Err(NameTokenServiceError::MissingGenesis)`.
        *   Mint → update → revoke across heights `1`, `2`, `3` from an empty service preserves final revoked state without rewinding.

#### 2. `name_token/src/lib.rs` (Modify)

*   **Scope**: Crate Public Exports
*   **Changes**: Re-export `NameTokenServiceError` alongside `NameTokenService`.

#### 3. `name_token/src/README.md` (Modify)

*   **Scope**: Source EGM Contract Documentation
*   **Changes**: Update `Interface` for `NameTokenService::new(...) -> Result<Self, NameTokenServiceError>`, `NameTokenServiceError`, and `genesis_hash() -> Option<BlockHash>`; update `Invariants` for height-0 genesis persistence and `MissingGenesis` startup rejection.

*   **Phase 1 Checkpoint**: `cargo test -p name_token` passes (note: `dns_nostr_server` updates for `Result` follow immediately in Phase 2).

---

### Phase 2: Server Environment Config, WAL Persistence, Resilient Watcher & Lifecycle Fixes (`dns_nostr_server`)

#### 4. `Cargo.toml` (Modify — Part 1: Tokio Feature)

*   **Scope**: Workspace Manifest
*   **Changes**: Add `"time"` to `[workspace.dependencies] tokio.features` alongside `"macros"`, `"rt-multi-thread"`, `"net"`, and `"fs"` (omit `"sync"` because `BlockchainWatcher` uses `JoinHandle` directly).

#### 5. `dns_nostr_server/Cargo.toml` (Modify)

*   **Scope**: Server Crate Manifest
*   **Changes**: Remove `name = "lib"` from `[lib]` so Cargo exports the library target under the standard crate name `dns_nostr_server` (enabling `end_to_end` to reuse `EnvironmentConfig`).

#### 6. `dns_nostr_server/src/environment_config.rs` (Create)

*   **Scope**: Server Configuration Adapter + Unit Tests (`mod test_environment_config`)
*   **Changes**:
    *   Public variable-name constants matching the Section 3 table (`BIND_ADDR_VARIABLE`, `ZONE_ORIGIN_VARIABLE`, `DB_PATH_VARIABLE`, `BITCOIN_RPC_URL_VARIABLE`, `BITCOIN_RPC_USER_VARIABLE`, `BITCOIN_RPC_PASSWORD_VARIABLE`, `NOSTR_RELAY_URL_VARIABLE`, `SYNC_INTERVAL_MS_VARIABLE`).
    *   `pub struct EnvironmentConfig` with fields `bind_addr: SocketAddr`, `zone_origin: LowerName`, `db_path: PathBuf`, `bitcoin_rpc_url: String`, `bitcoin_rpc_user: String`, `bitcoin_rpc_password: String`, `nostr_relay_url: String`, `sync_interval: Duration`. Derives `Clone, PartialEq, Eq` and implements custom `fmt::Debug` redacting `bitcoin_rpc_password` as `"[REDACTED]"`.
    *   `#[derive(Debug, Clone, PartialEq, Eq)] pub enum ConfigError` with explicit variants:
        *   `InvalidBindAddr { value: String }`
        *   `InvalidZoneOrigin { value: String }`
        *   `EmptyValue { variable: &'static str }`
        *   `InvalidUrl { variable: &'static str, value: String, expected_schemes: &'static str }`
        *   `InvalidSyncInterval { value: String }`
        Implementing `Display` and `std::error::Error`.
    *   `pub fn from_environment() -> Result<Self, ConfigError>` and `pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError>`:
        *   `DNS_NOSTR_ZONE_ORIGIN`: trim whitespace; reject empty or `"."`; append trailing `.` if absent; parse as `LowerName`; require `is_fqdn() && num_labels() >= 1`.
        *   `DNS_NOSTR_DB_PATH`, `DNS_NOSTR_BITCOIN_RPC_USER`, `DNS_NOSTR_BITCOIN_RPC_PASSWORD`: reject empty/whitespace-only strings.
        *   `DNS_NOSTR_BITCOIN_RPC_URL` / `DNS_NOSTR_NOSTR_RELAY_URL`: trim whitespace; verify scheme (`http://`/`https://` or `ws://`/`wss://`) AND non-empty host authority after `://`.
        *   `DNS_NOSTR_SYNC_INTERVAL_MS`: parse `u64 >= 1`.
    *   Co-located `mod test_environment_config` verifying all defaults, overriding every variable, zone-origin trailing-dot normalization and root `"."`/empty rejection, bare URL scheme rejection, whitespace-only rejection, zero/invalid sync interval rejection, and password redaction in `Debug`.

#### 7. `dns_nostr_server/src/lib.rs` (Modify)

*   **Scope**: Crate Root Module Index
*   **Changes**: Add `pub mod environment_config;`.

#### 8. `dns_nostr_server/src/sqlite_name_token_repository.rs` (Modify)

*   **Scope**: SQLite Persistence Adapter + Unit Tests (`mod test_sqlite_name_token_repository`)
*   **Changes**:
    *   Define `pub enum SqliteRepositoryError { CreateParentDir(std::io::Error), Sqlite(rusqlite::Error) }` implementing `Debug`, `Display`, and `std::error::Error`.
    *   Replace `pub async fn create() -> Self` with `pub fn open(db_path: impl AsRef<Path>) -> Result<Self, SqliteRepositoryError>` (calling `std::fs::create_dir_all(parent)` when `db_path.parent()` is non-empty).
    *   Add `pub fn in_memory() -> Result<Self, SqliteRepositoryError>` for unit tests.
    *   Configure WAL pragmas on open (`PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;`) for file-backed connections (`busy_timeout` on in-memory too).
    *   Update `create_tables(&self) -> Result<(), rusqlite::Error>` (propagating errors instead of panicking):
        *   Retain legacy `DROP TABLE IF EXISTS state` and `DROP TABLE IF EXISTS name_token_versions`.
        *   Define `CREATE TABLE IF NOT EXISTS chain_checkpoints (height INTEGER PRIMARY KEY, blockhash CHAR(64) NOT NULL)`.
        *   Keep `name_tokens` table definition and add `CREATE INDEX IF NOT EXISTS idx_name_tokens_label_hex ON name_tokens (label_hex)` and `CREATE INDEX IF NOT EXISTS idx_name_tokens_last_outpoint ON name_tokens (last_txid, last_vout)`.
    *   Update `save_block_updates` checkpoint insert to `INSERT INTO chain_checkpoints (height, blockhash) VALUES (?1, ?2) ON CONFLICT(height) DO UPDATE SET blockhash = excluded.blockhash`.
    *   Co-located `mod test_sqlite_name_token_repository` covering parent-directory auto-creation in a temp path, WAL mode enabled on file DB, idempotent upsert on `save_block_updates` at the same height, and verifying `idx_name_tokens_label_hex` and `idx_name_tokens_last_outpoint` exist in `sqlite_master`.

#### 9. `dns_nostr_server/src/blockchain_watcher.rs` (Modify)

*   **Scope**: Bitcoin Core Sync Adapter + Unit Tests (`mod test_blockchain_watcher`)
*   **Changes**:
    *   Define `pub struct BlockchainWatcherConfig { pub rpc_url: String, pub rpc_user: String, pub rpc_password: String, pub sync_interval: Duration }` deriving `Clone, PartialEq, Eq` with custom `fmt::Debug` redacting `rpc_password` as `"[REDACTED]"`.
    *   Define `#[derive(Debug, Clone, PartialEq, Eq)] pub enum WatcherFatalError { GenesisMismatch { stored: BlockHash, node: BlockHash } }` implementing `Display` and `std::error::Error`.
    *   Update `BlockchainWatcher::new(name_token_service: Arc<NameTokenService>, config: BlockchainWatcherConfig) -> Self` and `pub fn spawn(self) -> tokio::task::JoinHandle<Result<(), WatcherFatalError>>`.
    *   Return `Result<Client, bitcoincore_rpc::Error>` from `bitcoin_client(&self)` and `Result<(), SyncError>` (`TransientRpc(bitcoincore_rpc::Error)` vs `Fatal(WatcherFatalError)`) from `sync_blocks(&self)`:
        *   Fetch `node_genesis = client.get_block_hash(0)?`.
        *   If `let Some(stored) = self.name_token_service.genesis_hash()` and `stored != node_genesis`, return `Err(SyncError::Fatal(WatcherFatalError::GenesisMismatch { stored, node: node_genesis }))`.
        *   Propagate `emitter.next_block()?` errors instead of `.expect()`.
    *   In `watch_blockchain(self) -> Result<(), WatcherFatalError>`: log `TransientRpc` errors to `stderr` and sleep `sync_interval` before retrying; return `Err(fatal)` immediately on `SyncError::Fatal(fatal)`.
    *   Co-located `mod test_blockchain_watcher` verifying the genesis-mismatch check helper flags mismatched hashes and accepts matching hashes or `None`, and verifying `BlockchainWatcherConfig`'s `Debug` redacts `rpc_password`.

#### 10. `dns_nostr_server/src/nostr_events_repository.rs` (Modify)

*   **Scope**: Nostr Relay Adapter + Unit Tests (`mod test_nostr_events_repository`)
*   **Changes**:
    *   In `get_last_text_note_from_pubkey`: handle `add_relay` errors by logging to `stderr`, calling `nostr_client.shutdown().await`, and returning `None`.
    *   Use `Duration::from_secs(2)` for `fetch_events` (below E2E's `3`-second UDP timeout), capture the `Result`, log `Err` to `stderr`, always call `nostr_client.shutdown().await` unconditionally before any `?`/return, and then return `events.first().cloned()`.
    *   Co-located `mod test_nostr_events_repository` confirming an invalid relay URL returns `None` without panicking.

#### 11. `dns_nostr_server/src/nostr_authority.rs` (Modify)

*   **Scope**: DNS Authority Adapter
*   **Changes**: Log to `stderr` inside `get_zone_file` when `extract_token_label`, `dns_nostr_token_repository.get_token`, or `nostr_events_repository.get_last_text_note_from_pubkey` returns `None`, keeping DNS `ResponseCode::NXDomain` behavior unchanged.

#### 12. `dns_nostr_server/src/main.rs` (Modify)

*   **Scope**: Binary Composition Root
*   **Changes**:
    *   Update `use lib::` imports to `use dns_nostr_server::`.
    *   Change signature to `#[tokio::main] async fn main() -> ExitCode`.
    *   Load `EnvironmentConfig::from_environment()` (on `Err`, log to `stderr` and return `ExitCode::FAILURE`), then print the redacted `Debug` config to `stdout`.
    *   Open `SqliteNameTokenRepository::open(&config.db_path)` (on `Err`, log to `stderr` and return `ExitCode::FAILURE`).
    *   Construct `NameTokenService::new(Box::new(repository)).await`; on `Err(NameTokenServiceError::MissingGenesis)`, print recovery guidance to `stderr` naming `config.db_path` (`rm <db_path>*`) and return `ExitCode::FAILURE`.
    *   Spawn `let watcher_handle = BlockchainWatcher::new(name_token_service.clone(), watcher_config).spawn();`.
    *   Bind `UdpSocket::bind(config.bind_addr).await` (on `Err`, log to `stderr` and return `ExitCode::FAILURE`; no `.unwrap()`).
    *   Run `tokio::select!` across `server.block_until_done()` and `watcher_handle`:
        *   `Ok(Err(WatcherFatalError::GenesisMismatch { stored, node }))` → log both hashes and `rm <db_path>*` to `stderr`, return `ExitCode::FAILURE`.
        *   `Ok(Ok(()))` or `Err(join_err)` (unexpected watcher exit/panic) → log unexpected watcher termination to `stderr`, return `ExitCode::FAILURE`.
        *   `server.block_until_done()` → return `ExitCode::SUCCESS` on `Ok(())`, or log to `stderr` and return `ExitCode::FAILURE` on `Err`.

#### 13. `dns_nostr_server/README.md` & `dns_nostr_server/src/README.md` (Modify)

*   **Scope**: Crate & Source EGM Contract Documentation
*   **Changes**: Document `DNS_NOSTR_*` env vars, `environment_config.rs`, WAL mode, `chain_checkpoints` primary key, `name_tokens` indexes, `NameTokenService::new` `MissingGenesis` startup guard, supervised `BlockchainWatcher` `JoinHandle` fatal propagation, unconditional Nostr client shutdown (`2` s timeout), and `just serve` / `just test e2e` verification.

*   **Phase 2 Checkpoint**: `cargo test --workspace` passes with zero errors and zero warnings.

---

### Phase 3: On-Demand Regtest Infrastructure & `justfile` Orchestration

#### 14. `docker-compose.yml` (Modify)

*   **Scope**: Local Infrastructure Configuration
*   **Changes**:
    *   Replace `bitcoin-core`'s `build` block with `image: bitcoin/bitcoin:28.1-alpine`, `command`: `-regtest=1 -server=1 -rpcuser=rpcuser -rpcpassword=rpcpassword -rpcbind=0.0.0.0 -rpcallowip=0.0.0.0/0 -fallbackfee=0.0002 -acceptnonstdtxn=1`.
    *   Mount `bitcoin-core:/home/bitcoin/.bitcoin` and bind host ports to loopback (`127.0.0.1:18443:18443`, `127.0.0.1:18444:18444`).
    *   Add `bitcoin-core` `healthcheck`: `test: ["CMD", "bitcoin-cli", "-regtest", "-rpcuser=rpcuser", "-rpcpassword=rpcpassword", "getblockchaininfo"]`, `interval: 1s`, `timeout: 3s`, `retries: 30`.
    *   Keep `nostr-relay` (`image: scsibug/nostr-rs-relay:0.8.8`, volume `nostr-relay:/usr/src/app/db`), bind port to `127.0.0.1:8080:8080`, and add verified `healthcheck`: `test: ["CMD", "/usr/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080"]`, `interval: 1s`, `timeout: 3s`, `retries: 30`.

#### 15. `regtest_node/Dockerfile`, `regtest_node/bitcoin.conf`, `regtest_node/entry_point.sh`, `regtest_node/README.md` (Delete)

*   **Scope**: Legacy Custom Regtest Image & Directory Contract
*   **Changes**: Delete the entire `regtest_node/` directory.

#### 16. `justfile` (Create)

*   **Scope**: Developer & Agent Command Orchestration
*   **Changes**:
    *   `[positional-arguments] wallet *args`: `cargo run -q -p dns_nostr_wallet -- wallet "$@"` (preserves stdin piping and `DNS_NOSTR_*` env vars).
    *   `[positional-arguments] serve *args`: `cargo run -p dns_nostr_server -- "$@"`.
    *   `[positional-arguments] fund btc address`: POSIX shell recipe (using `grep` / `awk` only; no `jq`/`bc` dependency) invoking `docker compose exec -T bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword`:
        1.  Verify `"miner"` wallet via `listwallets`; if not loaded, check `listwalletdir` and run `loadwallet "miner"` when present or `createwallet "miner"` when absent.
        2.  Derive `<miner_addr>` unconditionally via `-rpcwallet=miner getnewaddress`.
        3.  Read `height` via `getblockcount`; if `height < 101`, mine `101 - height` maturity blocks via `-rpcwallet=miner generatetoaddress $((101 - height)) "$miner_addr"`.
        4.  Run `-rpcwallet=miner sendtoaddress "$2" "$1"` (delegating amount, address, and spendable-balance validation natively to Bitcoin Core).
        5.  Mine 1 confirmation block via `-rpcwallet=miner generatetoaddress 1 "$miner_addr"` and print the txid and resulting height.
    *   `[positional-arguments] test *args`:
        *   `0` args (`just test`) → `cargo test --workspace`
        *   `up` (`just test up`) → `docker compose up -d --wait`
        *   `down` (`just test down`) → `docker compose down` (preserves Docker volumes)
        *   `e2e` (`just test e2e`) → `just test up && cargo build -p dns_nostr_server -p dns_nostr_wallet && exec cargo run -p end_to_end`
        *   anything else → print `Usage: just test [up|down|e2e]` to `stderr` and exit `1`.

*   **Phase 3 Checkpoint**: `just test up && just fund 0.5 <regtest_addr> && just test down` succeeds.

---

### Phase 4: `end_to_end` Workspace Binary Crate & E2E Scenario Suites

#### 17. `Cargo.toml` (Modify — Part 2: Workspace Member) & `Cargo.lock` (Modify)

*   **Scope**: Workspace Manifest & Lockfile
*   **Changes**: Add `"./end_to_end"` to `[workspace] members` in `Cargo.toml` and update `Cargo.lock` with the new `end_to_end` workspace package entry (no new external crate downloads; all dependencies reuse existing lockfile versions).

#### 18. `end_to_end_tests/` (Delete)

*   **Scope**: Legacy Empty Directory
*   **Changes**: Delete empty `end_to_end_tests/src/` and `end_to_end_tests/tests/`.

#### 19. `end_to_end/Cargo.toml` (Create)

*   **Scope**: E2E Binary Crate Manifest
*   **Changes**:
    *   `[package]` `name = "end_to_end"`, `version = "0.0.0"`, `edition = "2021"`.
    *   No `[features]` and no `[[test]]` entries (standard binary target `src/main.rs`).
    *   `[dependencies]`: `bitcoin.workspace = true`, `bitcoincore-rpc = "0.19.0"`, `dns_nostr_server = { path = "../dns_nostr_server" }`, `dns_nostr_token = { path = "../dns_nostr_token" }`, `hickory-proto = "0.24.2"`, `name_token = { path = "../name_token" }`, `nostr-sdk = "0.41.0"`, `rand = "0.8.5"`, `tokio.workspace = true`.
    *   `[dev-dependencies]`: `bdk_chain = "0.23.3"` (required to construct `Position::new(BlockId { .. }, ..)` when testing `DnsNostrToken::try_from(NameToken)`).

#### 20. `end_to_end/src/main.rs` (Create)

*   **Scope**: Binary Composition Root & Fail-Fast E2E Runner
*   **Changes**:
    *   Declares private modules: `mod bitcoin_token_tx`, `config`, `dns_client`, `labels`, `nostr_publisher`, `server_process`, `server_scenarios`, `wallet_scenarios`.
    *   `#[tokio::main] async fn main() -> ExitCode`:
        1.  Loads `E2eConfig::from_environment()` once.
        2.  Starts `let server = ManagedServer::start(&config).await` (spawning `dns_nostr_server` against the shared `DNS_NOSTR_DB_PATH` SQLite database and waiting until DNS readiness passes).
        3.  Constructs `DnsClient` and `RegtestTokenClient` once.
        4.  Executes all 5 `server_scenarios` and all 5 `wallet_scenarios` sequentially, printing `[PASS] <name>` per passing scenario. On the first `Err(error)` (fail-fast), prints `[FAIL] <name>: <error>` along with any captured `dns_nostr_server` stderr logs, drops `server` cleanly, and returns `ExitCode::FAILURE`. Prints a final pass summary and returns `ExitCode::SUCCESS` when all 10 scenarios pass.

#### 21. `end_to_end/src/config.rs` (Create)

*   **Scope**: E2E Configuration Wrapper + Unit Tests (`mod test_config`)
*   **Changes**:
    *   Wrap `dns_nostr_server::environment_config::EnvironmentConfig` (consuming all 8 `DNS_NOSTR_*` variables and defaults, including `db_path`, without duplicating parsing logic).
    *   Expose `E2eConfig::from_environment() -> Result<Self, ConfigError>` and `E2eConfig::from_lookup(...) -> Result<Self, ConfigError>` with custom `fmt::Debug` redacting `bitcoin_rpc_password`.
    *   Map wildcard `bind_addr` (`0.0.0.0:<port>` → `127.0.0.1:<port>`, `[::]:<port>` → `[::1]:<port>`) into `dns_server_addr: SocketAddr`.
    *   Derived timing helpers: `poll_timeout()` (`max(Duration::from_secs(5), self.sync_interval * 5)`) and `poll_interval()` (`min(Duration::from_millis(250), self.sync_interval / 2)`).
    *   `binary_path(binary_name: &str) -> Result<PathBuf, String>` (plus convenience helpers `server_binary_path()` and `wallet_binary_path()`):
        1.  Inspect `std::env::current_exe()` to derive the active target profile directory (returning parent's parent when inside `deps/` during `cargo test`, or parent directly when running the `end_to_end` binary—automatically honoring `--release`, custom profiles, and `CARGO_TARGET_DIR`).
        2.  Fall back to `CARGO_TARGET_DIR/debug` (anchored to workspace root if relative) or `<workspace_root>/target/debug`.
        3.  Append `format!("{binary_name}{}", std::env::consts::EXE_SUFFIX)` and verify `path.is_file()` (returning an error advising `cargo build -p <binary_name>` or `just test e2e` if absent).
    *   Co-located `mod test_config` verifying defaults, `db_path`, wildcard IPv4/IPv6 loopback mapping, overrides, redacted `Debug`, and timing formulas.

#### 22. `end_to_end/src/server_process.rs` (Create)

*   **Scope**: Managed `dns_nostr_server` Child Process RAII Guard + Unit Tests (`mod test_server_process`)
*   **Changes**:
    *   `pub struct ManagedServer` holding `child: std::process::Child`, `stderr_log: Arc<Mutex<String>>`, and background reader handles (`JoinHandle<()>`).
    *   `ManagedServer::start(config: &E2eConfig) -> Result<Self, String>`:
        *   Probes `std::net::UdpSocket::bind(config.bind_addr)` before spawning to fail immediately if `DNS_NOSTR_BIND_ADDR` is already occupied (preventing a readiness probe race against an existing `just serve` instance).
        *   Locates `E2eConfig::server_binary_path()?`.
        *   Spawns `dns_nostr_server` inheriting all `DNS_NOSTR_*` environment variables (so it uses the shared `DNS_NOSTR_DB_PATH` SQLite DB and configured `DNS_NOSTR_BIND_ADDR`), discarding stdout (`Stdio::null()`) and draining stderr into a bounded buffer in `stderr_log`.
        *   Polls `DnsClient::new(config.dns_server_addr).preflight_check(&config.zone_origin.to_string()).await` every `100` ms up to `10` s, calling `child.try_wait()` before and after readiness checks so early child exits (e.g., `MissingGenesis` or invalid config) fail immediately with exit status and captured stderr.
    *   `pub fn captured_stderr(&self) -> String`: returns captured stderr for diagnostics on scenario failure.
    *   `impl Drop for ManagedServer`: unconditionally calls `let _ = self.child.kill(); let _ = self.child.wait();` so `dns_nostr_server` is always stopped and reaped on success, fail-fast error, or panic.
    *   Co-located `mod test_server_process` verifying bounded stderr buffer truncation behavior.

#### 23. `end_to_end/src/labels.rs` (Create)

*   **Scope**: Test Label Generator + Unit Tests (`mod test_labels`)
*   **Changes**:
    *   `pub fn unique_label(prefix: &str) -> dns_nostr_token::label::Label`: sanitizes `prefix` to lowercase ASCII replacing `_` with `-`, truncates `prefix` to at most `20` chars (stripping any trailing `-`) so `e2e-<prefix>-<unix_ts>-<8_hex_chars>` (generated from 4 random bytes via `rand`) stays well under `63` bytes, and validates via `Label::new`.
    *   Co-located `mod test_labels` verifying RFC-1034 validity, underscore sanitization, truncation within `<= 63` bytes, and uniqueness across consecutive calls.

#### 24. `end_to_end/src/dns_client.rs` (Create)

*   **Scope**: Async UDP DNS Client + Unit Tests (`mod test_dns_client`)
*   **Changes**:
    *   `pub struct DnsResponse { pub response_code: ResponseCode, pub txt_records: Vec<String> }` and typed `pub enum DnsClientError { InvalidName(String), Encode(String), Io(std::io::Error), Timeout(Duration), Decode(String), ResponseIdMismatch { expected: u16, actual: u16 } }`.
    *   `pub struct DnsClient { server_addr: SocketAddr }`:
        *   `query_txt(&self, fqdn: &str) -> Result<DnsResponse, DnsClientError>`: builds a `Message` query for `RecordType::TXT` with a random 16-bit ID, encodes via `BinEncodable::to_bytes()`, binds an ephemeral `UdpSocket` matching `self.server_addr`'s IP family (`"0.0.0.0:0"` for IPv4, `"[::]:0"` for IPv6), receives into a `4096`-byte buffer under a `3`-second `tokio::time::timeout`, decodes via `BinDecodable::from_bytes()`, validates matching `message.id()`, and extracts `ResponseCode` and `RData::TXT` strings.
        *   `preflight_check(&self, zone_origin: &str) -> Result<(), String>`: queries a random unminted label `<unique>.<zone_origin>` with a `1`-second timeout and asserts `ResponseCode::NXDomain`, returning a clear reachability or zone-origin mismatch error if not yet ready or misconfigured.
    *   Co-located `mod test_dns_client` testing wire-format query encoding, `TXT`/`NXDomain` decoding, and response-ID validation.

#### 25. `end_to_end/src/nostr_publisher.rs` (Create)

*   **Scope**: Nostr Zone-File Publisher Helper
*   **Changes**:
    *   `pub async fn publish_txt_zone_file(relay_url: &str, keys: &nostr_sdk::Keys, txt_value: &str) -> Result<EventId, String>`: formats `$TTL 60\n@ IN SOA ns1.nostr.dns.name. hostmaster.nostr.dns.name. ( 1 3600 600 86400 60 )\n@ IN TXT "<txt_value>"\n`, connects `nostr_sdk::Client::new(keys.clone())` to `relay_url`, sends via `client.send_event_builder(EventBuilder::text_note(zone_content)).await`, verifies `!output.success.is_empty()`, and unconditionally calls `client.shutdown().await` before returning.

#### 26. `end_to_end/src/bitcoin_token_tx.rs` (Create)

*   **Scope**: Regtest RPC & P2PKH-Signed Token UTXO Builder + Unit Tests (`mod test_bitcoin_token_tx`)
*   **Changes**:
    *   `pub struct TokenUtxo { pub outpoint: OutPoint, pub amount: Amount, pub script_pubkey: ScriptBuf, pub secret_key: SecretKey, pub block_height: u64 }`.
    *   `pub fn build_token_script_pubkey(label: &Label, nostr_pubkey: &nostr_sdk::PublicKey, owner_pubkey_hash: &PubkeyHash) -> ScriptBuf`: builds `OP_FALSE OP_IF "name" <label_bytes> OP_NOP "dns-nostr" <pubkey32> OP_ENDIF <P2PKH(owner_pubkey_hash)>`.
    *   `pub struct RegtestTokenClient` holding a root RPC `Client` (`<rpc_url>`) for `list_wallets`/`load_wallet`/`create_wallet` and a wallet-scoped RPC `Client` (`<rpc_url_trimmed>/wallet/miner`) for `get_balance`, `get_new_address`, `send_to_address`, `get_raw_transaction`, `send_raw_transaction`, and `generate_to_address`:
        *   `new(rpc_url, rpc_user, rpc_password)`: ensures `"miner"` is loaded/created, derives a miner address, mines `101 - height` if `height < 101`, and if spendable balance is below `0.01` BTC (`1_000_000` sats), mines batches of `10` blocks up to a hard cap of `500` additional blocks (returning an actionable volume-reset error if halving exhaustion prevents reaching `0.01` BTC).
        *   `mint_token(&self, label, nostr_pubkey) -> Result<TokenUtxo, String>`: generates an ephemeral `SecretKey` from `rand` bytes, derives `CompressedPublicKey` and its P2PKH `Address`/`ScriptBuf`, funds it with `100_000` sats via `send_to_address`, mines 1 block, inspects `get_raw_transaction(&funding_txid, Some(&funding_block_hash))` to locate the `vout` whose `script_pubkey` matches the ephemeral P2PKH script, builds a 1-in/1-out legacy transaction sending `99_000` sats to `build_token_script_pubkey(...)` at `vout[0]`, signs `vin[0]` via `SighashCache::legacy_signature_hash` (`<DER_sig || EcdsaSighashType::All> <33-byte compressed pubkey>`), broadcasts via `send_raw_transaction`, and mines 1 block.
        *   `update_token(&self, previous: &TokenUtxo, label, new_nostr_pubkey) -> Result<TokenUtxo, String>`: spends `previous.outpoint` at `vin[0]` to `build_token_script_pubkey(label, new_nostr_pubkey, same_pubkey_hash)` at `vout[0]` (`previous.amount - 1_000` sats), signing with the full `previous.script_pubkey` as `script_code`, broadcasts, and mines 1 block.
        *   `revoke_token(&self, previous: &TokenUtxo) -> Result<(Txid, u64), String>`: spends `previous.outpoint` at `vin[0]` to a plain uninscribed P2PKH `vout[0]` (`previous.amount - 1_000` sats), signing with `previous.script_pubkey` as `script_code`, broadcasts, and mines 1 block.
    *   Co-located `mod test_bitcoin_token_tx` verifying that `build_token_script_pubkey` produces a `TxOut` parsed by `name_token::Inscription::from_txout`, wrapped into `NameToken::new(label, vec![NameTokenEvent::Minted { position: Position::new(BlockId { .. }, ..), inscription }])`, and converted into `dns_nostr_token::dns_nostr_token::DnsNostrToken` with the expected `label` and `nostr_pubkey`.

#### 27. `end_to_end/src/server_scenarios.rs` & `end_to_end/src/wallet_scenarios.rs` (Create)

*   **Scope**: Server & Wallet End-to-End Scenario Modules
*   **Changes**:
    *   `server_scenarios.rs`: exposes `pub async fn run_all(config: &E2eConfig, dns_client: &DnsClient, bitcoin_client: &RegtestTokenClient) -> Result<(), (&'static str, String)>` executing five sequential scenarios (failing fast on the first error, returning diagnostic strings with `label`, `txid(s)`, `block_height(s)`, `pubkey(s)`, `ResponseCode`, and `txt_records`):
        1.  `not_found_returns_nxdomain`
        2.  `minted_resolves_txt_pubkey`
        3.  `updated_rotates_to_new_nostr_pubkey`
        4.  `first_is_root_ignores_competing_mint` (after mining competing Mint #2, publishes and mints a sequential canary label in the next block, polls DNS until the canary resolves, then asserts `<label>` still resolves to `pubkey_a`)
        5.  `revoked_returns_nxdomain`
    *   `wallet_scenarios.rs`: exposes `pub fn run_all() -> Result<(), (&'static str, String)>` executing five black-box scenarios using `E2eConfig::wallet_binary_path()` inside auto-cleaned temporary `DNS_NOSTR_WALLET_DIR` directories (failing fast on the first error):
        1.  `create_generates_seed_sets_0600_and_rejects_duplicate`
        2.  `import_args_and_address_auto_advance`: imports `"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"` and asserts regtest BIP 84 addresses advance across invocations (`bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk`, `bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh`, and a distinct third address) without leaking seed words.
        3.  `import_via_stdin_and_address`: pipes the same mnemonic via stdin and confirms `address` returns `bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk`.
        4.  `network_env_overrides_and_invalid_network`: verifies `testnet` (`tb1q...`), `mainnet` (`bc1q...`), and invalid network non-zero exit.
        5.  `error_paths_do_not_leak_seed_words`: verifies `address` with no wallet, `import zebra zebra` (without echoing `"zebra"`), and `address --index 0` exit non-zero.

#### 28. `end_to_end/README.md` & `end_to_end/src/README.md` (Create)

*   **Scope**: Crate & Source EGM Contract Documentation
*   **Changes**: Document the `end_to_end` binary crate role, managed `dns_nostr_server` child process using the shared `DNS_NOSTR_DB_PATH` SQLite DB, fail-fast `main.rs` runner, harness and scenario modules, invariants, side effects, and verification commands.

*   **Phase 4 Checkpoint**: `cargo build --workspace && just test` passes offline; with no manual `just serve` running, `just test e2e` passes all 10 scenarios and exits cleanly with `dns_nostr_server` stopped.

---

### Phase 5: Repository & Agent Verification Documentation

#### 29. `AGENTS.md` (Modify)

*   **Scope**: Agent Entry-Point Documentation
*   **Changes**: Add `just` to prerequisites; replace `regtest_node/` with `end_to_end/` (binary crate) in `Subdirectories & Contracts`; update `Commands` to `just test`, `just test up`, `just test down`, `just test e2e` (self-contained), `just serve`, `just wallet create | import [WORDS...] | address`, and `just fund <BTC> <Address>`; remove obsolete `cargo check --workspace --all-features`; document `dns_nostr_server`'s `DNS_NOSTR_*` environment variables and the one-time `docker compose down -v` + `rm ./data/name-tokens.sqlite*` migration.

#### 30. `.agents/rules/agent-orchestration.md` (Modify)

*   **Scope**: Agent Verification Policy
*   **Changes**: Update Section 3 (`Verification Before Done`) so `just test` (`cargo test --workspace`) remains mandatory for every task and `just test e2e` (self-contained; builds and spawns `dns_nostr_server` automatically) is required for any task modifying `name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`, `end_to_end`, `docker-compose.yml`, or `justfile`.

*   **Phase 5 Checkpoint**: Final verification plan below passes end-to-end.

### Verification Plan

*   **Offline Compilation**:
    *   `cargo build --workspace` succeeds with zero errors and zero warnings (compiling `end_to_end` and all E2E scenarios offline without any feature flags).
*   **Offline Unit Tests (Zero Infrastructure)**:
    *   With Docker stopped (`just test down`), `just test` (`cargo test --workspace`) passes all co-located unit tests across `name_token`, `dns_nostr_token`, `dns_nostr_server`, `dns_nostr_wallet`, and `end_to_end` (`config`, `server_process`, `labels`, `dns_client`, `bitcoin_token_tx`) without executing `end_to_end`'s `main()`.
*   **Infrastructure & `justfile` Smoke Verification**:
    *   `just test up` starts `bitcoin-core` (`bitcoin/bitcoin:28.1-alpine`) and `nostr-relay` (`scsibug/nostr-rs-relay:0.8.8`) and exits `0` once both healthchecks pass.
    *   `just fund 0.5 <regtest_address>` initializes/loads `"miner"`, mines maturity blocks on a fresh volume, sends `0.5` BTC, and mines 1 confirmation block; running `just fund` a second time mines only 1 confirmation block.
    *   Using a scratch `DNS_NOSTR_WALLET_DIR`: `just wallet create` exits `0` and prints a 12-word BIP39 phrase; `just wallet address` exits `0` and prints a `bcrt1q...` address; piping the 12-word phrase into `just wallet import` in a fresh scratch directory exits `0` and subsequent `just wallet address` exits `0` with the matching `bcrt1q...` address.
*   **Full-Stack End-to-End Verification (Self-Contained)**:
    *   With **no** `just serve` running, run `just test e2e` twice consecutively against persistent Docker volumes and the shared `./data/name-tokens.sqlite` database; confirm all 5 server scenarios (`not_found_returns_nxdomain`, `minted_resolves_txt_pubkey`, `updated_rotates_to_new_nostr_pubkey`, `first_is_root_ignores_competing_mint`, `revoked_returns_nxdomain`) and all 5 wallet scenarios pass on both runs, and confirm no `dns_nostr_server` child process remains bound to `:1053` after exit.
    *   Verify port-conflict early failure: start `just serve` on `:1053`, run `just test e2e`, and confirm `ManagedServer::start` fails immediately (not after the 10 s timeout) with `dns_nostr_server`'s bind error and guidance to stop `just serve` or override `DNS_NOSTR_BIND_ADDR`; then stop `just serve`.
    *   Verify resilience of `just serve`: run `just serve`, stop and restart containers (`just test down && just test up`) while `just serve` stays running, confirm `BlockchainWatcher` logs transient errors to `stderr` without exiting and resumes syncing after `just test up`, then stop `just serve`.

--------------------------------------------------------------------------------

<!-- Provenance: Indicates initial structural drafting via EGM. -->

*Drafted with EGM*
