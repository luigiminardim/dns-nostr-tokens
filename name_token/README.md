# name_token

## Overview
Cargo crate (`name_token`, edition 2021) holding the Name-Token domain. Dependencies (`Cargo.toml`): `async-trait`, `bdk_chain` (feature `serde`), `bitcoin`, `serde`; dev-dependency `tokio`. It contains only `Cargo.toml` and `src/`.

## Subdirectories & Boundaries
- src: [./src/README.md](./src/README.md) - Inscription parsing, token lifecycle, `NameTokenRepository` port, `NameTokenService`, in-memory repository.

## Verification
- Run: `cargo test -p name_token`
