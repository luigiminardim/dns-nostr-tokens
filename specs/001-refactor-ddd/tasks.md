# Tasks: ddd-artifacts

**Feature**: `ddd-artifacts`
**Branch**: `001-refactor-ddd`
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) | **Data Model**: [data-model.md](data-model.md)

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Cargo workspace restructuring and crate skeleton initialization

- [X] T001 Configure Cargo workspace members in root `Cargo.toml` to include `"crates/name_token_kernel"` and `"dns_nostr_server"`
- [X] T002 Initialize `crates/name_token_kernel/Cargo.toml` with `bitcoin`, `serde`, `serde_json`, and `thiserror` dependencies
- [ ] T003 [P] Create initial module exports and directory structure in `crates/name_token_kernel/src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Common error types and position value objects required by all domain components

**⚠️ CRITICAL**: Must be completed before User Story 1 implementation

- [ ] T004 Define strongly-typed error enums (`DomainError`, `ParseError`) in `crates/name_token_kernel/src/error.rs`
- [ ] T005 [P] Implement `BlockchainPosition` value object in `crates/name_token_kernel/src/model/position.rs` with `(block_height, block_index, vout, txid)` ordered by `(block_height, block_index, vout)`

**Checkpoint**: Foundation ready - User Story 1 implementation can begin

---

## Phase 3: User Story 1 - Shared Kernel for Name-Token (Priority: P1) 🎯 MVP

**Goal**: Implement the Core Domain Shared Kernel in `crates/name_token_kernel` with generic byte-based `TokenLabel`, equal-priority dual script parsing (spendable envelopes and OP_RETURN), event-sourced `NameToken` aggregate root, and `NameTokenValidator`.

**Independent Test**: Run `cargo test -p name_token_kernel` to verify 100% of domain rules, parsing, and lifecycle logic in complete isolation without live nodes.

### Tests for User Story 1 ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [ ] T006 [P] [US1] Unit tests for `TokenLabel` in `crates/name_token_kernel/src/model/token_label.rs` verifying arbitrary byte sequences up to 255 bytes are valid and empty/oversized bytes are rejected
- [ ] T007 [P] [US1] Unit tests for `InscriptionParser` in `crates/name_token_kernel/src/parsing/inscription.rs` verifying equal-priority extraction from spendable envelopes (`OP_FALSE OP_IF ... OP_ENDIF`) and `OP_RETURN` payload scripts
- [ ] T008 [P] [US1] Unit tests for event-sourced `NameToken` aggregate in `crates/name_token_kernel/src/model/name_token.rs` verifying state reconstruction from `Minted`, `Transferred`, `Revoked` events and rejection of mutations on revoked tokens
- [ ] T009 [P] [US1] Unit tests for `NameTokenValidator` in `crates/name_token_kernel/src/validation/validator.rs` verifying deterministic First-is-Root tie-breaking using the `(blockheight, blockindex, vout)` position tuple

### Implementation for User Story 1

- [ ] T010 [P] [US1] Implement `TokenLabel` value object in `crates/name_token_kernel/src/model/token_label.rs` (constraint: "Must not be empty. Length must not exceed protocol maximum (255 bytes). Does not enforce RFC-1034 rules")
- [ ] T011 [P] [US1] Implement `InscriptionSection` and `Inscription` value objects in `crates/name_token_kernel/src/model/inscription.rs` holding `TokenLabel` and protocol sections
- [ ] T012 [P] [US1] Implement domain events (`NameTokenMinted`, `NameTokenTransferred`, `NameTokenRevoked`, `NameTokenDomainEvent`) in `crates/name_token_kernel/src/model/events.rs`
- [ ] T013 [US1] Implement `NameToken` event-sourced aggregate root in `crates/name_token_kernel/src/model/name_token.rs` (invariants: "Active transitions to Active via Transferred, Active transitions to Revoked via Revoked, rejects mutations once in Revoked state with DomainError::TokenAlreadyRevoked")
- [ ] T014 [US1] Implement `InscriptionParser` in `crates/name_token_kernel/src/parsing/inscription.rs` parsing spendable envelopes (`OP_FALSE OP_IF ... OP_ENDIF`) and `OP_RETURN` scripts with equal priority
- [ ] T015 [US1] Implement `NameTokenValidator` domain service in `crates/name_token_kernel/src/validation/validator.rs` selecting the canonical root by earliest `(blockheight, blockindex, vout)` position tuple
- [ ] T016 [US1] Expose public API exports in `crates/name_token_kernel/src/lib.rs` matching `contracts/name-token-kernel-api.md`

**Checkpoint**: At this point, User Story 1 is fully functional and testable independently (`cargo test -p name_token_kernel` passes)

---

## Phase 4: User Story 2 - Separate Context for DNS Resolution (Priority: P2)

**Goal**: Integrate the Shared Kernel into `dns_nostr_server`, enforce RFC-1034 validation at the `DnsNostrToken` conversion boundary, and implement deterministic Nostr event conflict resolution.

**Independent Test**: Run `cargo test -p dns_nostr_server` to verify RFC-1034 enforcement, deterministic event resolution, and authority lookups with mocked relay responses.

### Tests for User Story 2 ⚠️

- [ ] T017 [P] [US2] Unit tests for `DnsNostrToken::try_from(&NameToken)` in `dns_nostr_server/src/dns_nostr_token.rs` verifying RFC-1034 validation (accepts lowercase `a-z`, digits `0-9`, hyphens; rejects uppercase, non-alphanumeric start, >63 chars) and rejection of revoked tokens
- [ ] T018 [P] [US2] Unit tests for `NostrConflictResolver` in `dns_nostr_server/src/nostr_conflict_resolver.rs` verifying highest `created_at` timestamp selection and lowest `event.id` tie-breaking

### Implementation for User Story 2

- [ ] T019 [US2] Add `name_token_kernel = { path = "../crates/name_token_kernel" }` dependency in `dns_nostr_server/Cargo.toml`
- [ ] T020 [US2] Refactor `DnsNostrToken` in `dns_nostr_server/src/dns_nostr_token.rs` to implement `TryFrom<&NameToken>`, validating RFC-1034 via `hickory_server::proto::rr::domain::Label` and extracting Nostr `PublicKey`
- [ ] T021 [P] [US2] Implement `NostrConflictResolver` in `dns_nostr_server/src/nostr_conflict_resolver.rs` (rule: "Highest created_at timestamp wins; if created_at is identical, lowest event.id wins")
- [ ] T022 [US2] Refactor `NostrEventsRepository` in `dns_nostr_server/src/nostr_events_repository.rs` to use `NostrConflictResolver` when fetching zone text notes from Nostr relays
- [ ] T023 [US2] Refactor `NostrAuthority` in `dns_nostr_server/src/nostr_authority.rs` to resolve queries via the refactored `DnsNostrToken` repository and conflict-resolved Nostr events

**Checkpoint**: At this point, User Stories 1 AND 2 both work independently and integrate cleanly (`cargo test --workspace` passes)

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Integration verification, cleanup, and documentation verification

- [ ] T024 [P] Refactor or re-export `name_token_kernel` types in `dns_nostr_server/src/name_token.rs` and `dns_nostr_server/src/name_token_repository.rs` to maintain backward compatibility with existing server modules
- [ ] T025 Run workspace compilation check via `cargo check --workspace` and resolve any compiler warnings or lint issues
- [ ] T026 Run complete test suite via `cargo test --workspace` ensuring 100% of domain tests and server integration tests pass
- [ ] T027 Execute end-to-end verification according to [quickstart.md](quickstart.md)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS User Story 1
- **User Story 1 (Phase 3)**: Depends on Foundational completion - BLOCKS User Story 2
- **User Story 2 (Phase 4)**: Depends on User Story 1 completion (`dns_nostr_server` consumes `name_token_kernel`)
- **Polish (Phase 5)**: Depends on User Stories 1 & 2 completion

### User Story Dependencies

- **User Story 1 (P1)**: Independent of external systems; pure domain logic in `crates/name_token_kernel`.
- **User Story 2 (P2)**: Consumes the Shared Kernel from User Story 1 as a Cargo path dependency.

### Within Each User Story

- Tests (if included) MUST be written and FAIL before implementation
- Value Objects and Models before Services
- Services before Adapters/Controllers
- Core implementation before integration
- Checkpoint validation before advancing to the next phase

### Parallel Opportunities

- **Phase 1 Setup**: T003 can run in parallel with T002 once T001 completes.
- **Phase 2 Foundational**: T005 can run in parallel with T004.
- **Phase 3 Tests**: T006, T007, T008, T009 can all be authored in parallel.
- **Phase 3 Models**: T010, T011, T012 can be authored in parallel.
- **Phase 4 Tests**: T017 and T018 can be authored in parallel.
- **Phase 4 Components**: T021 can be developed in parallel with T020.
- **Phase 5 Polish**: T024 can proceed in parallel with documentation checks.

---

## Parallel Example: User Story 1

```bash
# Launch all test authoring for User Story 1 in parallel:
Task: "Unit tests for TokenLabel in crates/name_token_kernel/src/model/token_label.rs"
Task: "Unit tests for InscriptionParser in crates/name_token_kernel/src/parsing/inscription.rs"
Task: "Unit tests for event-sourced NameToken aggregate in crates/name_token_kernel/src/model/name_token.rs"
Task: "Unit tests for NameTokenValidator in crates/name_token_kernel/src/validation/validator.rs"

# Launch model tasks in parallel:
Task: "Implement TokenLabel value object in crates/name_token_kernel/src/model/token_label.rs"
Task: "Implement InscriptionSection and Inscription value objects in crates/name_token_kernel/src/model/inscription.rs"
Task: "Implement domain events in crates/name_token_kernel/src/model/events.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (`Cargo.toml`, crate skeleton).
2. Complete Phase 2: Foundational (Error types, `BlockchainPosition`).
3. Complete Phase 3: User Story 1 (Core Domain Shared Kernel).
4. **STOP and VALIDATE**: Run `cargo test -p name_token_kernel` - 100% of domain rules verified!
5. This forms a complete, independently shippable Shared Kernel crate.

### Incremental Delivery

1. Setup + Foundation ready → `name_token_kernel` ready for modeling.
2. User Story 1 complete → Shared Kernel available for both Wallet and Server.
3. User Story 2 complete → DNS Server adapted to consume Shared Kernel with deterministic Nostr conflict resolution.
4. Polish complete → All tests pass across workspace.
