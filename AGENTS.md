# AGENTS.md

> This project is a work-in-progress. The codebase currently contains the
> `name_token` (domain library) and `dns_nostr_server` crates. The DNS-Nostr
> Wallet described in the README does not exist yet. All guidance below applies
> to **what is built today**.

> **Agent orchestration rules** (workflow, core principles, behavioral guardrails)
> are defined in [`.agents/rules/agent-orchestration.md`](.agents/rules/agent-orchestration.md)
> and are automatically loaded for every task.

---

## Project Overview

DNS-Nostr Tokens implements the **Name-Token** protocol — a system for managing
unique names on the Bitcoin blockchain via UTXO inscriptions. The current focus
is a specific application called **DNS-Nostr**: operators of DNS servers can
resolve subdomains (e.g., `blog.nostr.dns.app`) by reading Name-Token
inscriptions from Bitcoin and fetching DNS records from the Nostr network.

### Key concepts

- **Name-Token**: A UTXO whose `scriptPubKey` contains an `OP_FALSE OP_IF … OP_ENDIF` inscription with a `"name"` header and a label.
- **DNS-Nostr Token**: A Name-Token with a `"dns-nostr"` protocol section containing a Nostr public key (hex).
- **First Confirmed Rule**: Among competing inscriptions for the same label, the earliest confirmed, non-revoked one wins.
- **Update = Spend + Reinscribe**: Updating a token means spending its UTXO and creating a new one with the same label.
- **Revocation = Spend without Reinscription**: Spending a token's UTXO without creating a new inscription for that label.

### Architecture (current)

```
DNS Client ──DNS query──▶ dns_nostr_server ──RPC──▶ Bitcoin Node (regtest)
                                │
                                └──WebSocket──▶ Nostr Relay
```

The server resolves subdomains under a configured zone (e.g., `nostr.dns.name.`)
by looking up the label's Name-Token on Bitcoin, extracting the Nostr pubkey,
and fetching DNS records from a Nostr relay.

The Name-Token domain lives in the standalone `name_token` crate (depends only
on `bitcoin`, `serde`, `async-trait`). Dependencies point inward:

```
dns_nostr_server ──▶ name_token ──▶ bitcoin
```

`name_token` defines the `NameTokenRepository` port and the `NameTokenService`
(`apply_block`, `get_name_token`). `dns_nostr_server` implements the port with
SQLite (`SqliteNameTokenRepository`) and feeds blocks from Bitcoin Core into the
service (`BlockchainWatcher`). `main.rs` wires them together.

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

## Tech Stack

| Layer          | Technology                                                                                      |
| -------------- | ----------------------------------------------------------------------------------------------- |
| Language       | Rust (edition 2021)                                                                             |
| Build system   | Cargo workspace                                                                                 |
| Async runtime  | Tokio (multi-thread)                                                                            |
| Bitcoin        | `bitcoin` + `bitcoincore-rpc` crates (RPC to Bitcoin Core)                                      |
| DNS            | `hickory-server` (UDP authority handler)                                                        |
| Nostr          | `nostr-sdk` (WebSocket relay client)                                                            |
| Persistence    | `rusqlite` (bundled SQLite) for caching Name-Token state                                        |
| Serialization  | `serde` + `serde_json`                                                                          |
| Dev infra      | Docker Compose — Bitcoin Core regtest node + `nostr-rs-relay`                                   |

---

## Repository Structure

```
dns-nostr-tokens/
├── Cargo.toml                    # Workspace root (members: name_token, dns_nostr_server; shared [workspace.dependencies])
├── Cargo.lock
├── README.md
├── docker-compose.yml            # Bitcoin regtest + Nostr relay
├── .gitignore
│
├── docs/
│   └── domain.md                 # Ubiquitous language & high-level domain model (DDD)
│
├── name_token/                   # Core domain library (no infrastructure dependencies)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # Module declarations + public re-exports
│       ├── inscription.rs        # Bytes, Inscription, InscriptionSection, InscriptionMetadata, parsing
│       ├── name_token.rs         # NameToken, UpdateNameTokenError, lifecycle, First Confirmed Rule
│       ├── name_token_repository.rs  # NameTokenRepository async trait (port)
│       └── name_token_service.rs # NameTokenService: apply_block, get_name_token, next_block_height
│
├── dns_nostr_server/             # DNS server application (depends on name_token)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # Composition root — wires the DNS server on UDP :1053
│       ├── lib.rs                # Module declarations
│       ├── sqlite_name_token_repository.rs  # SQLite implementation of name_token::NameTokenRepository
│       ├── blockchain_watcher.rs # Bitcoin Core RPC sync loop feeding blocks to NameTokenService
│       ├── dns_nostr_token.rs    # DNS-Nostr-specific token logic (extracts npub from sections)
│       ├── dns_nostr_token_repository.rs  # Queries NameTokenService for DNS-Nostr tokens
│       ├── nostr_authority.rs    # Hickory Authority impl — handles DNS lookups via Nostr
│       └── nostr_events_repository.rs  # Fetches DNS record events from a Nostr relay
│
├── regtest_node/                 # Docker image for Bitcoin Core in regtest mode
│   ├── Dockerfile
│   ├── bitcoin.conf              # regtest=1, RPC credentials: rpcuser/rpcpassword
│   └── entry_point.sh            # Starts bitcoind, creates miner wallet, mines blocks
│
└── data/
    └── zone-files/               # (currently empty)
```

---

## Setup Commands

### Prerequisites

- Rust toolchain (install via [rustup](https://rustup.rs/))
- Docker & Docker Compose

### Start development infrastructure

```bash
docker compose up -d
```

This launches:
- **Bitcoin Core** regtest node on `localhost:18443` (RPC) / `localhost:18444` (P2P)
  - Credentials: `rpcuser` / `rpcpassword`
  - Auto-mines 101 blocks on startup, then 1 block every 10 minutes
- **Nostr relay** (`nostr-rs-relay`) on `ws://localhost:8080`
  - Debuggable via [nostrdebug.com](https://nostrdebug.com/) connected to `ws://localhost:8080`

### Build

```bash
cargo build --workspace
```

### Run the DNS server

```bash
cargo run -p dns_nostr_server
```

The server binds to `0.0.0.0:1053` (UDP) and serves the zone `nostr.dns.name.`.

---

## Testing Instructions

### Unit tests

The codebase uses Rust's built-in `#[cfg(test)]` modules. Tests are co-located
with production code (no separate `tests/` directory).

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p name_token
```

Key test files:
- `name_token/src/inscription.rs` — inscription parsing, metadata ordering
- `name_token/src/name_token.rs` — token lifecycle (create/update/revoke), valid-token selection
- `name_token/src/name_token_service.rs` — block application and label resolution against an in-memory repository fake
- Other modules follow the same pattern with `#[cfg(test)]` blocks

### Integration testing (manual)

1. Start the infrastructure: `docker compose up -d`
2. Run the server: `cargo run -p dns_nostr_server`
3. Query with dig: `dig @localhost -p 1053 <label>.nostr.dns.name A`

---

## Code Style

### Conventions

- **Rust edition 2021** — use modern Rust idioms.
- **Module-per-file** — each domain concept lives in its own file under `src/`.
- **Tests co-located** — `#[cfg(test)] mod test_<module_name>` at the bottom of each file.
- **Trait-based abstraction** — repositories use `async-trait` for trait objects.
- **Error handling** — domain errors are explicit enums (e.g., `UpdateNameTokenError`), not stringly-typed.
- **Type aliases** — `pub type Bytes = Vec<u8>` is used throughout for raw byte data.
- **Derive conventions** — structs derive `Debug, Clone, PartialEq, Eq` and optionally `serde::Serialize, serde::Deserialize`.

### Naming

- Structs: `PascalCase` (e.g., `NameToken`, `InscriptionSection`)
- Functions: `snake_case` (e.g., `generate_name_token_updates`, `select_valid_name_token`)
- Test modules: `test_<concept>` (e.g., `test_inscription`, `test_name_token`)
- Files: `snake_case.rs` matching the module name

### Architecture patterns

- The crate exposes both a library (`lib.rs`) and a binary (`main.rs`).
- `main.rs` is thin — it only wires dependencies together and starts the server.
- Domain logic is in the library modules; the binary is just the composition root.

---

## Environment

### Required environment / config

- Bitcoin Core RPC is hardcoded to `localhost:18443` with credentials `rpcuser`/`rpcpassword` (see `regtest_node/bitcoin.conf`).
- Nostr relay is hardcoded to `ws://localhost:8080`.
- DNS zone is hardcoded to `nostr.dns.name.`.
- The server listens on UDP port `1053`.

### Files to never commit

Per `.gitignore`:
- `target/` — Rust build artifacts
- `.env` — environment variables
- `*.sqlite` — local daabase files

---

## Debugging and Troubleshooting

- **Nostr debugging**: Use [nostrdebug.com](https://nostrdebug.com/) connected to `ws://localhost:8080` to inspect relay events.
- **Bitcoin RPC**: Use `docker compose exec bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword <command>` to interact with the node.
- **DNS queries**: Use `dig @localhost -p 1053 <label>.nostr.dns.name <record_type>` to test resolution.
- **SQLite state**: The server persists Name-Token state in a local `.sqlite` file (gitignored). Delete it to force a full resync from Bitcoin.

---

## Additional Notes

- The DNS-Nostr Wallet component described in the README **does not exist yet**. Do not create stubs or scaffolding for it unless explicitly asked.
- The Name-Token protocol is general-purpose; `dns-nostr` is just one protocol section type. The code is designed to support multiple protocol sections per inscription.
- Positional correlation matters for updates: the Nth input spending a token must correspond to the Nth output reinscribing it (see `generate_name_token_updates`).
