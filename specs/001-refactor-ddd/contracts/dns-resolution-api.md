# Contract: DNS Bounded Context & Nostr Resolution Contract

**Context**: `dns_nostr_server`

## Overview

Defines the contract for the DNS-Nostr Server bounded context, integrating the `name_token_kernel` Shared Kernel and Nostr Relays to provide authoritative DNS responses.

## Types & Interfaces

### 1. `DnsNostrToken` Domain Entity

```rust
use name_token_kernel::model::name_token::NameToken;
use hickory_server::proto::rr::domain::Label as DnsLabel;
use nostr_sdk::PublicKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsNostrToken {
    pub label: DnsLabel,
    pub nostr_pubkey: PublicKey,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DnsNostrTokenError {
    #[error("Token label is not RFC-1034 compliant for DNS resolution: {0}")]
    InvalidDnsLabel(String),
    #[error("Missing dns-nostr protocol section in inscription")]
    MissingProtocolSection,
    #[error("Invalid Nostr public key in inscription arguments")]
    InvalidPublicKey,
    #[error("Token is revoked")]
    TokenRevoked,
}

impl TryFrom<&NameToken> for DnsNostrToken {
    type Error = DnsNostrTokenError;
    
    /// Converts a generic NameToken into a DNS-specific entity.
    /// Strictly validates that token.label() satisfies RFC-1034 (FR-008 & Principle IV).
    fn try_from(token: &NameToken) -> Result<Self, Self::Error>;
}
```

### 2. Nostr Event Conflict Resolver

```rust
use nostr_sdk::Event;

pub struct NostrConflictResolver;

impl NostrConflictResolver {
    /// Selects the winning event from candidate relay events by newest created_at,
    /// breaking ties by lowest event ID hash (FR-006).
    pub fn resolve_latest(events: &[Event]) -> Option<&Event> {
        events.iter().max_by(|a, b| {
            a.created_at.cmp(&b.created_at)
                .then_with(|| b.id.cmp(&a.id)) // lowest ID wins tie-breaker
        })
    }
}
```

### 3. Asynchronous Resolution Port / Gateway

```rust
use hickory_server::proto::rr::domain::Label as DnsLabel;

#[async_trait::async_trait]
pub trait DnsNostrTokenRepository: Send + Sync {
    async fn get_token(&self, label: &DnsLabel) -> Result<Option<DnsNostrToken>, RepositoryError>;
}

#[async_trait::async_trait]
pub trait NostrEventFetcher: Send + Sync {
    async fn fetch_latest_zone_note(&self, pubkey: PublicKey) -> Result<Option<String>, FetchError>;
}
```
