# AGENTS.md

> This project is a work-in-progress. The codebase currently contains the
> `name_token` and `dns_nostr_token` (domain libraries), `dns_nostr_server` and
> `dns_nostr_wallet` crates. The wallet currently only creates/imports a BIP39
> seed and derives addresses and keys; token operations and send/receive are not
> built yet. All guidance
> below applies to **what is built today**.

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

### Architecture

```
DNS Client ──DNS──▶ dns_nostr_server ──RPC──▶ Bitcoin Node (regtest)
                           │
                           └──WebSocket──▶ Nostr Relay

dns_nostr_server ──▶ dns_nostr_token ──▶ name_token
dns_nostr_wallet ──▶ dns_nostr_token
```

Dependencies point inward; `main.rs` in each binary crate is the composition
root. The wallet is only partially built (seed create/import, receive address,
key derivation): do not add token operations, UTXO scanning or send/receive
unless explicitly asked.

### Subdirectories & Contracts

| Directory | Contract | Role |
| --------- | -------- | ---- |
| `name_token/` | [README](name_token/README.md), [src](name_token/src/README.md) | Name-Token domain, repository port, service |
| `dns_nostr_token/` | [README](dns_nostr_token/README.md), [src](dns_nostr_token/src/README.md) | `Label`, `DnsNostrToken`, `DnsNostrService` |
| `dns_nostr_server/` | [README](dns_nostr_server/README.md), [src](dns_nostr_server/src/README.md) | DNS authority, SQLite repository, Bitcoin watcher, Nostr lookups |
| `dns_nostr_wallet/` | [README](dns_nostr_wallet/README.md), [src](dns_nostr_wallet/src/README.md), [adapters](dns_nostr_wallet/src/adapters/README.md), [application](dns_nostr_wallet/src/application/README.md), [domain](dns_nostr_wallet/src/domain/README.md) | Wallet CLI |
| `regtest_node/` | [README](regtest_node/README.md) | Bitcoin Core regtest Docker image |

Other: `docs/` (domain model), `report.md` (technical report), `data/zone-files/`
(example zone), `.data/` (gitignored local runtime data).

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

Prerequisites: Rust toolchain, Docker & Docker Compose.

```bash
docker compose up -d            # regtest node (RPC :18443) + nostr relay (ws://localhost:8080)
cargo build --workspace
cargo run -p dns_nostr_server   # UDP :1053; needs ./data/ to exist
cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address
cargo test --workspace          # co-located #[cfg(test)] modules, no infrastructure needed
dig @localhost -p 1053 <label>.nostr.dns.name A   # manual check
```

Wallet environment variables: `DNS_NOSTR_NETWORK` (`regtest` default, `testnet`,
`mainnet`) and `DNS_NOSTR_WALLET_DIR` (default `./.data/dns-nostr-wallet`).
Server endpoints are hardcoded (see the server README).

---

## Code Style

- Module-per-file; tests co-located as `#[cfg(test)] mod test_<module_name>` at the bottom of each file.
- Domain ports are async traits (`async-trait` or `impl Future`); domain and use-case errors are explicit enums, not strings.
- Domain value types derive `Debug, Clone, PartialEq, Eq` (plus `serde` when serialized); entities holding secret keys implement custom `Debug` to avoid leaking secrets.
- Libraries expose `lib.rs`; `main.rs` is thin and only wires dependencies.
- Naming: `PascalCase` types, `snake_case` functions and files, `test_<concept>` test modules.
- Avoid creating one-line helper functions without expressive domain meaning that are not reused across multiple places.
- Avoid declaring constants in global/module scope if they are not reused; scope single-use constants locally inside the function or method that uses them.
- Keep helper functions that belong to a policy, entity, service, or adapter as private methods inside its `impl` block rather than as free module-level functions.

---

## Debugging and Troubleshooting

- **Nostr**: inspect relay events with [nostrdebug.com](https://nostrdebug.com/) on `ws://localhost:8080`.
- **Bitcoin RPC**: `docker compose exec bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword <command>`.
- **SQLite state**: the server persists to `./data/name-tokens.sqlite` (gitignored); delete it to force a full resync.
