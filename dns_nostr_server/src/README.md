# dns_nostr_server/src

## Overview
Source of the `dns_nostr_server` crate: the infrastructure adapters and composition root of the DNS-Nostr server. It syncs Name-Token state from Bitcoin Core into SQLite and answers DNS queries under the zone `nostr.dns.name.` by resolving a label's token and fetching the zone file from a Nostr relay. The library target is named `lib` (see `Cargo.toml`, `src/lib.rs`); the binary is `dns_nostr_server` (`src/main.rs`).

## Interface
- [`lib.rs`](./lib.rs): declares `pub mod blockchain_watcher`, `nostr_authority`, `nostr_events_repository`, `sqlite_name_token_repository`.
- [`main.rs`](./main.rs): `#[tokio::main] async fn main()` composition root. Wires `SqliteNameTokenRepository::create()` → `NameTokenService::new` → `BlockchainWatcher::new(..).spawn()`, `DnsNostrService::new`, `NostrEventsRepository::new("ws://localhost:8080")`, `NostrAuthority::new("nostr.dns.name.", ..)`, registered in a hickory `Catalog`, served on a `UdpSocket` bound to `0.0.0.0:1053`.
- [`blockchain_watcher.rs`](./blockchain_watcher.rs): `BlockchainWatcher::new(Arc<NameTokenService>)`, `BlockchainWatcher::spawn(self) -> JoinHandle<()>`; private `SYNC_INTERVAL` (10 s), `bitcoin_client`, `watch_blockchain`, `sync_blocks`.
- [`nostr_authority.rs`](./nostr_authority.rs):
  - `trait GetDnsNostrToken { fn get_token(&self, &Label) -> impl Future<Output = Option<DnsNostrToken>> + Send }`, implemented for `DnsNostrService`.
  - `NostrAuthority<GetTokenT: GetDnsNostrToken>` with `NostrAuthority::new(zone, token_source, NostrEventsRepository)`, and a hickory `Authority` impl (`lookup`, `search`, `ns`, `soa`, `soa_secure`, `get_nsec_records`, `update`, `zone_type`, `is_axfr_allowed`, `origin`).
- [`nostr_events_repository.rs`](./nostr_events_repository.rs): `NostrEventsRepository::new(String)`, `get_last_text_note_from_pubkey(PublicKey) -> Option<Event>`.
- [`sqlite_name_token_repository.rs`](./sqlite_name_token_repository.rs): `SqliteNameTokenRepository` (`Clone`), `create()`, and the `name_token::NameTokenRepository` impl: `get_name_token_by_outpoint`, `get_name_tokens_by_label`, `save_block_updates`, `get_name_tokens_by_block`, `undo_block_updates`, `get_chain`.

## Invariants
- `NostrAuthority::zone_type` is `ZoneType::Primary`; `is_axfr_allowed` is `false`; `update` returns `ResponseCode::NotImp`; `get_nsec_records` returns `NotImp`.
- A name is served only if `is_valid_dns_nostr_name` holds: it has more labels than the origin and lies within it (`zone_of`). The token label is the label immediately left of the origin; the per-query zone is that label plus the origin. A label failing `Label::new` yields `None` and the lookup returns `NXDomain`.
- The zone file is the `content` of the first event returned for `Kind::TextNote` by the token's `nostr_pubkey` (`text_notes.first()`), parsed with hickory `Parser`; parse or authority-creation failure is logged with `eprintln!` and results in `NXDomain`.
- SQLite tables: `chain_checkpoints(height, blockhash)` and `name_tokens(first_txid, first_vout, label_hex, events_json, last_txid, last_vout)` with `PRIMARY KEY (first_txid, first_vout)`. `save_block_updates` and `undo_block_updates` each run in a single transaction; token writes are upserts keyed by the first position. `get_chain` is ordered by `height ASC`.
- `create_tables` runs `DROP TABLE IF EXISTS` on legacy tables `state` and `name_token_versions` on every start; `chain_checkpoints` and `name_tokens` are kept (`CREATE TABLE IF NOT EXISTS`).
- The repository's connection is shared via `Arc<Mutex<rusqlite::Connection>>`; labels are stored as lowercase hex.
- `BlockchainWatcher` resumes from `NameTokenService::tip()` or, if `None`, from a genesis checkpoint (`LocalChain::from_genesis_hash`), and applies blocks via `apply_block_connected_to`.
- Storage and RPC failures are handled by `unwrap`/`expect` (panic), not returned errors.

## Side Effects
- Disk: opens/creates `./data/name-tokens.sqlite` (relative to the working directory); creates tables, drops the legacy ones above, writes checkpoints and tokens.
- Network (Bitcoin Core RPC): `http://0.0.0.0:18443` with credentials `rpcuser`/`rpcpassword`, polled every 10 seconds in a spawned tokio task that loops forever.
- Network (Nostr): each `get_last_text_note_from_pubkey` call creates a new `nostr_sdk::Client`, connects to `ws://localhost:8080` and fetches with a 10 s timeout.
- Network (DNS): binds UDP `0.0.0.0:1053`; serves until `block_until_done` returns.
- Stdout/stderr: prints `Starting blockchain watcher...` and `Synced block at height {}`; parse and authority errors go to stderr.

## Verification
- Run: `cargo test -p dns_nostr_server`
