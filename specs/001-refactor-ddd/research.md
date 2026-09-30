# Research & Design Decisions: ddd-artifacts

**Feature**: [001-refactor-ddd]
**Spec**: [spec.md](spec.md)

## Decision 1: Event-Sourced `NameToken` Aggregate Root Pattern

### Decision
Model `NameToken` as an event-sourced aggregate root reconstructed from an ordered sequence of domain events:
- `NameTokenMinted { label: TokenLabel, outpoint: OutPoint, inscription: Inscription, position: BlockchainPosition }`
- `NameTokenTransferred { label: TokenLabel, previous_outpoint: OutPoint, new_outpoint: OutPoint, position: BlockchainPosition }`
- `NameTokenRevoked { label: TokenLabel, spent_outpoint: OutPoint, spending_txid: Txid, position: BlockchainPosition }`

The aggregate maintains an internal status enum:
```rust
pub enum TokenStatus {
    Active { current_outpoint: OutPoint },
    Revoked { burned_by_position: BlockchainPosition, spending_txid: Txid },
}
```

State changes apply via a pure `apply(&mut self, event: NameTokenDomainEvent) -> Result<(), DomainError>` method. Any operation attempted on a `Revoked` token returns `DomainError::TokenAlreadyRevoked`.

### Rationale
- Satisfies **SC-001** (entities contain business logic and enforce invariants, rather than being anemic data bags).
- Accurately captures blockchain reality: a token's lifecycle is a linear sequence of transactions (Mint → [Transfers...] → [Revocation]).
- Full auditability: allows replaying historical state transitions from block transactions.
- Fully unit-testable without mock frameworks or database connections.

### Alternatives Considered
- **Stateless Anemic Struct**: Only store the latest `OutPoint` and `label`. Rejected because it moves invariant enforcement into services and fails SC-001.
- **Classic Mutable OOP Aggregate with internal mutation methods**: Rejected because it doesn't provide the historical transition audit trail or clean replay from blockchain blocks.

---

## Decision 2: Dual Script Inscription Parsing with Equal Priority

### Decision
Implement `parsing::inscription` in `name_token_kernel` to treat both spendable envelopes and `OP_RETURN` scripts with **equal priority**:
1. **Spendable Inscription Envelope**: `OP_FALSE OP_IF OP_PUSH "name" <label> ... OP_ENDIF <standard P2PKH / locking script>` (as specified in `report.md`).
2. **OP_RETURN Inscription Script**: `OP_RETURN OP_PUSH "name" <label> ...` (or push data containing the inscription format).

The parser inspects the output script:
- If the script starts with `OP_FALSE OP_IF`, it parses the spendable envelope.
- If the script starts with `OP_RETURN`, it parses the unspendable data payload.
- Both extract into the identical `Inscription { label: TokenLabel, sections: Vec<InscriptionSection> }` model.

### Rationale
- Fulfills **FR-005** and User Story 1 Acceptance Scenario 1.
- Equal priority ensures neither format is treated as a second-class citizen or deprecated variant.
- Encapsulates script parsing cleanly within the Shared Kernel so downstream consumers (Wallet, Server, Repository) do not have to worry about script opcode differences.

### Alternatives Considered
- **Only spendable envelopes**: Rejected by user requirement; OP_RETURN support is explicitly required.
- **Separate standalone parser modules**: Rejected because the internal protocol structure (`"name"` <label> [OP_NOP <protocol> <args>]*) is identical across both wrappers.

---

## Decision 3: Generic `TokenLabel` vs. Contextual RFC-1034 Enforcement

### Decision
Decouple the Name-Token label representation from DNS-specific RFC-1034 rules:
1. **Shared Kernel (`name_token_kernel`)**:
   - Implements `TokenLabel(Vec<u8>)` (or string-convertible bytes).
   - Validates only protocol-level constraints (non-empty, maximum size limit e.g. 255 bytes).
   - Does **not** enforce RFC-1034 (no restriction to lowercase ASCII, starting with a letter, etc.).
2. **DNS Bounded Context (`dns_nostr_server`)**:
   - `DnsNostrToken` conversion checks RFC-1034 compliance (lowercase `a-z`, digits `0-9`, hyphens, starting with a letter, length <= 63 chars).
   - Non-compliant labels produce `DnsNostrTokenError::InvalidDnsLabel` during conversion, cleanly preventing invalid DNS resolution without corrupting the on-chain Name-Token protocol.

### Rationale
- Fulfills **FR-008** and resolves architectural boundary leakage.
- The Name-Token protocol is a general-purpose naming layer on Bitcoin; subdomains for DNS are just one specific application (`section_protocol == "dns-nostr"`).
- Aligns with Domain-Driven Design (Principle VI): DNS constraints belong in the DNS bounded context, not in the core shared kernel.

### Alternatives Considered
- **Enforcing RFC-1034 in the Shared Kernel**: Rejected by user requirement. It would prevent non-DNS applications from using the Name-Token protocol with arbitrary identifiers.
- **Raw unvalidated `Vec<u8>` without a value object**: Rejected because wrapping in `TokenLabel` provides type safety, equality, ordering, and prevents primitive obsession.

---

## Decision 4: Deterministic Nostr Event Conflict Resolution

### Decision
When querying Nostr relays for DNS records published by a public key:
1. Fetch matching text note events (`kind: 1`) authored by the token's Nostr public key.
2. Sort candidate events by `(event.created_at, Reverse(event.id))`.
3. Deterministically select the single event with the highest `created_at` timestamp.
4. If two events have identical timestamps, select the one with the lexicographically smallest `event.id` (byte-wise comparison).

### Rationale
- Fulfills **FR-006** and resolves the specification edge case.
- Relays operate asynchronously and independently; deterministic tie-breaking ensures every server instance reaches identical DNS answers from the same set of relay events.
- Negligible computational overhead (`O(k log k)` where `k` is the small number of notes returned).

### Alternatives Considered
- **Treat duplicate timestamps as SERVFAIL / error**: Rejected because it creates denial-of-service vulnerabilities if a malicious or broken relay broadcasts a colliding timestamp.
- **Enforce NIP-16 / NIP-78 replaceable events**: While standard text notes (`kind: 1`) are currently used by the wallet, enforcing this rule at the resolution layer preserves compatibility with existing wallet notes while providing deterministic resolution.

---

## Decision 5: Cargo Workspace Crate Structure

### Decision
Extract the Shared Kernel into `crates/name_token_kernel` within the Cargo workspace.
- The root `Cargo.toml` will declare:
  ```toml
  [workspace]
  members = [
      "crates/name_token_kernel",
      "dns_nostr_server",
  ]
  ```
- `dns_nostr_server/Cargo.toml` will add:
  ```toml
  [dependencies]
  name_token_kernel = { path = "../crates/name_token_kernel" }
  ```
- `crates/name_token_kernel` contains the Core Domain (aggregates, value objects, domain events, parsing, validator domain service) with zero dependencies on `hickory-server` or database engines.

### Rationale
- Fulfills **FR-001** and aligns with Clean Architecture (Constitution Principle VII).
- Allows the future Wallet and Repository indexer to import `name_token_kernel` as a lightweight dependency without pulling in DNS server runtime code or Hickory DNS.

### Alternatives Considered
- **Internal module inside `dns_nostr_server`**: Rejected during clarification (User chose independent crate Option A).
