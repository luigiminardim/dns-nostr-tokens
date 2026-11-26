# end_to_end/src

## Overview
Binary composition root and project-scoped E2E test suites for the `end_to_end` crate: shared harness utilities (`utils/`), `dns_nostr_server` E2E suite (`dns_nostr_server_e2e/`), and `dns_nostr_wallet` E2E suite (`dns_nostr_wallet_e2e/`).

## Interface
- [`main.rs`](./main.rs): `#[tokio::main] async fn main() -> ExitCode` fail-fast composition root loading `E2eConfig`, initializing `RegtestBitcoinClient`, running `dns_nostr_server_e2e::run_all` and `dns_nostr_wallet_e2e::run_all`, and printing `[PASS]`/`[FAIL]` diagnostics.
- [`utils/`](./utils/README.md): cross-project utilities (`E2eConfig`, `JustCommand`, `RegtestBitcoinClient`, `scenario_runner`).
- [`dns_nostr_server_e2e/`](./dns_nostr_server_e2e/README.md): server E2E suite (`utils/`, `lookup_e2e/`, `mint_e2e/`, `update_e2e/`, `revoke_e2e/`).
- [`dns_nostr_wallet_e2e/`](./dns_nostr_wallet_e2e/README.md): wallet E2E suite (`utils/`, `create_e2e/`, `import_e2e/`, `address_e2e/`, `balance_e2e/`, `send_e2e/`).

## Invariants
- Dependency direction is strictly acyclic: `utils/` has zero dependencies on either project suite, and `dns_nostr_server_e2e/` and `dns_nostr_wallet_e2e/` do not depend on each other.
- Each tested feature has its own `*_e2e/` folder, and each atomic scenario lives in its own `.rs` file.

## Side Effects
- Unit tests across `utils/` and `dns_nostr_server_e2e/utils/` perform no network or disk I/O.
- Runtime execution spawns `dns_nostr_server`, `dns_nostr_wallet`, and workspace `just` subcommands (`docker compose exec -T bitcoin-core bitcoin-cli ...`), and performs UDP DNS queries and Nostr WebSocket publishing.

## Verification
- Unit tests: `cargo test -p end_to_end` (included in `just test`)
- Full E2E runner: `just test-e2e`
