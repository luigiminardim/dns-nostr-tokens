# dns_nostr_token/src

## Overview
DNS-Nostr domain layer built on the generic `name_token` crate. It validates DNS labels (RFC-1034 style), converts a generic `NameToken` carrying a `dns-nostr` protocol section into a typed `DnsNostrToken`, and exposes a lookup service over `NameTokenService`.

## Interface
- [lib.rs](./lib.rs): declares `pub mod label`, `pub mod dns_nostr_token`, `pub mod dns_nostr_service`.
- [label.rs](./label.rs):
  - `Label` (newtype over `String`; derives `Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash`; implements `Display`).
  - `Label::new(&str) -> Result<Label, InvalidLabel>`, `Label::as_str`, `Label::as_bytes`.
  - `InvalidLabel`: `Empty`, `TooLong`, `InvalidStartCharacter`, `TrailingHyphen`, `InvalidCharacter`.
- [dns_nostr_token.rs](./dns_nostr_token.rs):
  - `DnsNostrToken { label: Label, nostr_pubkey: nostr_sdk::PublicKey }`.
  - `impl TryFrom<name_token::NameToken> for DnsNostrToken`.
  - `DnsNostrTokenFromNameTokenError`: `InvalidLabel(InvalidLabel)`, `MissingProtocolArgs`, `InvalidPublicKey`, `InvalidUtf8Label`.
- [dns_nostr_service.rs](./dns_nostr_service.rs):
  - `DnsNostrService::new(Arc<NameTokenService>)`.
  - `DnsNostrService::get_token(&self, &Label) -> Option<DnsNostrToken>` (async).

## Invariants
- A `Label` built via `Label::new` is non-empty, at most 63 bytes, starts with a lowercase ASCII letter, does not end with `-`, and otherwise contains only `a-z`, `0-9`, `-`. A first character that is not alphabetic yields `InvalidStartCharacter`; an uppercase first letter yields `InvalidCharacter`.
- The `Label` field is private, so a `Label` can only be obtained through `Label::new`.
- `DnsNostrToken::try_from` requires the label to be valid UTF-8 and a valid `Label`, a `dns-nostr` protocol section (`MissingProtocolArgs` otherwise), and the first argument of that section to parse with `PublicKey::from_slice` (`InvalidPublicKey` if absent or unparsable).
- `DnsNostrService::get_token` returns `None` both when no Name-Token exists for the label and when conversion to `DnsNostrToken` fails (the conversion error is discarded). Token validity/uniqueness rules are delegated to `name_token`.

## Side Effects
- None in this package itself; `DnsNostrService::get_token` awaits `NameTokenService::get_name_token`, which reads through the injected `NameTokenRepository` (storage defined by the caller).

## Verification
- Run: `cargo test -p dns_nostr_token`
