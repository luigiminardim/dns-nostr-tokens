# src

## Overview
Crate root of `dns_nostr_wallet`. `lib.rs` exposes the three layers (dependencies point inward: `adapters` → `application` → `domain`) and `main.rs` is the composition root of the `dns_nostr_wallet` binary.

## Interface
- [`lib.rs`](./lib.rs): `pub mod adapters`, `pub mod application`, `pub mod domain`.
- [`main.rs`](./main.rs): `main() -> ExitCode` (parses `Cli`, loads `EnvironmentConfig`, opens SQLite, constructs `SqliteNameTokenWalletRepository`, wires `CreateWallet`, `ImportWallet`, `GetReceiveAddress` and `CliController`, prints the result), private `fail`.
- [adapters](./adapters/README.md) — config, SQLite repository adapter and CLI.
- [application](./application/README.md) — use cases.
- [domain](./domain/README.md) — `DnsNostrWallet` entity, `KeyDerivationPolicy` and `NameTokenWalletRepository` port.

## Invariants
- `main` exits with `ExitCode::FAILURE` and `error: …` on stderr for any config, directory, database or command error; success output goes to stdout.
- The database file permissions are set to `0o600` on Unix after opening (best effort; failures are ignored).
- `main` contains wiring only; business logic stays in the layers.

## Side Effects
- Creates the wallet directory (`fs::create_dir_all`), opens/creates `<wallet_dir>/wallet.sqlite`, changes its file mode, reads stdin and environment, writes stdout/stderr.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
