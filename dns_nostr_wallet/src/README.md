# src

## Overview
Crate root of `dns_nostr_wallet`. `lib.rs` exposes the three layers (dependencies point inward: `adapters` → `application` → `domain`) and `main.rs` is the composition root of the `dns_nostr_wallet` binary.

## Interface
- [`lib.rs`](./lib.rs): `pub mod adapters`, `pub mod application`, `pub mod domain`.
- [`main.rs`](./main.rs): `main() -> ExitCode` (parses `Cli`, loads `EnvironmentConfig`, constructs lazy `SqliteNameTokenWalletRepository::open(config.db_path())`, `RpcBlockchainAdapter`, `SyncService`, all five use cases, and `CliController`, prints the result), private `fail`.
- [adapters](./adapters/README.md) — config, Bitcoin Core RPC adapter, lazy SQLite repository adapter, and CLI controller.
- [application](./application/README.md) — use cases.
- [domain](./domain/README.md) — `DnsNostrWallet` entity, `SyncService`, `KeyDerivationPolicy`, `BlockchainPort`, and `NameTokenWalletRepository`.

## Invariants
- `main` exits with `ExitCode::FAILURE` and `error: …` on stderr for any config or command error; success output goes to stdout.
- `main` never eagerly creates directories or opens `wallet.sqlite`; database file creation and `0o600` Unix permissions are owned lazily by `SqliteNameTokenWalletRepository::create`.
- `main` contains wiring only; business logic stays in the layers.

## Side Effects
- Reads stdin and environment variables, writes stdout/stderr, and delegates persistence and Bitcoin Core RPC I/O to the wired adapters.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
