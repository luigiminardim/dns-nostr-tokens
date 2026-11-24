# end_to_end

## Overview
Workspace binary crate providing self-contained black-box end-to-end verification for `dns_nostr_server` (spawning the host-built `dns_nostr_server` binary against the shared SQLite database, live Bitcoin Core regtest, and live Nostr relay) and `dns_nostr_wallet` (black-box CLI execution against temporary wallet directories).

## Interface
- Package `end_to_end` (`Cargo.toml`):
  - Binary target (`src/main.rs`): fail-fast E2E runner wiring the shared harness and scenario modules — see [`./src/README.md`](./src/README.md).
  - Server scenarios (`src/server_scenarios.rs`): five sequential black-box DNS scenarios (`not_found_returns_nxdomain`, `minted_resolves_txt_pubkey`, `updated_rotates_to_new_nostr_pubkey`, `first_is_root_ignores_competing_mint`, `revoked_returns_nxdomain`).
  - Wallet scenarios (`src/wallet_scenarios.rs`): five sequential black-box CLI scenarios covering `create` (12-word phrase, `0600` DB permissions, duplicate rejection), `import` via CLI args and stdin, `address` auto-advance, `DNS_NOSTR_NETWORK` / `DNS_NOSTR_WALLET_DIR` overrides, and error paths without leaking seed words.

## Invariants
- `cargo test --workspace` / `just test` compiles the entire `end_to_end` binary crate and runs its offline co-located unit tests with zero Docker or network dependencies and without executing `main()`.
- `end_to_end` spawns the host-built `dns_nostr_server` binary via an RAII guard (`ManagedServer`) inheriting `DNS_NOSTR_*` (using the shared `DNS_NOSTR_DB_PATH` SQLite database and configured `DNS_NOSTR_BIND_ADDR`), waits for DNS readiness, fails fast on early exit or the first scenario error, and terminates/reaps `dns_nostr_server` unconditionally on exit.
- All DNS scenarios use unique letter-prefixed lowercase labels (`e2e-<prefix>-<unix_ts>-<rand_hex>`) and real P2PKH-signed Token UTXOs (`<inscription> <P2PKH>`), allowing repeated runs against persistent Docker volumes and the shared SQLite database without collisions.
- `wallet_scenarios` isolates every scenario in a fresh auto-cleaned temporary `DNS_NOSTR_WALLET_DIR`.

## Side Effects
- When executed (`just test e2e` / `cargo run -p end_to_end`):
  - Spawns and stops `dns_nostr_server` using the shared `DNS_NOSTR_DB_PATH` SQLite database (default `./data/name-tokens.sqlite`).
  - Initializes/uses the Bitcoin Core regtest `"miner"` wallet, mines blocks on regtest, broadcasts P2PKH-signed Token UTXO transactions, publishes Nostr `Kind::TextNote` zone files to `DNS_NOSTR_NOSTR_RELAY_URL`, and sends UDP DNS queries to the managed `dns_nostr_server`.
  - Spawns the compiled `dns_nostr_wallet` binary and creates/deletes temporary wallet directories under the OS temp directory.

## Verification
- Offline build & unit tests: `cargo build --workspace && just test`
- Full-stack self-contained end-to-end suite: `just test e2e`
