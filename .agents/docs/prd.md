# Product Requirements Document (PRD)

## Title & Status
**Title:** Extract `dns_nostr_token` Domain Crate
**Status:** Draft

## Problem Statement & Motivation
Currently, the `dns-nostr-token` domain logic—which interprets Name-Token inscriptions to extract Nostr public keys—is entangled within the `dns_nostr_server` crate. Because there are upcoming plans to build new applications, such as a DNS-Nostr Wallet, leaving this core domain logic embedded in the server crate creates improper coupling. We need a proactive structural cleanup to decouple the DNS-Nostr domain logic from the server infrastructure.

## Target Audience
- Developers and maintainers of the `dns-nostr-tokens` repository.
- Future downstream applications (like the DNS-Nostr Wallet).

## Scope & Out of Scope
**In Scope:**
- Creating a new `dns_nostr_token` library crate in the workspace.
- Moving `DnsNostrToken` into this new crate as the core aggregate/entity.
- Creating a `DnsNostrService` inside the new crate that depends on `name_token::NameTokenService` to fetch and translate tokens.
- Refactoring `dns_nostr_server` to depend on the new `dns_nostr_token` crate instead of housing the logic itself.
- Updating workspace documentation (`AGENTS.md`, `Cargo.toml`, etc.) to reflect the new architecture.

**Out of Scope:**
- Creating a dedicated repository trait/port for `dns_nostr_token` (it will rely purely on `NameTokenService`).
- Implementing the DNS-Nostr Wallet or any net-new application features.
- Changing the existing First-is-Root selection logic in `name_token`.

## User Stories & Requirements
1. **Developer Experience:** As a developer, I want to import `dns_nostr_token` in a new application (like a wallet) without being forced to pull in `dns_nostr_server` dependencies like DNS handling (`hickory-server`) or Nostr relay interaction logic.
2. **Technical Requirement 1:** The `dns_nostr_token` crate must define a `DnsNostrService` that takes a reference to `NameTokenService` (or similar) to fetch tokens.
3. **Technical Requirement 2:** The system must continue to compile and pass all tests (`cargo test --workspace`) without functionality changes from the perspective of the DNS server.

## Success Metrics (KPIs)
- **Zero infrastructure dependencies** inside the new `dns_nostr_token` crate.
- **100% pass rate** for the existing test suite (`cargo test --workspace`).
- The `dns_nostr_server` successfully delegates token extraction to the new crate.

## Technical Impact & Codebase Survey
- **New Crate:** `dns_nostr_token` must be added to the Cargo workspace.
- **`dns_nostr_server/src/dns_nostr_token.rs` & `dns_nostr_token_repository.rs`**: These files will be removed/refactored, moving their core logic into the new crate.
- **`dns_nostr_server/Cargo.toml`**: Will need a dependency on `dns_nostr_token`.
- **`AGENTS.md`**: Must be updated to document the new crate in the Repository Structure and Architecture sections.
