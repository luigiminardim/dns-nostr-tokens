# name_token (src)

## Overview
Core Name-Token domain library. Parses Name-Token inscriptions from Bitcoin
`TxOut` scripts, models token lifecycles (mint / update / revoke), and applies
Bitcoin blocks to a persistence port, handling chain reorganizations. It has no
infrastructure dependencies: storage is abstracted by `NameTokenRepository`.
`src/bin/` exists but is empty.

## Interface
- [`lib.rs`](./lib.rs): declares modules and re-exports `Bytes`, `Inscription`, `InscriptionSection`, `NameToken`, `NameTokenEvent`, `UpdateNameTokenError`, `NameTokenRepository`, `NameTokenService`, `Label`, `Position`, `InMemoryNameTokenRepository`.
- [`label.rs`](./label.rs): `Label(Vec<u8>)` newtype (`new`, `AsRef<[u8]>`, `From<Vec<u8>>`, `From<&[u8]>`, `PartialEq<[u8]>`, `PartialEq<&[u8]>`); derives `Ord`, `Serialize`, `Deserialize`.
- [`position.rs`](./position.rs): `Position::new(BlockId, blockindex, OutPoint)` with getters `block_id`, `block_height`, `block_hash`, `blockindex`, `outpoint`, `txid`, `vout`. Derives `Ord`.
- [`inscription.rs`](./inscription.rs): `type Bytes = Vec<u8>`, `InscriptionSection { protocol, arguments }`, `Inscription { label, sections }`, `Inscription::from_txout(&TxOut) -> Option<Inscription>`.
- [`name_token.rs`](./name_token.rs): `NameTokenEvent::{Minted, Updated, Revoked}` (+ `position()`), `NameToken` (`new`, `label`, `events`, `first_position`, `last_position`, `last_outpoint`, `inscription`, `is_revoked`, `has_minted`, `protocol_args`, `update`, `revoke`, `process_same_index_chain`, `select_root_token`, `undo_block_events`), `UpdateNameTokenError::{LabelMismatch, Revoked, StaleInscription}`.
- [`name_token_repository.rs`](./name_token_repository.rs): `NameTokenRepository` async trait (port): `get_name_token_by_outpoint`, `get_name_tokens_by_label`, `save_block_updates`, `get_name_tokens_by_block`, `undo_block_updates`, `get_chain`.
- [`name_token_service.rs`](./name_token_service.rs): `NameTokenService::new(Box<dyn NameTokenRepository>)`, `apply_block_connected_to(&Block, height, connected_to: BlockId)`, `get_name_token(&[u8])`, `tip() -> Option<CheckPoint>`.
- [`in_memory_name_token_repository.rs`](./in_memory_name_token_repository.rs): `InMemoryNameTokenRepository` (`Default`), a `Mutex`-guarded in-memory implementation of `NameTokenRepository`.

## Invariants
- An inscription is `OP_FALSE OP_IF <"name"> <label> [OP_NOP <protocol> <args…>]… OP_ENDIF`; `OP_NOP` separates sections, `OP_ENDIF` ends the last. Any other opcode, a missing label, or a non-`"name"` header makes `Inscription::from_txout` return `None` (`inscription.rs:parse_header`, `parse_section`; extra push-bytes arguments after `<label>` inside the `"name"` header section are ignored).
- `Position` ordering is derived field-wise: `block_id`, then `blockindex`, then `outpoint`.
- `NameToken::update` fails with `LabelMismatch`, `Revoked`, or `StaleInscription` (new position not strictly greater than `last_position`), checked in that order.
- `process_same_index_chain` pairs the Nth input with the Nth output: no input/inscription → mint; input only → revoke; both → update. On `LabelMismatch` it revokes and mints; on `Revoked` it mints; on `StaleInscription` it returns nothing.
- `select_root_token` picks the non-revoked token of the label with the smallest `first_position` (First Confirmed Rule).
- `first_position` / `last_position` panic if a token has no events.
- `NameTokenService::apply_transaction` iterates `max(inputs, outputs)` indices; each position's outpoint is `(txid, index)`. Within a block, pending updates are consulted by `last_outpoint` before the repository.
- Reorg: if the stored tip differs from `connected_to`, all blocks above `connected_to.height` are undone (events removed via `undo_block_events`; tokens with no remaining events are dropped) before the new block is applied.
- A `LocalChain` is created only when the first applied block has `height == 0` (or rebuilt from `get_chain` in `new`).
- `NameTokenRepository` implementors must be `Send + Sync`; `save_block_updates` and `undo_block_updates` are documented as atomic.

## Side Effects
- `NameTokenService` mutates its internal `Mutex<Option<LocalChain>>` and calls repository writes `save_block_updates` / `undo_block_updates`; reads via `get_chain` in `new`.
- `InMemoryNameTokenRepository` mutates only in-process state (`Mutex`); no disk or network access.
- No other file performs I/O.

## Verification
- Run: `cargo test -p name_token`
