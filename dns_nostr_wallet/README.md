# dns_nostr_wallet Blueprint

## System Overview
CLI wallet crate (library + binary) for DNS-Nostr. Currently it creates or imports a BIP39 seed, persists it and BDK `ChangeSet` state atomically in SQLite via `SqliteNameTokenWalletRepository`, reveals BIP 84 receive addresses (`DnsNostrWallet`), and derives BIP 84 descriptors, Name-Token UTXO keys and per-label Nostr keys in `KeyDerivationPolicy`. Token operations, UTXO scanning and send/receive are not built. Usage: `cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address`.

Dependencies (`Cargo.toml`): `bdk_wallet`, `bip39`, `bitcoin`, `clap`, `dns_nostr_token` (for `Label`), `nostr-sdk`, `rusqlite`.

## Subdirectories & System Boundaries
- src: [./src/README.md](./src/README.md) - crate root, `main` composition root and layer declarations.
- src/adapters: [./src/adapters/README.md](./src/adapters/README.md) - `EnvironmentConfig`, `SqliteNameTokenWalletRepository` and clap `CliController`.
- src/application: [./src/application/README.md](./src/application/README.md) - `CreateWallet`, `ImportWallet`, `GetReceiveAddress` use cases.
- src/domain: [./src/domain/README.md](./src/domain/README.md) - `DnsNostrWallet` entity, `KeyDerivationPolicy` and `NameTokenWalletRepository` port.

## Global Invariants
- Dependency direction: `adapters` → `application` → `domain`.
- At most one wallet per database; create/import never overwrite an existing wallet.
- Network is `regtest` by default; BIP 44/84 coin type is `0'` on mainnet and `1'` otherwise.

## Global Side Effects
- Persists state in `<DNS_NOSTR_WALLET_DIR>/wallet.sqlite` (mode `0600` on Unix), including the mnemonic in plaintext.
- Reads `DNS_NOSTR_NETWORK` and `DNS_NOSTR_WALLET_DIR`.

## System Verification
- Run: `cargo test -p dns_nostr_wallet`
