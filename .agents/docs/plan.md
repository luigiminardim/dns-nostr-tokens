# Plan: Extract `dns_nostr_token` Domain Crate

## Clean Architecture Sweep
- **Entity Layer:** `DnsNostrToken` represents the core domain aggregate for this context. While libraries like `hickory-proto`, `rust-bitcoin`, and `bdk*` are considered domain libraries and are permitted here, we will still introduce a custom `dns_nostr_token::Label` struct. This is because `hickory-proto`'s label parser does not natively enforce the strict RFC-1034 validation rules (starts with a letter, lowercase only, digits, hyphens, max 63 chars) required by the Name-Token protocol.
- **Use Case Layer:** `DnsNostrService` orchestrated the retrieval of NameTokens and converting them to DnsNostrTokens. It will live in the new crate and depend on `NameTokenService`.
- **Adapter Layer:** `dns_nostr_server` is the outer infrastructure. Its `NostrAuthority` adapter will depend on `DnsNostrService` and map its DNS (`hickory-server`) types to the core `name_token::Label` type before making the call.

## Slices

### Slice 1: Create `dns_nostr_token` Crate
**Outcome:** A new `dns_nostr_token` crate is created and added to the workspace, with correct dependencies.
- **Check 1:** The `dns_nostr_token` directory exists with a `Cargo.toml`.
  - *Proof:* `cargo tree -p dns_nostr_token` succeeds.
- **Check 2:** `dns_nostr_token` has NO infrastructure dependencies (it must not depend on `hickory-server`, though `hickory-proto` is allowed as a domain library).
  - *Proof:* `cargo tree -p dns_nostr_token | grep hickory-server` returns no results.

### Slice 2: Implement Domain Entity (`DnsNostrToken` and `Label`)
**Outcome:** The `DnsNostrToken` and its specific `Label` value object are implemented in the new crate, enforcing protocol rules.
- **Check 1:** `dns_nostr_token::Label` correctly validates strings according to RFC-1034 (starts with letter, no trailing hyphen, lowercase, max 63 chars).
  - *Proof:* `cargo test -p dns_nostr_token` passes, with tests covering valid and invalid labels.
- **Check 2:** `DnsNostrToken` successfully converts from `NameToken` natives, returning proper validation errors on invalid labels.
  - *Proof:* `cargo test -p dns_nostr_token` passes, including a test for `TryFrom<NameToken>`.

### Slice 3: Implement Use Case (`DnsNostrService`)
**Outcome:** `DnsNostrService` is created in the new crate to replace the old repository.
- **Check 1:** `DnsNostrService` has an async `get_token` method taking a `name_token::Label` (to search by) and returning a valid `DnsNostrToken`.
  - *Proof:* `cargo test -p dns_nostr_token` passes.

### Slice 4: Refactor `dns_nostr_server` Infrastructure
**Outcome:** The `dns_nostr_server` uses the new crate and compiles successfully.
- **Check 1:** `dns_nostr_token.rs` and `dns_nostr_token_repository.rs` are deleted from `dns_nostr_server`.
  - *Proof:* `ls dns_nostr_server/src/dns_nostr_token.rs` fails.
- **Check 2:** `nostr_authority.rs` defines its own `GetDnsNostrToken` interface (or consumes `DnsNostrService` directly) and queries the service using `name_token::Label`.
  - *Proof:* `cargo check -p dns_nostr_server` succeeds.
- **Check 3:** `main.rs` is updated to inject `DnsNostrService`.
  - *Proof:* `cargo build --workspace` succeeds.
- **Check 4:** Tests for `dns_nostr_server` still pass.
  - *Proof:* `cargo test -p dns_nostr_server` passes.

### Slice 5: Documentation & Cleanup
**Outcome:** Documentation reflects the new crate.
- **Check 1:** `AGENTS.md` is updated with the new `dns_nostr_token` crate under the Repository Structure.
  - *Proof:* `grep "dns_nostr_token" AGENTS.md` returns results.
- **Check 2:** All workspace tests pass and the temporary plan is cleaned up.
  - *Proof:* `cargo test --workspace` passes.
