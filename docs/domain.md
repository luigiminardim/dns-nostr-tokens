# Domain Model — DNS-Nostr Tokens

> **This is the canonical reference for the ubiquitous language and high-level
> domain model.** Every domain term used in code, tests, and conversation must
> be defined here. If a term is missing or wrong, update this file first.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Name-Token** | ([`name_token.rs`](../name_token/src/name_token.rs)) A Bitcoin UTXO whose `scriptPubKey` contains an `OP_FALSE OP_IF … OP_ENDIF` inscription with a `"name"` header and a **Label**. Represents ownership of a unique name on the blockchain, modeled as an event-sourced sequence of `NameTokenEvent`s. |
| **Label** | ([`name_token/src/label.rs`](../name_token/src/label.rs), [`dns_nostr_token/src/label.rs`](../dns_nostr_token/src/label.rs)) The name inscribed inside a Name-Token (e.g., `blog`). Raw bytes in `name_token`; validated RFC-1034 lowercase ASCII string in `dns_nostr_token`. Labels are unique within the system — **First-is-Root Uniqueness** resolves conflicts. |
| **Position** | ([`position.rs`](../name_token/src/position.rs)) The on-chain location of a Name-Token event: `(BlockId { height, hash }, blockindex, OutPoint { txid, vout })`, ordered field-wise. |
| **NameTokenEvent** | ([`name_token.rs`](../name_token/src/name_token.rs)) A lifecycle event recorded on a `NameToken`: `Minted { position, inscription }`, `Updated { position, inscription }`, or `Revoked { position }`. |
| **Inscription** | ([`inscription.rs`](../name_token/src/inscription.rs)) The `OP_FALSE OP_IF … OP_ENDIF` data envelope embedded in a UTXO's `scriptPubKey`. Validated for a `"name"` header, carrying a `Label` and zero or more **Inscription Sections**. |
| **Inscription Section** | ([`inscription.rs`](../name_token/src/inscription.rs)) A typed segment (`InscriptionSection`) within an Inscription, separated by `OP_NOP`. Each section has a `protocol: Bytes` identifier (e.g., `b"dns-nostr"`) and protocol-specific `arguments: Vec<Bytes>`. |
| **DNS-Nostr Token** | ([`dns_nostr_token.rs`](../dns_nostr_token/src/dns_nostr_token.rs)) A Name-Token (`DnsNostrToken`) whose Label conforms to DNS rules and that contains a `"dns-nostr"` Inscription Section whose first argument is a 32-byte raw Nostr public key (`nostr_sdk::PublicKey`). |
| **First-is-Root Uniqueness** | ([`NameToken::select_root_token`](../name_token/src/name_token.rs)) The conflict-resolution rule (also called **First Confirmed Rule**): among competing non-revoked Name-Tokens for the same Label, the earliest confirmed one (smallest `first_position()`) wins. |
| **Update** | ([`NameToken::update`](../name_token/src/name_token.rs)) Spending a Name-Token's UTXO and creating a new UTXO with the same Label in the same transaction. Continuity via Same-Index Chain (`process_same_index_chain`): the Nth input must correspond to the Nth output. |
| **Revocation** | ([`NameToken::revoke`](../name_token/src/name_token.rs)) Spending a Name-Token's UTXO without creating a new inscription for that Label at the same output index. Appends `NameTokenEvent::Revoked`; the name becomes unowned. |
| **Valid Name-Token** | The Name-Token selected by the First-is-Root Uniqueness rule for a given Label — the one the system treats as authoritative. |
| **Zone** | ([`nostr_authority.rs`](../dns_nostr_server/src/nostr_authority.rs)) The DNS domain under which subdomains are resolved (e.g., `nostr.dns.name.`). Labels become subdomains of this zone. |
| **Name-Token Service** | ([`name_token_service.rs`](../name_token/src/name_token_service.rs)) Domain service (`NameTokenService`) that applies confirmed blocks and handles reorgs (`apply_block_connected_to`), tracks the chain tip (`tip`), and resolves a Label to its Valid Name-Token (`get_name_token`). |
| **Name-Token Repository** | ([`name_token_repository.rs`](../name_token/src/name_token_repository.rs)) Port (`NameTokenRepository` trait) defined by the domain and implemented by outer layers ([`InMemoryNameTokenRepository`](../name_token/src/in_memory_name_token_repository.rs), [`SqliteNameTokenRepository`](../dns_nostr_server/src/sqlite_name_token_repository.rs)); stores Name-Tokens and chain checkpoints (`Vec<BlockId>`), supporting atomic block updates (`save_block_updates`) and reorg rollbacks (`undo_block_updates`). |
| **DNS-Nostr Service** | ([`dns_nostr_service.rs`](../dns_nostr_token/src/dns_nostr_service.rs)) Domain service (`DnsNostrService`) wrapping `NameTokenService` that resolves a validated `Label` into a `DnsNostrToken` (`get_token`). |
| **DnsNostrWallet** | ([`dns_nostr_wallet.rs`](../dns_nostr_wallet/src/domain/dns_nostr_wallet.rs)) An in-memory wallet entity wrapping `bdk_wallet::Wallet` for BIP 84 address management (`reveal_next_bitcoin_address`) and staged `ChangeSet` tracking alongside `master_key: Xpriv`. Master keys and descriptors remain strictly private. |
| **KeyDerivationPolicy** | ([`key_derivation_policy.rs`](../dns_nostr_wallet/src/domain/key_derivation_policy.rs)) Stateless domain policy encapsulating BIP 32 / BIP 44 / BIP 84 key derivation rules operating on `master_key: &Xpriv`: BIP 84 external/internal descriptors (`bitcoin_external_descriptor`, `bitcoin_internal_descriptor`), Name-Token UTXO keys (`name_token_utxo_key` on chain `0x6E616D65`), and per-label Nostr keys (`nostr_key` via `hash31`). |
| **Name-Token Wallet Repository** | ([`name_token_wallet_repository.rs`](../dns_nostr_wallet/src/domain/name_token_wallet_repository.rs)) Synchronous persistence port (`NameTokenWalletRepository` trait) defined in the wallet domain and implemented by [`SqliteNameTokenWalletRepository`](../dns_nostr_wallet/src/adapters/sqlite_name_token_wallet_repository.rs); atomically persists the `Mnemonic` and BDK `ChangeSet` inside a single SQLite transaction (`create`, `save`, `load`), clearing the wallet's staged `ChangeSet` only after commit succeeds. |
| **Mnemonic** | The BIP39 seed phrase (12 words when generated; 12–24 accepted on import) with an empty passphrase. Used to initialize `DnsNostrWallet` and persisted in `wallet.sqlite` (mode `0600`); never logged or printed except when generated by `wallet create`. |
| **Standard UTXO** | A wallet UTXO at a BIP 84 path (`m/84'/coin'/0'/{0,1}/i`, P2WPKH) used for ordinary funds and fees. |
| **Token UTXO** | A Name-Token UTXO owned by the wallet (P2PKH), at the custom chain `0x6E616D65` (ASCII `"name"`) under the BIP 44 purpose: `m/44'/coin'/0'/0x6E616D65/i`. Standard wallets do not scan this chain, so Token UTXOs are never spent by accident. |
| **Derivation Path** | A BIP 32 path. Bitcoin coin type is `0'` on mainnet and `1'` on testnet/signet/regtest. Nostr keys use SLIP-0044 coin type `1237'` (`m/44'/1237'/0'/0x6E616D65/hash31(label)`). |
| **hash31** | ([`key_derivation_policy.rs`](../dns_nostr_wallet/src/domain/key_derivation_policy.rs)) The per-label derivation index: first 4 bytes (big-endian) of the double SHA-256 of the Label, masked with `0x7FFFFFFF` (non-hardened). `hash31("alice") = 0x3D306425` (`1_026_581_541`). |
| **Wallet use cases** | Application-layer interactors, one per file: [`CreateWallet`](../dns_nostr_wallet/src/application/create_wallet.rs), [`ImportWallet`](../dns_nostr_wallet/src/application/import_wallet.rs), [`GetReceiveAddress`](../dns_nostr_wallet/src/application/get_receive_address.rs). |

---

## High-Level Domain Model

```
┌─────────────────────────────────────────────────────────────┐
│                        Inscription                          │
│  (parsed from OP_FALSE OP_IF "name" <label> … OP_ENDIF)     │
│  label: Label                                               │
│  sections: Vec<InscriptionSection>                          │
│    ├── protocol: Bytes  (e.g., b"dns-nostr")                │
│    └── arguments: Vec<Bytes>                                │
└─────────────────────────────────────────────────────────────┘
                          │
                    recorded in
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                       Name-Token                            │
│  label: Label                                               │
│  events: Vec<NameTokenEvent>                                │
│    ├── Minted  { position: Position, inscription }          │
│    ├── Updated { position: Position, inscription }          │
│    └── Revoked { position: Position }                       │
│  (Position = { block_id: BlockId, blockindex, outpoint })   │
└─────────────────────────────────────────────────────────────┘
                          │
                   specialized as
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    DNS-Nostr Token                          │
│  (a Name-Token with a valid DNS Label & "dns-nostr" arg)    │
│  label: Label  (RFC-1034 lowercase ASCII)                   │
│  nostr_pubkey: PublicKey  (32 raw bytes)                    │
│    └── used to fetch DNS records from Nostr relays          │
└─────────────────────────────────────────────────────────────┘
```

### Lifecycle

1. **Minting** — A transaction inscribes a new Name-Token UTXO with a Label (`NameTokenEvent::Minted`).
2. **Conflict Resolution** — If multiple inscriptions claim the same Label, the
   First-is-Root Uniqueness rule (`NameToken::select_root_token`) picks the valid one.
3. **Update** — The owner spends the UTXO and reinscribes the same Label in a
   new output (`NameTokenEvent::Updated` via Continuity via Same-Index Chain: Nth input ↔ Nth output).
4. **Revocation** — The owner spends the UTXO without reinscribing the same Label at the same index (`NameTokenEvent::Revoked`). The Label becomes unowned.
5. **Reorganization** — If a block is disconnected during a chain reorg, events from that block are removed (`NameToken::undo_block_events`).

### Resolution Flow (DNS-Nostr)

```
DNS query for <label>.nostr.dns.name.
        │
        ▼
  Find valid Name-Token for <label>  (First-is-Root Uniqueness)
        │
        ▼
  Extract nostr_pubkey from "dns-nostr" section
        │
        ▼
  Fetch DNS records from Nostr relay using pubkey
        │
        ▼
  Return DNS response
```
