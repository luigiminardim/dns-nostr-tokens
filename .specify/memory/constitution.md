<!--
SYNC IMPACT REPORT (remove before committing)
=============================================
Version change: 1.0.0 → 1.1.0 (MINOR: three new Engineering Practice principles added)
Added sections:
  - Engineering Practices (new section with three principles)
    · VI. Domain-Driven Design is the Primary Modeling Approach
    · VII. Clean Architecture Governs All Structural Decisions
    · VIII. Clean Code is the Baseline Standard for All Code
Modified principles: none
Removed sections: none
Compliance Review line updated to cover Principles I–VIII.
Follow-up TODOs: none — all three principles fully specified via installed skills.
-->

# DNS-Nostr Tokens Constitution

## Core Principles

### I. Protocol Correctness is Non-Negotiable

The Name-Token protocol specification (inscription format, uniqueness rules, update/revocation
semantics, positional update correlation) MUST be faithfully implemented. Any deviation from the
spec — even one that "works" empirically — is a defect. Protocol logic MUST be tested with
deterministic Bitcoin UTXO fixtures before any integration with a live node.

**Rationale**: The blockchain is immutable; incorrect inscriptions cannot be undone. Protocol
bugs permanently corrupt on-chain state and break all downstream consumers (wallets, servers,
DNS resolvers).

### II. UTXO-Set as Single Source of Truth

All Name-Token state MUST be derived exclusively from the active Bitcoin UTXO set. No
application-level database, cache, or secondary index is authoritative. Caches and indexes are
read-optimisation layers only, MUST be clearly labelled as derived, and MUST be re-buildable
from scratch by replaying the UTXO set at any time.

**Rationale**: Relying on off-chain state introduces consistency gaps. The protocol's security
model depends on the UTXO set being the canonical record of ownership.

### III. Strict Name Uniqueness and First-Confirmed Rule

Code that writes or validates Name-Tokens MUST enforce global name uniqueness across the live
UTXO set. The First-Confirmed Rule MUST be the sole conflict-resolution mechanism; no
application-layer tie-breaking is permitted. Duplicate-detection logic MUST be covered by tests
that exercise race conditions (same name, multiple blocks).

**Rationale**: Allowing two valid Name-Tokens for the same label would break DNS resolution
determinism and undermine ownership guarantees.

### IV. DNS RFC-1034 Compliance for Labels

All label values stored in DNS-Nostr tokens MUST conform to RFC-1034: lowercase ASCII letters
(a–z), digits (0–9), and hyphens only; MUST start with a letter; MUST NOT exceed 63 characters.
Label validation MUST happen at creation time (wallet) and at read time (server) independently.

**Rationale**: Non-compliant labels silently corrupt DNS responses and interoperate poorly with
standard resolvers. Double-validation prevents one component's bugs from poisoning the other.

### V. Security-First for Key Material

Nostr private keys and Bitcoin wallet keys MUST never be logged, serialised to disk unencrypted,
or included in error messages. All cryptographic operations MUST use audited libraries from the
Rust ecosystem (bitcoin, secp256k1, nostr). Introducing a new cryptographic dependency requires
explicit justification and review.

**Rationale**: Key compromise is irreversible on both Bitcoin and Nostr. Operational security
failures undermine user trust in the entire protocol.

## Engineering Practices

### VI. Domain-Driven Design is the Primary Modeling Approach

**Modeling time is never wasted.** It is preferable to invest time in correct domain modeling
than to start implementing prematurely and reshape the code later. The `domain-driven-design`
skill MUST be consulted for all modeling decisions.

Non-negotiable rules:
- **Ubiquitous Language**: Every class, method, module, and domain event name MUST use terms
  the domain experts (Bitcoin/Nostr/DNS operators) would recognise. Technical jargon
  (`DataManager`, `Processor`) is rejected at code review.
- **Bounded Contexts**: The DNS-Nostr Wallet and DNS-Nostr Server are distinct bounded contexts.
  Shared models MUST be governed via an explicit context map. Anti-Corruption Layers are REQUIRED
  at every external integration (Bitcoin RPC, Nostr relay, external DNS).
- **Aggregates and Value Objects**: Prefer immutable Value Objects. Keep Aggregates small; the
  Aggregate Root enforces all invariants. Reference other Aggregates by ID only.
- **Domain Events**: Cross-aggregate communication MUST use named past-tense domain events
  (e.g., `NameTokenInscribed`, `DnsRecordPublished`). Internal events stay within their bounded
  context; integration events cross the Bitcoin/Nostr boundary.
- **Core Domain**: The Name-Token inscription/validation logic is the Core Domain — it receives
  the deepest modeling effort. Bitcoin RPC, Nostr relay integration, and DNS resolution are
  Supporting/Generic Subdomains.
- **Strategic Design First**: Before implementing any feature, classify its subdomain, draw or
  update the context map, and identify which aggregates and domain events are involved.

**Skill**: `domain-driven-design` (`.agents/skills/domain-driven-design/SKILL.md`)

### VII. Clean Architecture Governs All Structural Decisions

**Source code dependencies MUST point inward — toward higher-level policies.** The Dependency
Rule is inviolable. Frameworks, databases, and delivery mechanisms are details; they MUST reside
in the outermost circle and depend on inner circles, never the reverse.

Non-negotiable rules:
- **Dependency Rule**: Inner circles (Entities, Use Cases) MUST NOT import anything from outer
  circles (Interface Adapters, Frameworks/Drivers). Violations are rejected at code review.
- **Layer Structure**: The Cargo workspace MUST reflect the concentric-circle model. Domain
  entities and use-case logic live in inner crates; Bitcoin RPC, Nostr, and DNS adapters live
  in outer crates.
- **Interfaces Defined Inward**: Repository and gateway interfaces are defined in the Use Case
  layer and implemented in the Infrastructure/Adapter layer.
- **DTOs at Boundaries**: No ORM models, framework request/response types, or Bitcoin/Nostr SDK
  structs cross a layer boundary. Plain Rust structs (DTOs) are used at boundary crossings.
- **Testability Without Infrastructure**: Business rules (entities + use cases) MUST be fully
  testable without a running Bitcoin node, Nostr relay, or DNS server. Mock implementations of
  gateway interfaces provide this isolation.
- **Main as Composition Root**: All concrete dependency wiring happens in the binary entry point
  (`main.rs`). Inner modules never instantiate their own infrastructure dependencies.
- **SOLID**: All five SOLID principles apply to every module and struct. SRP violations and
  upward dependency inversions are treated as architecture defects.

**Skill**: `clean-architecture` (`.agents/skills/clean-architecture/SKILL.md`)

### VIII. Clean Code is the Baseline Standard for All Code

**Code is read far more often than it is written — optimize for the reader.** Every commit MUST
leave the codebase at least as readable as it was found (Boy Scout Rule). The `clean-code` skill
MUST be applied when writing new code and MUST be applied when refactoring existing code.

Non-negotiable rules:
- **Naming**: Function, struct, and variable names MUST reveal intent without a comment. Single
  letters are allowed only for trivial loop counters. `unwrap()` / `expect()` are banned in
  non-test code (see Principle V).
- **Functions**: Functions MUST be small (target ≤ 20 lines), do one thing, and operate at a
  single level of abstraction. Flag arguments and output arguments are banned.
- **Comments**: Comments explain *why*, not *what*. Commented-out code is deleted; version
  control preserves history. TODOs MUST reference an issue or be resolved within the same PR.
- **Error Handling**: `Result` and typed errors are used everywhere; no silent `unwrap()` in
  library or server code; no generic catch-all error types in public APIs.
- **Unit Tests**: Tests are first-class code following the same quality bar. Test names describe
  behavior: `should_reject_label_exceeding_63_chars`. F.I.R.S.T. properties MUST hold.
- **Refactoring**: Refactoring tasks are performed in isolation from feature work — never mix
  refactoring and new behavior in the same commit. Characterization tests are written first.
- **Code Smells**: Duplication above three instances, magic numbers, feature envy, god structs,
  and dead code are treated as defects and MUST be flagged in code review.

**Skill**: `clean-code` (`.agents/skills/clean-code/SKILL.md`)

## Architecture & Technology Constraints

- **Language**: Rust (stable toolchain). No `unsafe` blocks without explicit documented justification.
- **Workspace**: Cargo workspace rooted at the repository root; DNS-Nostr Server lives in
  `dns_nostr_server/`. New protocol components MUST be added as separate workspace crates whose
  boundaries reflect the Clean Architecture layers defined in Principle VII.
- **Bitcoin Integration**: The server MUST support connecting to a local Bitcoin node via RPC.
  Regtest support is required for automated integration tests (`regtest_node/` fixture).
- **Nostr Integration**: DNS record publishing and fetching MUST use the Nostr relay protocol.
  Relay URL MUST be configurable at runtime; no hardcoded relay endpoints in production code.
- **DNS Record Types**: The server MUST support A, AAAA, CNAME, NS, MX, and TXT record types.
  Adding a new record type MUST include a corresponding integration test.
- **No Silent Failures**: All I/O errors (Bitcoin RPC, Nostr relay, DNS) MUST surface as typed
  errors. `unwrap()` / `expect()` are forbidden in library and server code paths; permitted only
  in test code with an explanatory comment.

## Development Workflow

- **Model Before Implement**: Every feature begins with DDD modeling (Principle VI): identify
  the subdomain, update the context map, name the aggregates and domain events. Implementation
  starts only after the model is stable.
- **Test-First**: For any protocol logic change, tests MUST be written and reviewed before
  implementation begins. The Red-Green-Refactor cycle is enforced.
- **Integration Tests**: Any feature touching Bitcoin UTXO scanning, Nostr record
  publishing/fetching, or DNS resolution MUST include an integration test using the regtest
  fixture or a mock relay.
- **Code Review**: Reviewers MUST verify compliance with Principles I–VIII. Changes to protocol
  logic, cryptographic handling, architecture boundaries, or governance documents MUST be
  reviewed by at least one other contributor before merging.
- **Versioning**: The project uses Semantic Versioning (MAJOR.MINOR.PATCH). Protocol-breaking
  changes (inscription format changes) increment MAJOR. New capabilities increment MINOR.
  Bug fixes and non-breaking improvements increment PATCH.
- **Commit Messages**: Use Conventional Commits format (feat:, fix:, docs:, test:, chore:,
  refactor:). Constitution amendments use `docs: amend constitution to vX.Y.Z`.
  Refactoring commits use `refactor:` and MUST NOT include feature changes.

## Governance

This constitution supersedes all informal decisions, ad-hoc conventions, and prior undocumented
practices. It is the highest-priority reference document for the DNS-Nostr Tokens project.

**Amendment Procedure**:
1. Propose the change via a pull request that modifies `.specify/memory/constitution.md`.
2. Include a Sync Impact Report (as an HTML comment at the top) describing affected principles,
   version bump rationale, and any follow-up TODOs.
3. Obtain review and approval from at least one other contributor.
4. Increment `CONSTITUTION_VERSION` according to the versioning rules above.
5. Update `LAST_AMENDED_DATE` to the merge date.
6. Remove the Sync Impact Report comment block before merging.

**Compliance Review**: Protocol implementation PRs MUST verify compliance with Principles I–VIII.
Reviewers are expected to reject code that violates this constitution regardless of other merits.

**Version**: 1.1.0 | **Ratified**: 2026-09-29 | **Last Amended**: 2026-09-29
