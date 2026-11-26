# end_to_end/src/utils

## Overview
Shared cross-project E2E test utilities used by both `dns_nostr_server_e2e` and `dns_nostr_wallet_e2e`: environment configuration, workspace `just` command runner, regtest Bitcoin Core RPC/mining client, and scenario step reporting.

## Interface
- [`mod.rs`](./mod.rs): re-exports `config`, `just_command`, `regtest_bitcoin`, and `scenario_runner`.
- [`config.rs`](./config.rs): `E2eConfig` (`from_environment`, `from_lookup`, `poll_timeout`, `poll_interval`, `server_binary_path`, `wallet_binary_path`, redacted `Debug`).
- [`just_command.rs`](./just_command.rs): `JustCommand` (`new`, `run`) executing workspace `just` recipes with forwarded `DNS_NOSTR_BITCOIN_RPC_USER` / `DNS_NOSTR_BITCOIN_RPC_PASSWORD`.
- [`regtest_bitcoin.rs`](./regtest_bitcoin.rs): `RegtestBitcoinClient` (`new`, `fund_address`, `fund_address_unconfirmed`, `mine_one_block`, `get_raw_transaction`, `send_raw_transaction`, `parse_regtest_address`, `parse_btc_amount`).
- [`scenario_runner.rs`](./scenario_runner.rs): `ScenarioError` and `report_step(name, result) -> Result<usize, ScenarioError>`.

## Invariants
- Zero dependencies on `dns_nostr_server_e2e` or `dns_nostr_wallet_e2e`.
- `E2eConfig` reuses `dns_nostr_server::environment_config::EnvironmentConfig` as the single config parser and redacts `bitcoin_rpc_password` as `"[REDACTED]"` in `Debug`.
- `RegtestBitcoinClient` delegates regtest wallet initialization, funding, mining, raw-tx fetch, broadcast, and block-height queries to workspace `just` recipes via `JustCommand`.

## Side Effects
- Unit tests (`test_config`, `test_just_command`, `test_regtest_bitcoin`, `test_scenario_runner`) perform no network or disk I/O.
- Runtime execution invokes workspace `just` recipes against the local Docker `bitcoin-core` regtest container.

## Verification
- Unit tests: `cargo test -p end_to_end utils` (included in `just test`)
