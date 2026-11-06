# dns_nostr_server

## Overview
Crate for the DNS-Nostr server: a DNS authority for the zone `nostr.dns.name.` that resolves a label through its Name-Token on Bitcoin (synced from Bitcoin Core into SQLite) and serves the zone file published on a Nostr relay. It depends on the `name_token` and `dns_nostr_token` crates and implements their ports with SQLite, Bitcoin RPC and Nostr adapters.

## Interface
- Package `dns_nostr_server` (`Cargo.toml`): library target named `lib` (`src/lib.rs`) and binary `dns_nostr_server` (`src/main.rs`). Run with `cargo run -p dns_nostr_server`.
- Source contract: [`./src/README.md`](./src/README.md)

## Invariants
- The library target is named `lib`, so `main.rs` imports from `lib::...`.
- Endpoints are hardcoded in source: DNS UDP `0.0.0.0:1053`, Bitcoin RPC `http://0.0.0.0:18443`, Nostr relay `ws://localhost:8080`, zone `nostr.dns.name.`, database `./data/name-tokens.sqlite`.

## Side Effects
- See [`./src/README.md`](./src/README.md): SQLite file writes, Bitcoin RPC polling, Nostr relay connections, UDP listener.

## Verification
- Run: `cargo test -p dns_nostr_server`
