# Quickstart & Verification Guide: ddd-artifacts

**Feature**: [001-refactor-ddd]
**Spec**: [spec.md](spec.md) | **Data Model**: [data-model.md](data-model.md) | **Contracts**: [contracts/](contracts/)

This guide provides step-by-step instructions to run and verify the domain-driven design artifacts implemented in this feature.

## Prerequisites

- Rust stable toolchain (1.75+) with `cargo` installed.
- Repository root workspace with `crates/name_token_kernel` and `dns_nostr_server`.

## 1. Build Verification

Verify that the Cargo workspace and all member crates compile without warnings:

```bash
cargo check --workspace
```

Expected result: Zero compilation errors across both `name_token_kernel` and `dns_nostr_server`.

---

## 2. Unit Testing the Core Domain (Shared Kernel)

Run pure unit tests on the `name_token_kernel` crate in complete isolation from Bitcoin or Nostr network infrastructure:

```bash
cargo test -p name_token_kernel
```

### Verified Scenarios:
1. **Generic `TokenLabel` Representation (FR-008)**:
   - Accepts arbitrary valid byte sequences, lowercase/uppercase strings, and UTF-8 characters up to protocol limit (255 bytes).
   - Rejects empty byte arrays or labels exceeding 255 bytes.
   - Ensures no premature RFC-1034 constraints are enforced at the Name-Token layer.
2. **Equal-Priority Dual Inscription Parsing (FR-005)**:
   - Spendable envelope scripts (`OP_FALSE OP_IF ... OP_ENDIF <locking_script>`) parse correctly into `Inscription`.
   - Unspendable `OP_RETURN <push_data>` payload scripts parse correctly with equal priority into `Inscription`.
   - Corrupted or non-protocol scripts return `Ok(None)` or typed parse errors.
3. **Event-Sourced `NameToken` Aggregate Lifecycle (FR-007)**:
   - `NameTokenMinted` initializes active state with initial outpoint.
   - `NameTokenTransferred` updates current outpoint.
   - `NameTokenRevoked` burns the token.
   - Any further transfer or mutation attempt on a revoked token fails with `DomainError::TokenAlreadyRevoked`.
4. **First-is-Root Ordering (FR-003)**:
   - Competing tokens for identical labels are evaluated by `NameTokenValidator`.
   - The token with the earliest full position tuple `(block_height, block_index, vout)` is deterministically selected as canonical root.

---

## 3. Integration Testing the DNS Context

Run the DNS server tests to verify integration with the Shared Kernel and Nostr event conflict resolution:

```bash
cargo test -p dns_nostr_server
```

### Verified Scenarios:
1. **RFC-1034 Compliance on `DnsNostrToken` Conversion (FR-008 & Principle IV)**:
   - Successfully converts a `NameToken` with an RFC-1034 compliant label (lowercase, 1-63 chars, starts with letter).
   - Rejects `NameToken` instances whose `TokenLabel` violates RFC-1034 (uppercase letters, starting with number/hyphen, ending with hyphen, >63 chars) with `DnsNostrTokenError::InvalidDnsLabel`.
   - Rejects revoked tokens or tokens without a valid `dns-nostr` inscription section.
2. **Deterministic Nostr Conflict Resolution (FR-006)**:
   - Given multiple mock events with different timestamps, selects the newest `created_at`.
   - Given multiple mock events with identical timestamps, deterministically selects the lowest `event.id`.
3. **Authority Lookup**:
   - Queries for delegated subdomain labels resolve to parsed Nostr zone records.
