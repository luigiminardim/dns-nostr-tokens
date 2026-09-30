# Feature Specification: ddd-artifacts

**Feature Branch**: `[001-refactor-ddd]`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "Start implementing domain-driven design artificacts in the project. I want to define entities, services, an unique umbiquos language. Use the @[report.md] as base to understand the desired state of the project. Grill-me for questions."

## Clarifications

### Session 2026-09-30

- Q: How should the Name-Token Shared Kernel be physically structured within the Cargo workspace? (FR-001) → A: An independent workspace crate (e.g., `crates/name_token_kernel`) will be created and imported by `dns_nostr_server`.
- Q: Which script format should the Name-Token parser validate and extract inscriptions from in User Story 1? (User Story 1) → A: Both spendable inscription envelopes (`OP_FALSE OP_IF ... OP_ENDIF`) and unspendable `OP_RETURN` payload scripts with equal priority.
- Q: How should the DNS resolution context deterministically resolve conflicting Nostr events returned for the same public key? (Edge Case) → A: Select the event with the newest created_at timestamp, breaking ties by lowest event ID hash.
- Q: How should the NameToken aggregate root model its lifecycle states and enforce state transition invariants? (Key Entities: NameToken) → A: Event-sourced entity reconstructed from domain events (Minted, Transferred, Revoked).
- Q: When two NameTokens for the same label are confirmed in the same block, how does NameTokenValidator determine the First-is-Root winner? (FR-003) → A: Order by full blockchain position tuple: blockheight, then blockindex (tx index), then vout (output index) per report.md.
- Q: How should the Name-Token label be modeled in the Shared Kernel vs. the DNS Context? (Key Entities: Label) → A: Model as a generic byte-based TokenLabel in the Shared Kernel, enforcing RFC-1034 strictly in the DNS context (DnsNostrToken).
- Q: What script format should the Name-Token inscription parser support as its specification? (FR-005) → A: Require both spendable envelopes (OP_FALSE OP_IF ... OP_ENDIF) and OP_RETURN scripts with equal priority.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Shared Kernel for Name-Token (Priority: P1)

As a developer, I want a shared kernel that handles the Name-Token protocol logic (parsing, UTXO rules, First-is-Root uniqueness) so that both the Wallet and the Repository components can rely on a single, unified source of truth for domain rules.

**Why this priority**: The Name-Token logic is the Core Domain and must be consistent across all components reading or writing to the blockchain.

**Independent Test**: The Name-Token domain logic can be fully tested in isolation using pure unit tests with mocked UTXO inputs.

**Acceptance Scenarios**:

1. **Given** a transaction output containing either a spendable inscription envelope (`OP_FALSE OP_IF ... OP_ENDIF`) or an `OP_RETURN` payload script, **When** parsed, **Then** it correctly maps to the `NameToken` entity with a byte-based `TokenLabel`.
2. **Given** two NameTokens for the same label, **When** validated by the `NameTokenValidator`, **Then** the winner is deterministically selected using the full blockchain position tuple: earliest `blockheight`, then earliest `blockindex` (transaction index within block), then earliest `vout` (output index).

---

### User Story 2 - Separate Context for DNS Resolution (Priority: P2)

As a developer, I want the DNS-Nostr Server logic to be modeled as a distinct bounded context that interacts with the Name-Token shared kernel and external Nostr Relays to resolve DNS queries.

**Why this priority**: DNS resolution is the main product but it builds upon the foundational Name-Token protocol.

**Independent Test**: The DNS resolution logic can be independently tested by mocking the Repository (for Name-Tokens) and the Nostr Relay network.

**Acceptance Scenarios**:

1. **Given** a DNS query for a valid label, **When** resolved, **Then** the server queries the Name-Token shared kernel, fetches the Nostr event, and returns the DNS record.

### Edge Cases

- What happens when the Bitcoin and Nostr library structures change? (Direct dependency was requested, increasing coupling).
- **Conflicting Nostr events**: When multiple relays return different text note events for the same public key, the DNS context selects the event with the newest `created_at` timestamp. If timestamps are identical, it deterministically breaks ties by choosing the lexicographically lowest event ID hash.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The codebase MUST implement a Shared Kernel for the "Name-Token" logic as an independent Cargo workspace crate (e.g., `crates/name_token_kernel`), shared between the Wallet and Repository components and imported by `dns_nostr_server`.
- **FR-002**: The codebase MUST implement a distinct Bounded Context for the DNS-Nostr Server logic.
- **FR-003**: The codebase MUST implement a `NameTokenValidator` domain service to enforce the "First-is-Root" validation logic by evaluating the full blockchain position tuple `(blockheight, blockindex, vout)` across candidate `NameToken` entities.
- **FR-004**: The domain model MUST directly use Bitcoin and Nostr library structures to avoid mapping overhead, as explicitly requested by the user. **Note: This explicitly conflicts with Principle VII of the Constitution (DTOs at Boundaries), but is recorded as a direct user requirement for this implementation.**
- **FR-005**: The Name-Token parser MUST support extracting inscriptions from both spendable outputs (prefixed with `OP_FALSE OP_IF ... OP_ENDIF`) and unspendable `OP_RETURN` script outputs with equal priority.
- **FR-006**: When resolving DNS records from Nostr, the DNS context MUST select the event with the newest `created_at` timestamp, breaking ties by selecting the lexicographically lowest event ID.
- **FR-007**: The `NameToken` aggregate root MUST be event-sourced, reconstructed from domain events (`NameTokenMinted`, `NameTokenTransferred`, `NameTokenRevoked`) and enforcing transition invariants (e.g., rejecting operations once revoked).
- **FR-008**: The Shared Kernel MUST treat the Name-Token label as a generic byte-based `TokenLabel` without RFC-1034 restrictions; RFC-1034 compliance MUST be enforced strictly within the DNS Bounded Context during DNS token conversion (`DnsNostrToken`) and query processing.

### Key Entities

- **NameToken**: (Shared Kernel) Event-sourced aggregate root representing a domain label registration inscribed on the Bitcoin blockchain, reconstructed from lifecycle domain events (`NameTokenMinted`, `NameTokenTransferred`, `NameTokenRevoked`).
- **Domain Events**: (Shared Kernel) Past-tense events (`NameTokenMinted`, `NameTokenTransferred`, `NameTokenRevoked`) carrying state transition facts.
- **TokenLabel**: (Shared Kernel) Generic byte-based Value Object representing an arbitrary name identifier on the Bitcoin blockchain (not restricted to RFC-1034).
- **DnsNostrToken**: (DNS Context) Entity representing a NameToken converted for DNS usage, enforcing RFC-1034 label validity.
- **DnsRecord**: (DNS Context) Value object representing a resolved DNS record from Nostr.
- **NameTokenValidator**: (Shared Kernel) Domain service responsible for enforcing the "First-is-Root" uniqueness rule.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Domain entities (`NameToken`, `TokenLabel`) are fully decoupled from database and framework code, and contain business logic rather than just being data bags.
- **SC-002**: 100% of the core Name-Token validation rules are covered by pure unit tests.
- **SC-003**: A Context Map is clearly definable showing the Shared Kernel and the separate DNS context.

## Assumptions

- The user has accepted the architectural trade-off of directly using Bitcoin and Nostr library structures in the domain, despite the project constitution's guidelines on Clean Architecture.
- Nostr and Bitcoin libraries are available and stable enough to act as direct domain dependencies.
