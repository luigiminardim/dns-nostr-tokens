# Contract: Name-Token Shared Kernel Public API

**Crate**: `name_token_kernel` (`crates/name_token_kernel`)

## Overview

The `name_token_kernel` crate defines the core domain entities, value objects, domain events, parsing logic, and validation rules for the Name-Token protocol.

## Public Module Hierarchy

```rust
pub mod model {
    pub mod token_label;
    pub mod position;
    pub mod inscription;
    pub mod events;
    pub mod name_token;
}

pub mod parsing {
    pub mod inscription;
}

pub mod validation {
    pub mod validator;
}
```

## Types & Traits

### 1. TokenLabel (`model::token_label`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct TokenLabel(Vec<u8>);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TokenLabelValidationError {
    #[error("Token label cannot be empty")]
    Empty,
    #[error("Token label length {0} exceeds protocol maximum {1}")]
    TooLong(usize, usize),
}

impl TokenLabel {
    pub const MAX_LABEL_BYTES: usize = 255;

    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, TokenLabelValidationError>;
    pub fn as_bytes(&self) -> &[u8];
    pub fn as_utf8(&self) -> Result<&str, std::str::Utf8Error>;
}

impl std::fmt::Display for TokenLabel { ... }
impl AsRef<[u8]> for TokenLabel { ... }
```

### 2. Inscription & Script Parser (`parsing::inscription`)

```rust
pub struct InscriptionParser;

impl InscriptionParser {
    /// Extracts an inscription with equal priority from:
    /// 1. A spendable transaction output (prefixed with OP_FALSE OP_IF ... OP_ENDIF)
    /// 2. An unspendable OP_RETURN script output
    pub fn parse_txout(txout: &bitcoin::TxOut) -> Result<Option<Inscription>, ParseError>;
    
    /// Parses an inscription directly from script instructions.
    pub fn parse_script(script: &bitcoin::Script) -> Result<Option<Inscription>, ParseError>;
}
```

### 3. Events & Aggregate Root (`model::name_token`, `model::events`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenStatus {
    Active { current_outpoint: bitcoin::OutPoint, current_script: bitcoin::ScriptBuf },
    Revoked { burned_by_position: BlockchainPosition, spending_txid: bitcoin::Txid },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameToken {
    label: TokenLabel,
    mint_position: BlockchainPosition,
    status: TokenStatus,
    inscription: Inscription,
}

impl NameToken {
    /// Reconstruct aggregate from historical domain events
    pub fn from_events(events: impl IntoIterator<Item = NameTokenDomainEvent>) -> Result<Self, DomainError>;
    
    /// Apply an incoming domain event, updating state or returning an error if invalid
    pub fn apply(&mut self, event: NameTokenDomainEvent) -> Result<(), DomainError>;
    
    pub fn label(&self) -> &TokenLabel;
    pub fn mint_position(&self) -> &BlockchainPosition;
    pub fn status(&self) -> &TokenStatus;
    pub fn is_active(&self) -> bool;
    pub fn current_outpoint(&self) -> Option<&bitcoin::OutPoint>;
    pub fn inscription(&self) -> &Inscription;
}
```

### 4. Domain Service (`validation::validator`)

```rust
pub struct NameTokenValidator;

impl NameTokenValidator {
    pub fn new() -> Self;
    
    /// Determines the first confirmed token among competitors for the same label
    /// by comparing the full position tuple (blockheight, blockindex, vout).
    pub fn select_first_confirmed<'a>(&self, candidates: &'a [NameToken]) -> Option<&'a NameToken>;
}
```
