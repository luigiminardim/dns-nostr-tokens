# end_to_end

## Overview
Workspace binary crate providing self-contained black-box end-to-end verification for `dns_nostr_server` (spawning the host-built `dns_nostr_server` binary against the shared SQLite database, live Bitcoin Core regtest, and live Nostr relay) and `dns_nostr_wallet` (black-box CLI execution against temporary wallet directories), organized into a project / feature / scenario hierarchy.

## Interface
- Package `end_to_end` (`Cargo.toml`):
  - Binary target (`src/main.rs`): fail-fast E2E composition root wiring `utils/`, `dns_nostr_server_e2e/`, and `dns_nostr_wallet_e2e/` — see [`./src/README.md`](./src/README.md).
  - Shared utilities (`src/utils/`): `E2eConfig`, `JustCommand`, `RegtestBitcoinClient`, and `scenario_runner` — see [`./src/utils/README.md`](./src/utils/README.md).
  - Server suite (`src/dns_nostr_server_e2e/`): 5 sequential black-box DNS scenarios across 4 Name-Token lifecycle feature folders (`lookup_e2e/`, `mint_e2e/`, `update_e2e/`, `revoke_e2e/`) with server-local helpers in `utils/` — see [`./src/dns_nostr_server_e2e/README.md`](./src/dns_nostr_server_e2e/README.md).
  - Wallet suite (`src/dns_nostr_wallet_e2e/`): 13 sequential atomic black-box CLI scenarios across 5 feature folders (`create_e2e/`, `import_e2e/`, `address_e2e/`, `balance_e2e/`, `send_e2e/`) with wallet-local helpers in `utils/` — see [`./src/dns_nostr_wallet_e2e/README.md`](./src/dns_nostr_wallet_e2e/README.md).

## Invariants
- `cargo test --workspace` / `just test` compiles the entire `end_to_end` binary crate and runs its offline co-located unit tests with zero Docker or network dependencies and without executing `main()`.
- `dns_nostr_server_e2e` spawns the host-built `dns_nostr_server` binary via an RAII guard (`ManagedServer`) inheriting `DNS_NOSTR_*` (using the shared `DNS_NOSTR_DB_PATH` SQLite database and configured `DNS_NOSTR_BIND_ADDR`), waits for DNS readiness, fails fast on early exit or the first scenario error, and terminates/reaps `dns_nostr_server` unconditionally on exit.
- All DNS scenarios use unique letter-prefixed lowercase labels (`e2e-<prefix>-<unix_ts>-<rand_hex>`) and real P2PKH-signed Token UTXOs (`<inscription> <P2PKH>`), allowing repeated runs against persistent Docker volumes and the shared SQLite database without collisions.
- `dns_nostr_wallet_e2e` isolates every scenario in a fresh auto-cleaned temporary `DNS_NOSTR_WALLET_DIR`.

## Side Effects
- When executed (`just test-e2e` / `cargo run -p end_to_end`):
  - Spawns and stops `dns_nostr_server` using the shared `DNS_NOSTR_DB_PATH` SQLite database (default `./data/name-tokens.sqlite`).
  - Shells out to workspace `just` recipes (`miner-address`, `miner-balance`, `miner-fund`, `miner-mineblock`, `bitcoin-cli`) against the local Docker `bitcoin-core` container to initialize/use the regtest `"miner"` wallet, mine blocks, and broadcast P2PKH-signed Token UTXO transactions; publishes Nostr `Kind::TextNote` zone files to `DNS_NOSTR_NOSTR_RELAY_URL`; and sends UDP DNS queries to the managed `dns_nostr_server`.
  - Spawns the compiled `dns_nostr_wallet` binary and creates/deletes temporary wallet directories under the OS temp directory.

## Verification
- Offline build & unit tests: `just build && just test`
- Full-stack self-contained end-to-end suite: `just test-e2e`
