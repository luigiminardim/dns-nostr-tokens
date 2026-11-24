# dns_nostr_server

## Overview
Crate for the DNS-Nostr server: a DNS authority for a configurable zone (default `nostr.dns.name.`) that resolves a label through its Name-Token on Bitcoin (synced from Bitcoin Core into SQLite) and serves the zone file published on a Nostr relay. It depends on the `name_token` and `dns_nostr_token` crates and implements their ports with SQLite, Bitcoin RPC, and Nostr adapters.

## Interface
- Package `dns_nostr_server` (`Cargo.toml`): standard library target `dns_nostr_server` (`src/lib.rs`) and binary `dns_nostr_server` (`src/main.rs`). Run with `just serve` or `cargo run -p dns_nostr_server`.
- Environment variables (validated at startup via `environment_config::EnvironmentConfig`):
  - `DNS_NOSTR_BIND_ADDR` (default `0.0.0.0:1053`)
  - `DNS_NOSTR_ZONE_ORIGIN` (default `nostr.dns.name.`)
  - `DNS_NOSTR_DB_PATH` (default `./data/name-tokens.sqlite`)
  - `DNS_NOSTR_BITCOIN_RPC_URL` (default `http://127.0.0.1:18443`)
  - `DNS_NOSTR_BITCOIN_RPC_USER` (default `rpcuser`)
  - `DNS_NOSTR_BITCOIN_RPC_PASSWORD` (default `rpcpassword`; redacted in logs/`Debug`)
  - `DNS_NOSTR_NOSTR_RELAY_URL` (default `ws://localhost:8080`)
  - `DNS_NOSTR_SYNC_INTERVAL_MS` (default `1000`)
- Source contract: [`./src/README.md`](./src/README.md)

## Invariants
- Invalid `DNS_NOSTR_*` configuration causes a non-zero exit with a descriptive error on `stderr`; `bitcoin_rpc_password` is redacted as `"[REDACTED]"` in `Debug` output and startup logs.
- Startup rejects persisted SQLite databases missing height `0` (`NameTokenServiceError::MissingGenesis`), and runtime sync aborts non-zero if SQLite's height-0 genesis hash diverges from Bitcoin Core (`WatcherFatalError::GenesisMismatch`).

## Side Effects
- See [`./src/README.md`](./src/README.md): SQLite file/WAL writes and parent-directory creation, Bitcoin RPC polling, Nostr relay connections, UDP listener.

## Verification
- Unit tests: `cargo test -p dns_nostr_server` (or `just test`)
- End-to-end tests (with `just serve` running): `just test e2e`
