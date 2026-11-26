# end_to_end/src/dns_nostr_wallet_e2e

## Overview
Black-box end-to-end CLI test suite for `dns_nostr_wallet`, organized by CLI feature (`create_e2e/`, `import_e2e/`, `address_e2e/`, `balance_e2e/`, `send_e2e/`) with wallet-local harness helpers under `utils/`.

## Interface
- [`mod.rs`](./mod.rs): `pub fn run_all(config: &E2eConfig, bitcoin_client: &RegtestBitcoinClient) -> Result<usize, ScenarioError>` — runs all 5 feature suites sequentially (13 atomic scenarios total).
- `utils/`: `TempWalletDir` (`temp_wallet_dir.rs`), `WalletCli`, `BIP39_VECTOR`, `REGTEST_EXTERNAL_0` (`wallet_cli.rs`).
- `create_e2e/`: `generates_seed_and_sets_0600_permissions`, `rejects_duplicate_wallet`, `rejects_genesis_mismatch_network`, `rejects_invalid_network`.
- `import_e2e/`: `import_from_cli_args`, `import_from_stdin`, `rejects_invalid_mnemonic_without_leaking_seed`, `recovers_historical_balance`.
- `address_e2e/`: `auto_advances_external_addresses`, `rejects_uninitialized_wallet`, `rejects_unknown_flag`.
- `balance_e2e/`: `tracks_unconfirmed_and_confirmed_receive`.
- `send_e2e/`: `sends_and_spends_consecutive_unconfirmed_change`.

## Invariants
- Depends only on `crate::utils` and `bitcoin`; zero dependency on `dns_nostr_server_e2e` or Name-Token inscription logic.
- Every scenario isolates state in a fresh `TempWalletDir` auto-cleaned on `Drop`.

## Side Effects
- Runtime execution spawns the compiled `dns_nostr_wallet` binary, creates and deletes temporary wallet directories under the OS temp directory, and funds/mines regtest transactions via `RegtestBitcoinClient`.

## Verification
- Offline build & unit tests: `cargo test -p end_to_end` (included in `just test`)
- Full E2E runner: `just test-e2e`
