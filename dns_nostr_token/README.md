# dns_nostr_token

## Overview
Rust library crate (`dns_nostr_token`, edition 2021) holding the DNS-Nostr specifics on top of the generic `name_token` crate. Dependencies in `Cargo.toml`: `async-trait` (workspace), `name_token` (path `../name_token`), `nostr-sdk` 0.41.0; dev-dependencies: `bitcoin` (workspace), `bdk_chain` 0.23.3. The crate contains only `Cargo.toml` and `src/`.

## Subdirectories & Boundaries
- src: [./src/README.md](./src/README.md) - `Label` validation, `DnsNostrToken` conversion from `NameToken`, and `DnsNostrService` lookup.
