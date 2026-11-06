# dns_nostr_wallet Blueprint

## System Overview
CLI wallet crate (library + binary) for DNS-Nostr. Currently it creates or imports a BIP39 seed, stores it in SQLite, reveals/peeks BIP 84 receive addresses, and derives Name-Token P2PKH addresses and per-label Nostr keys in the domain layer. Token operations, UTXO scanning and send/receive are not built. Usage: `cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address [--index N]`.

Dependencies (`Cargo.toml`): `bdk_wallet`, `bip39`, `bitcoin`, `clap`, `dns_nostr_token` (for `Label`), `nostr-sdk`, `rusqlite`.

## Subdirectories & System Boundaries
- src: [./src/README.md](./src/README.md) - crate root, `main` composition root and layer declarations.
- src/adapters: [./src/adapters/README.md](./src/adapters/README.md) - `EnvironmentConfig` and clap `CliController`.
- src/application: [./src/application/README.md](./src/application/README.md) - `CreateWallet`, `ImportWallet`, `GetReceiveAddress` use cases.
- src/domain: [./src/domain/README.md](./src/domain/README.md) - `DnsNostrWallet` entity and key derivation.

## Global Invariants
- Dependency direction: `adapters` → `application` → `domain`.
- At most one wallet per database; create/import never overwrite an existing wallet.
- Network is `regtest` by default; BIP 44/84 coin type is `0'` on mainnet and `1'` otherwise.

## Global Side Effects
- Persists state in `<DNS_NOSTR_WALLET_DIR>/wallet.sqlite` (mode `0600` on Unix), including the mnemonic in plaintext.
- Reads `DNS_NOSTR_NETWORK` and `DNS_NOSTR_WALLET_DIR`.

## System Verification
- Run: `cargo test -p dns_nostr_wallet`
