# dns_nostr_server/src

## Overview
Source of the `dns_nostr_server` crate: the infrastructure adapters and composition root of the DNS-Nostr server. It syncs Name-Token state from Bitcoin Core into SQLite and answers DNS queries under a configurable zone origin (default `nostr.dns.name.`) by resolving a label's token and fetching the zone file from a Nostr relay.

## Interface
- [`lib.rs`](./lib.rs): declares `pub mod blockchain_watcher`, `environment_config`, `nostr_authority`, `nostr_events_repository`, `sqlite_name_token_repository`.
- [`main.rs`](./main.rs): `#[tokio::main] async fn main() -> ExitCode` composition root. Loads `EnvironmentConfig::from_environment()`, opens `SqliteNameTokenRepository::open(&config.db_path)`, initializes `NameTokenService::new(...)`, spawns `BlockchainWatcher`, wires `DnsNostrService`, `NostrEventsRepository`, and `NostrAuthority` into a hickory `Catalog`, binds UDP, and supervises `server.block_until_done()` alongside the watcher `JoinHandle` via `tokio::select!`.
- [`environment_config.rs`](./environment_config.rs): `BIND_ADDR_VARIABLE`, `ZONE_ORIGIN_VARIABLE`, `DB_PATH_VARIABLE`, `BITCOIN_RPC_URL_VARIABLE`, `BITCOIN_RPC_USER_VARIABLE`, `BITCOIN_RPC_PASSWORD_VARIABLE`, `NOSTR_RELAY_URL_VARIABLE`, `SYNC_INTERVAL_MS_VARIABLE`; `EnvironmentConfig` (`from_environment`, `from_lookup`, redacted `Debug`); `ConfigError` (`InvalidBindAddr`, `InvalidZoneOrigin`, `EmptyValue`, `InvalidUrl`, `InvalidSyncInterval`).
- [`blockchain_watcher.rs`](./blockchain_watcher.rs): `BlockchainWatcherConfig` (redacted `Debug`), `WatcherFatalError::GenesisMismatch { stored, node }`, `BlockchainWatcher::new(Arc<NameTokenService>, BlockchainWatcherConfig)`, `BlockchainWatcher::spawn(self) -> JoinHandle<Result<(), WatcherFatalError>>`.
- [`nostr_authority.rs`](./nostr_authority.rs):
  - `trait GetDnsNostrToken { fn get_token(&self, &Label) -> impl Future<Output = Option<DnsNostrToken>> + Send }`, implemented for `DnsNostrService`.
  - `NostrAuthority<GetTokenT: GetDnsNostrToken>` with `NostrAuthority::new(zone, token_source, NostrEventsRepository)`, and a hickory `Authority` impl (`lookup`, `search`, `ns`, `soa`, `soa_secure`, `get_nsec_records`, `update`, `zone_type`, `is_axfr_allowed`, `origin`).
- [`nostr_events_repository.rs`](./nostr_events_repository.rs): `NostrEventsRepository::new(String)`, `get_last_text_note_from_pubkey(PublicKey) -> Option<Event>`.
- [`sqlite_name_token_repository.rs`](./sqlite_name_token_repository.rs): `SqliteRepositoryError { CreateParentDir, Sqlite }`, `SqliteNameTokenRepository` (`Clone`), `open(impl AsRef<Path>) -> Result<Self, SqliteRepositoryError>`, `in_memory() -> Result<Self, SqliteRepositoryError>`, and the `name_token::NameTokenRepository` impl: `get_name_token_by_outpoint`, `get_name_tokens_by_label`, `save_block_updates`, `get_name_tokens_by_block`, `undo_block_updates`, `get_chain`.

## Invariants
- `EnvironmentConfig` validates all `DNS_NOSTR_*` variables, normalizes `DNS_NOSTR_ZONE_ORIGIN` to a trailing-dot non-root FQDN, rejects empty/whitespace-only values and bare URL schemes without a host, enforces `DNS_NOSTR_SYNC_INTERVAL_MS >= 1`, and redacts `bitcoin_rpc_password` as `"[REDACTED]"` in `Debug`.
- `NostrAuthority::zone_type` is `ZoneType::Primary`; `is_axfr_allowed` is `false`; `update` returns `ResponseCode::NotImp`; `get_nsec_records` returns `NotImp`.
- A name is served only if `is_valid_dns_nostr_name` holds: it has more labels than the origin and lies within it (`zone_of`). Lookup failures (invalid name/label, missing or revoked token, missing Nostr note, parse/authority failure) are logged to `stderr` and return `NXDomain`.
- `NostrEventsRepository` uses a `2`-second `fetch_events` timeout, returns `None` on invalid relay URL or fetch error (logging to `stderr` without panicking), and calls `nostr_client.shutdown().await` unconditionally on every exit path.
- `SqliteNameTokenRepository::open` creates missing parent directories, enables `PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, and `PRAGMA busy_timeout = 5000`, defines `chain_checkpoints(height INTEGER PRIMARY KEY, blockhash CHAR(64) NOT NULL)` with upsert on conflict, retains `DROP TABLE IF EXISTS` for legacy `state` and `name_token_versions`, and creates lookup indexes `idx_name_tokens_label_hex` and `idx_name_tokens_last_outpoint`.
- `BlockchainWatcher` logs transient Bitcoin RPC errors to `stderr` and retries after `sync_interval`. On each sync tick it compares Bitcoin Core's block `0` hash against `NameTokenService::genesis_hash()` and returns `Err(WatcherFatalError::GenesisMismatch { .. })` on mismatch; `main.rs` terminates non-zero on watcher fatal errors, unexpected exits, or panics, and on `NameTokenServiceError::MissingGenesis` at startup.

## Side Effects
- Disk: creates missing parent directories and opens/writes the SQLite database (plus `-wal`/`-shm` sidecars) at `DNS_NOSTR_DB_PATH` (default `./data/name-tokens.sqlite`).
- Network (Bitcoin Core RPC): polls `DNS_NOSTR_BITCOIN_RPC_URL` every `DNS_NOSTR_SYNC_INTERVAL_MS` (default `1000` ms) in a supervised tokio task.
- Network (Nostr): opens a `nostr_sdk::Client` per DNS lookup to `DNS_NOSTR_NOSTR_RELAY_URL` (`2`-second fetch timeout) and shuts it down before returning.
- Network (DNS): binds UDP on `DNS_NOSTR_BIND_ADDR` (default `0.0.0.0:1053`); exits cleanly when `server.block_until_done()` completes or the watcher terminates fatally.
- Stdout/stderr: logs redacted startup configuration, watcher startup, and synced block heights to stdout; logs config/startup/watcher/Nostr/lookup errors to stderr.

## Verification
- Unit tests: `cargo test -p dns_nostr_server` (or `just test`)
- End-to-end tests (with `just serve` running): `just test e2e`
