# Implementation Plan: ddd-artifacts

**Branch**: `001-refactor-ddd` | **Date**: 2026-09-30 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-refactor-ddd/spec.md`

## Summary

Extract and implement Domain-Driven Design (DDD) artifacts into an independent Cargo workspace crate (`crates/name_token_kernel`) serving as the Shared Kernel between current and future components (Wallet, Repository indexer, DNS Server). The Shared Kernel encapsulates an event-sourced `NameToken` aggregate root reconstructed from lifecycle domain events (`NameTokenMinted`, `NameTokenTransferred`, `NameTokenRevoked`), a generic byte-based `TokenLabel` value object, equal-priority dual script inscription parsing (spendable envelopes and OP_RETURN), and a `NameTokenValidator` domain service enforcing First-is-Root ordering via the full `(blockheight, blockindex, vout)` tuple. The DNS Bounded Context in `dns_nostr_server` consumes this kernel, strictly enforces RFC-1034 compliance when converting tokens to `DnsNostrToken`, and incorporates deterministic Nostr event conflict resolution.

## Technical Context

**Language/Version**: Rust 2021 edition (stable 1.75+)

**Primary Dependencies**:
- `bitcoin` (v0.32): script parsing, transaction models, outpoints, hashes
- `nostr-sdk` (v0.34): Nostr public keys, events, filters, relay clients
- `hickory-server` (v0.24): authoritative DNS server, protocol records
- `tokio`: asynchronous runtime
- `async-trait`: async trait definitions
- `serde`, `serde_json`: serialization support
- `thiserror`: strongly-typed domain and parsing errors

**Storage**: In-memory for pure unit tests / SQLite for indexer state

**Testing**: `cargo test --workspace` (pure unit tests with mocked UTXO inputs in `name_token_kernel`; integration tests with regtest fixture in `dns_nostr_server`)

**Target Platform**: Linux server / cross-platform Rust

**Project Type**: Multi-crate Cargo workspace (library crate `crates/name_token_kernel` + server binary/library `dns_nostr_server`)

**Performance Goals**: Sub-millisecond pure in-memory validation for domain rules; deterministic Nostr event selection in O(k log k)

**Constraints**: Business rules fully testable without running Bitcoin or Nostr nodes (Principle VII); zero `unwrap()`/`expect()` in production paths (Principle V & VIII); function length target <= 20 lines (Principle VIII)

**Scale/Scope**: Cargo workspace restructuring adding `crates/name_token_kernel` and refactoring `dns_nostr_server` to consume it

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Protocol Correctness** | **PASS** | Faithful implementation of inscription parsing (spendable envelope + OP_RETURN with equal priority), First-is-Root tuple, and deterministic UTXO fixtures. |
| **II. UTXO-Set as Single Source of Truth** | **PASS** | Token state transitions derived directly from blockchain positions and outputs. |
| **III. Strict Name Uniqueness** | **PASS** | First-is-Root strictly enforced via `(blockheight, blockindex, vout)` ordering in `NameTokenValidator`. |
| **IV. DNS RFC-1034 Compliance** | **PASS** | RFC-1034 is strictly validated at the DNS context boundary (`DnsNostrToken`), preserving protocol generality for `TokenLabel` in the core kernel. |
| **V. Security-First for Key Material** | **PASS** | No private keys logged or serialized unencrypted; standard audited crates used. |
| **VI. Domain-Driven Design** | **PASS** | Bounded contexts defined; Shared Kernel separated; event-sourced `NameToken` aggregate root; domain events defined. |
| **VII. Clean Architecture** | **JUSTIFIED EXCEPTION** | Inner crate `name_token_kernel` has no outer dependencies. Direct use of `bitcoin` and `nostr` types in domain allowed per FR-004 trade-off. |
| **VIII. Clean Code Baseline** | **PASS** | Small functions (<= 20 lines), typed errors (`Result`), no production `unwrap()`, high unit test coverage. |

## Project Structure

### Documentation (this feature)

```text
specs/001-refactor-ddd/
├── spec.md              # Feature specification
├── plan.md              # Implementation plan (this file)
├── research.md          # Phase 0 decisions & rationale
├── data-model.md        # Phase 1 domain models, aggregates, & context map
├── quickstart.md        # Phase 1 test and verification guide
├── contracts/           # Phase 1 interface contracts
│   ├── name-token-kernel-api.md
│   └── dns-resolution-api.md
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 task decomposition (/speckit-tasks output)
```

### Source Code (repository root)

```text
.
├── Cargo.toml                          # Workspace root referencing crates/name_token_kernel and dns_nostr_server
├── crates/
│   └── name_token_kernel/              # Shared Kernel crate (Core Domain)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  # Module exports
│           ├── model/
│           │   ├── mod.rs
│           │   ├── token_label.rs      # Generic byte-based TokenLabel Value Object
│           │   ├── position.rs         # BlockchainPosition Value Object (tuple ordering)
│           │   ├── inscription.rs      # Inscription & InscriptionSection Value Objects
│           │   ├── events.rs           # Domain Events (Minted, Transferred, Revoked)
│           │   └── name_token.rs       # NameToken Event-Sourced Aggregate Root
│           ├── parsing/
│           │   ├── mod.rs
│           │   └── inscription.rs      # Equal-priority dual script parser (envelope + OP_RETURN)
│           └── validation/
│               ├── mod.rs
│               └── validator.rs        # NameTokenValidator Domain Service (First-is-Root)
└── dns_nostr_server/                   # DNS Bounded Context & Server
    ├── Cargo.toml                      # Depends on name_token_kernel
    └── src/
        ├── lib.rs
        ├── main.rs
        ├── dns_nostr_token.rs          # DnsNostrToken (enforces RFC-1034 for DNS resolution)
        ├── nostr_authority.rs          # Hickory DNS Authority
        ├── nostr_events_repository.rs  # Nostr relay client with conflict resolution
        ├── dns_nostr_token_repository.rs
        └── name_token_repository.rs
```

**Structure Decision**: A multi-crate Cargo workspace separating the core Shared Kernel (`crates/name_token_kernel`) from delivery and server infrastructure (`dns_nostr_server`). The Shared Kernel contains zero dependencies on `hickory-server` or database drivers.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Direct use of `bitcoin` and `nostr` types in domain model (FR-004 vs Principle VII) | Explicitly requested by user to eliminate mapping overhead between domain and blockchain/relay data structures. | Mapping through internal domain DTOs was rejected by user requirement FR-004. |
