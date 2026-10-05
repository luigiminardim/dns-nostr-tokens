# Domain Model — DNS-Nostr Tokens

> **This is the canonical reference for the ubiquitous language and high-level
> domain model.** Every domain term used in code, tests, and conversation must
> be defined here. If a term is missing or wrong, update this file first.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Name-Token** | A Bitcoin UTXO whose `scriptPubKey` contains an `OP_FALSE OP_IF … OP_ENDIF` inscription with a `"name"` header and a **Label**. Represents ownership of a unique name on the blockchain. |
| **Label** | The human-readable name inscribed inside a Name-Token (e.g., `blog`). Labels are unique within the system — the **First Confirmed Rule** resolves conflicts. |
| **Inscription** | The `OP_FALSE OP_IF … OP_ENDIF` data envelope embedded in a UTXO's `scriptPubKey`. Contains a header (`"name"`) and one or more **Inscription Sections**. |
| **Inscription Section** | A typed segment within an Inscription. Each section has a protocol identifier (e.g., `"dns-nostr"`) and protocol-specific data. |
| **DNS-Nostr Token** | A Name-Token that contains a `"dns-nostr"` Inscription Section. The section data is a Nostr public key (hex-encoded). |
| **First Confirmed Rule** | The conflict-resolution rule: among competing inscriptions for the same Label, the earliest confirmed (lowest block height, then lowest transaction index), non-revoked one wins. |
| **Update** | Spending a Name-Token's UTXO and creating a new UTXO with the same Label in the same transaction. Positional correlation: the Nth input must correspond to the Nth output. |
| **Revocation** | Spending a Name-Token's UTXO without creating a new inscription for that Label. The name becomes unowned. |
| **Valid Name-Token** | The Name-Token selected by the First Confirmed Rule for a given Label — the one the system treats as authoritative. |
| **Zone** | The DNS domain under which subdomains are resolved (e.g., `nostr.dns.name.`). Labels become subdomains of this zone. |
| **Name-Token Service** | Domain service (`NameTokenService`) that applies confirmed blocks to the Name-Token state (`apply_block`) and resolves a Label to its Valid Name-Token (`get_name_token`). Lives in the `name_token` crate. |
| **Name-Token Repository** | Port (`NameTokenRepository` trait) defined by the domain and implemented by outer layers (e.g., SQLite); stores Name-Tokens and the next block height to apply. |

---

## High-Level Domain Model

```
┌─────────────────────────────────────────────────────────────┐
│                        Inscription                          │
│  header: "name"                                             │
│  label: Label                                               │
│  sections: Vec<InscriptionSection>                          │
│    ├── protocol: String  (e.g., "dns-nostr")                │
│    └── data: Bytes                                          │
└─────────────────────────────────────────────────────────────┘
                          │
                    inscribed in
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                       Name-Token                            │
│  label: Label                                               │
│  first_inscription_metadata: InscriptionMetadata            │
│  last_inscription_metadata: InscriptionMetadata             │
│    (blockheight, blockindex, vout, txid)                    │
│  inscription: Option<Inscription>  (None = revoked)         │
└─────────────────────────────────────────────────────────────┘
                          │
                   specialized as
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    DNS-Nostr Token                           │
│  (a Name-Token with a "dns-nostr" section)                  │
│  nostr_pubkey: PublicKey (hex)                               │
│    └── used to fetch DNS records from Nostr relays          │
└─────────────────────────────────────────────────────────────┘
```

### Lifecycle

1. **Creation** — A transaction inscribes a new Name-Token UTXO with a Label.
2. **Conflict Resolution** — If multiple inscriptions claim the same Label, the
   First Confirmed Rule picks the valid one.
3. **Update** — The owner spends the UTXO and reinscribes the same Label in a
   new output (positional correlation: Nth input ↔ Nth output).
4. **Revocation** — The owner spends the UTXO without reinscribing. The Label
   becomes unowned.

### Resolution Flow (DNS-Nostr)

```
DNS query for <label>.nostr.dns.name.
        │
        ▼
  Find valid Name-Token for <label>  (First Confirmed Rule)
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
