# end_to_end/src

## Overview
Binary entry point, harness modules, and fail-fast scenario suites for the `end_to_end` crate: environment configuration wrapping `dns_nostr_server::environment_config`, managed `dns_nostr_server` process lifecycle, unique label generation, async UDP DNS client + readiness check, Nostr zone-file publisher, P2PKH-signed regtest Token UTXO builder/client, and server/wallet E2E scenarios.

## Interface
- [`main.rs`](./main.rs): `#[tokio::main] async fn main() -> ExitCode` fail-fast composition root starting `ManagedServer`, running `server_scenarios::run_all` and `wallet_scenarios::run_all`, and printing `[PASS]`/`[FAIL]` diagnostics.
- [`config.rs`](./config.rs): `E2eConfig` (`from_environment`, `from_lookup`, `poll_timeout`, `poll_interval`, `server_binary_path`, `wallet_binary_path`, redacted `Debug`). Wraps `dns_nostr_server::environment_config::EnvironmentConfig` (including `db_path`) and maps wildcard bind hosts (`0.0.0.0` / `::`) to loopback (`127.0.0.1` / `::1`).
- [`server_process.rs`](./server_process.rs): `ManagedServer` (`start`, `captured_stderr`, `Drop`) spawning `dns_nostr_server` with inherited `DNS_NOSTR_*`, draining stderr into a bounded buffer, polling DNS readiness with early-exit detection, and killing/reaping the child on drop.
- [`labels.rs`](./labels.rs): `unique_label(prefix: &str) -> dns_nostr_token::label::Label`.
- [`dns_client.rs`](./dns_client.rs): `DnsResponse`, `DnsClientError`, `DnsClient` (`new`, `query_txt`, `preflight_check`).
- [`nostr_publisher.rs`](./nostr_publisher.rs): `publish_txt_zone_file(relay_url, keys, txt_value) -> Result<EventId, String>`.
- [`just_runner.rs`](./just_runner.rs): `JustRunner` (`new`, `run`) executing workspace `just` recipes with forwarded `DNS_NOSTR_BITCOIN_RPC_USER` / `DNS_NOSTR_BITCOIN_RPC_PASSWORD`.
- [`bitcoin_token_tx.rs`](./bitcoin_token_tx.rs): `TokenUtxo`, `build_token_script_pubkey(label, nostr_pubkey, owner_pubkey_hash) -> ScriptBuf`, `RegtestTokenClient` (`new`, `fund_address`, `fund_address_unconfirmed`, `mine_one_block`, `mint_token`, `update_token`, `revoke_token`).
- [`server_scenarios.rs`](./server_scenarios.rs): `run_all(config, dns_client, bitcoin_client) -> Result<(), (&'static str, String)>`.
- [`wallet_scenarios.rs`](./wallet_scenarios.rs): `run_all(config, bitcoin_client) -> Result<(), (&'static str, String)>`.

## Invariants
- `E2eConfig` reuses `dns_nostr_server::environment_config::EnvironmentConfig` as the single config parser and redacts `bitcoin_rpc_password` as `"[REDACTED]"` in `Debug`.
- `ManagedServer` verifies `DNS_NOSTR_BIND_ADDR` is available before spawning (preventing a readiness race against an already-running `just serve`), drains child stderr into a bounded UTF-8 buffer (preventing pipe deadlocks), checks `child.try_wait()` during readiness polling so startup failures fail immediately, and kills + reaps the child in `Drop`.
- `unique_label` always produces a valid RFC-1034 `Label` (`<= 63` bytes, lowercase ASCII starting with `e2e-`).
- `DnsClient::query_txt` enforces a `3`-second UDP timeout and validates response message ID matching; `preflight_check` uses a `1`-second timeout and requires `ResponseCode::NXDomain` for an unminted label under `<zone_origin>`.
- `publish_txt_zone_file` asserts relay acceptance (`!output.success.is_empty()`) and unconditionally calls `client.shutdown().await`.
- `RegtestTokenClient` delegates regtest wallet setup, funding, mining, raw-tx fetch, broadcast, and block-height queries to workspace `just` commands (`miner-address`, `miner-balance`, `miner-fund`, `miner-mineblock`, `bitcoin-cli`) via `JustRunner`, and signs real legacy P2PKH signatures over `<inscription> <P2PKH>` scripts (preserving same-index `0 -> 0` continuity on updates).

## Side Effects
- Unit tests in `config.rs`, `server_process.rs`, `labels.rs`, `dns_client.rs`, `just_runner.rs`, and `bitcoin_token_tx.rs` perform no network or disk I/O.
- Runtime execution spawns `dns_nostr_server`, `dns_nostr_wallet`, and workspace `just` subcommands (`docker compose exec -T bitcoin-core bitcoin-cli ...`), and performs UDP DNS queries and Nostr WebSocket publishing.

## Verification
- Unit tests: `cargo test -p end_to_end` (included in `just test`)
- Full E2E runner: `just test-e2e`
