# Product Requirements Document (PRD)

## Title & Status
**Title:** Refactor `NameTokenService` to use Dynamic Dispatch
**Status:** Draft

## Problem Statement & Motivation
Currently, `NameTokenService` uses a generic type parameter (`<R: NameTokenRepository>`) to define its persistence dependency. In Rust, generics are "viral"—any struct that contains `NameTokenService` must also become generic over `R`. This propagates all the way up to the composition root (e.g., `main.rs`, `BlockchainWatcher`, etc.), leading to noisy code signatures, complex bounds, and a rigid architecture that is difficult to compose or test dynamically.

By moving from static dispatch (generics) to dynamic dispatch (trait objects), we can encapsulate the repository dependency and dramatically simplify the service's signature and the signatures of all downstream consumers.

## Target Audience
- Developers and maintainers of the `dns-nostr-tokens` repository.

## Scope & Out of Scope
**In Scope:**
- Updating `NameTokenService` struct definition to remove the generic parameter `R` and instead store a `Box<dyn NameTokenRepository>`.
- Updating `NameTokenService::new` to accept and box the repository.
- Updating tests in `name_token_service.rs` to reflect the new API.
- Updating consuming modules (e.g., in `dns_nostr_server`) that currently depend on `NameTokenService<R>` to depend on the simplified `NameTokenService`.

**Out of Scope:**
- Changing the `NameTokenRepository` trait definition (aside from ensuring it remains object-safe, which it should already be via `#[async_trait]`).
- Altering any domain logic or First-is-Root selection rules.
- Introducing new features or database implementations.

## User Stories & Requirements
1. **Developer Experience:** As a developer, I want to instantiate and pass around `NameTokenService` without having to define generic bounds `<R>` on every struct that holds it.
2. **Technical Requirement 1:** `NameTokenService` must internally hold `Box<dyn NameTokenRepository>` (or similar dynamic reference).
3. **Technical Requirement 2:** The system must continue to compile and pass all tests without functionality changes.

## Success Metrics (KPIs)
- **Zero generic parameters** on `NameTokenService`.
- **100% pass rate** for the existing test suite (`cargo test --workspace`).
- Reduction in generic bounds boilerplate across the `dns_nostr_server` crate.

## Technical Impact & Codebase Survey
- **`name_token/src/name_token_service.rs`**: Core struct definition and implementation block must drop the `<R>` parameter.
- **`dns_nostr_server/src/blockchain_watcher.rs`** (and potentially other consumers): Will need type signatures simplified to drop `<R>`.
- **Performance:** Dynamic dispatch introduces a negligible vtable lookup cost on repository calls, which is perfectly acceptable here since repository calls are asynchronous database/network I/O operations anyway.
