# end_to_end/src/dns_nostr_server_e2e

## Overview
Black-box end-to-end test suite for `dns_nostr_server`, organized by Name-Token lifecycle feature (`lookup_e2e/`, `mint_e2e/`, `update_e2e/`, `revoke_e2e/`) with server-local harness helpers under `utils/`.

## Interface
- [`mod.rs`](./mod.rs): `pub async fn run_all(config: &E2eConfig, bitcoin_client: &RegtestBitcoinClient) -> Result<usize, ScenarioError>` — starts `ManagedServer`, runs all 4 feature suites sequentially (5 scenarios total), prints captured server stderr on failure, and stops/reaps the server on exit.
- `utils/`: `ManagedServer` (`server_process.rs`), `DnsClient` (`dns_client.rs`), `unique_label`/`fqdn_for` (`labels.rs`), `publish_txt_zone_file` (`nostr_publisher.rs`), `TokenTxClient`/`TokenUtxo`/`build_token_script_pubkey` (`token_tx.rs`).
- `lookup_e2e/`: `not_found_returns_nxdomain`.
- `mint_e2e/`: `minted_resolves_txt_pubkey`, `first_is_root_ignores_competing_mint`.
- `update_e2e/`: `updated_rotates_to_new_nostr_pubkey`.
- `revoke_e2e/`: `revoked_returns_nxdomain`.

## Invariants
- Depends only on `crate::utils` and workspace domain crates; zero dependency on `dns_nostr_wallet_e2e`.
- `ManagedServer` verifies `DNS_NOSTR_BIND_ADDR` is free before spawning, drains stderr into a bounded UTF-8 buffer, detects early exits during readiness polling, and kills + reaps the child in `Drop`.
- Every scenario uses a unique letter-prefixed lowercase label (`e2e-<prefix>-<unix_ts>-<rand_hex>`) and real P2PKH-signed Token UTXOs, enabling collision-free repeated runs.

## Side Effects
- Unit tests (`test_dns_client`, `test_labels`, `test_server_process`, `test_token_tx`) perform no network or disk I/O.
- Runtime execution spawns `dns_nostr_server`, broadcasts P2PKH-signed Token UTXO transactions on regtest Bitcoin Core, publishes Nostr `Kind::TextNote` zone files, and sends UDP DNS queries.

## Verification
- Unit tests: `cargo test -p end_to_end dns_nostr_server_e2e` (included in `just test`)
- Full E2E runner: `just test-e2e`
