# AGENTS.md

> This project is a work-in-progress. The codebase currently contains the
> `name_token` and `dns_nostr_token` (domain libraries), `dns_nostr_server`,
> `dns_nostr_wallet`, and `end_to_end` crates. The wallet supports BIP39 seed
> creation/import, BIP 84 Bitcoin Core RPC sync, receive addresses, balance
> checks, and sending Bitcoin; Name-Token operations (mint/update/revoke and
> Token UTXO scanning) are not built yet. All guidance below applies to **what
> is built today**.

> **Agent orchestration rules** (workflow, core principles, behavioral guardrails)
> are defined in [`.agents/rules/agent-orchestration.md`](.agents/rules/agent-orchestration.md)
> and are automatically loaded for every task.

---

## Project Overview

DNS-Nostr Tokens implements the **Name-Token** protocol: unique names on Bitcoin
via UTXO inscriptions. The current application is **DNS-Nostr**, where a DNS
server resolves `<label>.nostr.dns.name.` by reading the label's Name-Token from
Bitcoin and serving the zone file from the token's Nostr pubkey. Domain terms
are defined in [`docs/domain.md`](docs/domain.md).

### Documentation layout

- **`README.md` (root)** is prose for humans who want to understand what the
  project does. Keep it free of agent-facing material: no directory indexes,
  contracts, invariants, commands for agents or repository structure. Those
  belong in this file.
- **This file (`AGENTS.md`)** is the entry point for agents: architecture,
  directory contract links, commands and conventions.
- **Per-directory `README.md` files** hold the EGM contract of each directory
  (`Interface`, `Invariants`, `Side Effects`, `Verification`). Update them in
  the same change that alters a directory's public symbols, invariants or side
  effects.
- **`docs/domain.md`** is the ubiquitous language and domain model.
- **`docs/designs/`** holds design documents. Keep each document's `**Status:**`
  field up to date:
  - Whenever a design document is reviewed, update its status to `Reviewed`.
  - Whenever a design document is implemented, update its status to `Implemented`.

### Architecture

```
DNS Client ──DNS──▶ dns_nostr_server ──RPC──▶ Bitcoin Node (regtest)
                           │                      ▲
                           └──WebSocket──▶ Nostr Relay  │
                                                  │ RPC
                                          dns_nostr_wallet

dns_nostr_server ──▶ dns_nostr_token ──▶ name_token
dns_nostr_wallet ──▶ dns_nostr_token
end_to_end       ──▶ dns_nostr_server, dns_nostr_token, name_token
```

Dependencies point inward; `main.rs` in each binary crate is the composition
root. The wallet supports Standard UTXO (BIP 84) sync, balance, and send
operations against Bitcoin Core RPC; do not add Name-Token operations or Token
UTXO scanning unless explicitly asked.

### Subdirectories & Contracts

| Directory | Contract | Role |
| --------- | -------- | ---- |
| `name_token/` | [README](name_token/README.md), [src](name_token/src/README.md) | Name-Token domain, repository port, service |
| `dns_nostr_token/` | [README](dns_nostr_token/README.md), [src](dns_nostr_token/src/README.md) | `Label`, `DnsNostrToken`, `DnsNostrService` |
| `dns_nostr_server/` | [README](dns_nostr_server/README.md), [src](dns_nostr_server/src/README.md) | DNS authority, SQLite repository, Bitcoin watcher, Nostr lookups, env config |
| `dns_nostr_wallet/` | [README](dns_nostr_wallet/README.md), [src](dns_nostr_wallet/src/README.md), [adapters](dns_nostr_wallet/src/adapters/README.md), [application](dns_nostr_wallet/src/application/README.md), [domain](dns_nostr_wallet/src/domain/README.md) | Wallet CLI |
| `end_to_end/` | [README](end_to_end/README.md), [src](end_to_end/src/README.md), [utils](end_to_end/src/utils/README.md), [dns_nostr_server_e2e](end_to_end/src/dns_nostr_server_e2e/README.md), [dns_nostr_wallet_e2e](end_to_end/src/dns_nostr_wallet_e2e/README.md) | Self-contained black-box E2E binary crate (`src/main.rs`) and project/feature/scenario suites |

Other: `docs/` (domain model & designs), `report.md` (technical report),
`data/zone-files/` (example zone), `.data/` (gitignored local runtime data),
`docker-compose.yml` (on-demand `bitcoin/bitcoin:28.1-alpine` regtest +
`nostr-rs-relay`), `justfile` (dev/test orchestration).

---

## Design & Architecture Decisions

These decisions are **binding for all agents** working on this codebase.

### 1. Domain-Driven Design (DDD)

The project follows Domain-Driven Design principles. The canonical reference for
the domain model and ubiquitous language is [`docs/domain.md`](docs/domain.md).

**Agent rules:**
- **Before planning any task** that touches domain concepts, read the
  `domain-driven-design` skill (`SKILL.md`).
- **Before modifying domain code**, read `docs/domain.md` to understand the
  current ubiquitous language and domain model.
- **After any change** that introduces, renames, or removes a domain concept,
  update `docs/domain.md` to keep it in sync with the code.

### 2. Clean Architecture

The codebase follows Clean Architecture (Dependency Rule): source-code
dependencies point inward — from frameworks/drivers → adapters → use cases →
entities.

**Agent rules:**
- **Before planning any task** that involves architectural decisions (new
  modules, layer placement, dependency direction), read the
  `clean-architecture` skill (`SKILL.md`).
- Inner layers define interfaces; outer layers implement them.
- `main.rs` is the composition root — it wires concrete implementations.

### 3. Clean Code

All code follows Clean Code discipline: meaningful names, small focused
functions, clean error handling, and co-located tests.

**Agent rules:**
- **Before modifying any code**, read the `clean-code` skill (`SKILL.md`).
- Apply the Boy Scout Rule: leave every file cleaner than you found it.
- Follow the Quick Diagnostic checklist from the skill before considering
  code complete.

---

## Commands

Prerequisites: Rust toolchain, Docker & Docker Compose, and `just`.

```bash
just build                                      # build all workspace crates
just test                                       # offline workspace unit tests (no Docker needed)
just test-e2e                                   # run self-contained full-stack E2E suite (starts Docker + spawns dns_nostr_server)
just containers-up                              # start bitcoin-core (regtest) + nostr-relay and wait until healthy
just containers-down                            # stop containers (preserves Docker volumes)
just containers-reset                           # stop containers, wipe Docker volumes, and delete server SQLite DB
just serve                                      # run dns_nostr_server (UDP :1053 by default)
just wallet create | import [WORDS...] | address | balance | send <address> <btc> # run dns_nostr_wallet CLI
just bitcoin-cli <args...>                      # pass-through to regtest bitcoin-cli inside bitcoin-core container
just miner-address                              # print a regtest address from the local miner wallet
just miner-balance                              # print the spendable BTC balance of the local miner wallet
just miner-fund <address> <btc> [--no-mine]     # fund regtest address from miner wallet (auto-mines 1 block unless --no-mine)
just miner-mineblock [count]                    # mine regtest block(s) to miner wallet and print block hash(es)
just dns-query <label> [type]                   # query local dns_nostr_server via dig (default type TXT)
```

### Environment Variables

**`dns_nostr_server` (also consumed by `end_to_end`):**
- `DNS_NOSTR_BIND_ADDR` (default `0.0.0.0:1053`)
- `DNS_NOSTR_ZONE_ORIGIN` (default `nostr.dns.name.`)
- `DNS_NOSTR_DB_PATH` (default `./data/name-tokens.sqlite`)
- `DNS_NOSTR_BITCOIN_RPC_URL` (default `http://127.0.0.1:18443`)
- `DNS_NOSTR_BITCOIN_RPC_USER` (default `rpcuser`)
- `DNS_NOSTR_BITCOIN_RPC_PASSWORD` (default `rpcpassword`; redacted in logs/`Debug`)
- `DNS_NOSTR_NOSTR_RELAY_URL` (default `ws://localhost:8080`)
- `DNS_NOSTR_SYNC_INTERVAL_MS` (default `1000`)

**`dns_nostr_wallet`:**
- `DNS_NOSTR_NETWORK` (`regtest` default, `testnet`, `mainnet`)
- `DNS_NOSTR_WALLET_DIR` (default `./.data/dns-nostr-wallet`)
- `DNS_NOSTR_BITCOIN_RPC_URL` (default `http://127.0.0.1:18443`)
- `DNS_NOSTR_BITCOIN_RPC_USER` (default `rpcuser`)
- `DNS_NOSTR_BITCOIN_RPC_PASSWORD` (default `rpcpassword`; redacted in `Debug`)

---

## Code Style

- Module-per-file; tests co-located as `#[cfg(test)] mod test_<module_name>` at the bottom of each file.
- Domain ports are async traits (`async-trait` or `impl Future`); domain and use-case errors are explicit enums, not strings.
- Domain value types derive `Debug, Clone, PartialEq, Eq` (plus `serde` when serialized); entities holding secret keys implement custom `Debug` to avoid leaking secrets.
- Libraries expose `lib.rs`; `main.rs` is thin and only wires dependencies.
- Naming: `PascalCase` types, `snake_case` functions and files, `test_<concept>` test modules.
- **Architectural role naming convention**: Structs and file names must be postfixed with their architectural class type / role:
  - **Use Cases** (`application/`): Postfixed with `Usecase` and `*_usecase.rs` (e.g., `SendBitcoinUsecase` in `send_bitcoin_usecase.rs`, `GetBalanceUsecase` in `get_balance_usecase.rs`, `CreateWalletUsecase` in `create_wallet_usecase.rs`).
  - **Domain Services** (`domain/`): Postfixed with `Service` and `*_service.rs` (e.g., `SyncService` in `sync_service.rs`, `NameTokenService` in `name_token_service.rs`, `DnsNostrService` in `dns_nostr_service.rs`).
  - **Repositories** (`domain/` and `adapters/`): Postfixed with `Repository` and `*_repository.rs` (`Repository` is already a role postfix — do not add `Port` or `Adapter`, e.g., `NameTokenWalletRepository` in `name_token_wallet_repository.rs`, `SqliteNameTokenWalletRepository` in `sqlite_name_token_wallet_repository.rs`).
  - **Ports** (`domain/`): Postfixed with `Port` and `*_port.rs` (e.g., `BlockchainPort` in `blockchain_port.rs`).
  - **Adapters** (`adapters/`): Postfixed with `Adapter` and `*_adapter.rs` (e.g., `RpcBlockchainAdapter` in `rpc_blockchain_adapter.rs`).
  - **Controllers** (`adapters/`): Postfixed with `Controller` and `*_controller.rs` (e.g., `CliController` in `cli_controller.rs`).
  - **Entities / Aggregates**: Ubiquitous language domain nouns without suffix (e.g., `DnsNostrWallet`, `Label`, `NameToken`).
  - **Policies**: Postfixed with `Policy` and `*_policy.rs` (e.g., `KeyDerivationPolicy` in `key_derivation_policy.rs`).
- Avoid creating one-line helper functions without expressive domain meaning that are not reused across multiple places.
- Avoid declaring constants in global/module scope if they are not reused; scope single-use constants locally inside the function or method that uses them.
- Keep helper functions that belong to a policy, entity, service, or adapter as private methods inside its `impl` block rather than as free module-level functions.

---

## Debugging and Troubleshooting

- **Nostr**: inspect relay events with [nostrdebug.com](https://nostrdebug.com/) on `ws://localhost:8080`.
- **Bitcoin RPC**: `just bitcoin-cli <command>`.
- **SQLite state**: the server persists to `./data/name-tokens.sqlite` (WAL mode, gitignored); delete `./data/name-tokens.sqlite*` to force a full resync.
- **One-time migration / full local reset**: if upgrading from the legacy `regtest_node/` volume or resetting local regtest + SQLite state from scratch, run `just containers-reset`.
