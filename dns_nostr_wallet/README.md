# dns_nostr_wallet Blueprint

## System Overview
CLI wallet crate (library + binary) for DNS-Nostr. Connects to a local Bitcoin Core JSON-RPC node across all five `wallet` subcommands (`create`, `import`, `address`, `balance`, `send`), persisting wallet state atomically in SQLite via `SqliteNameTokenWalletRepository`. Supports BIP39 seed creation (tip-anchored) and import (height-`0` confirmed-block scan), incremental BIP 84 Standard UTXO sync with targeted self-tx mempool eviction checks, spendable balance reporting, automatic fee estimation, and PSBT signing/broadcast, plus Name-Token UTXO key and per-label Nostr key derivation in `KeyDerivationPolicy`. Token operations and Token UTXO scanning are not built yet. Usage: `cargo run -p dns_nostr_wallet -- wallet create | import [WORDS...] | address | balance | send <address> <btc>`.

Dependencies (`Cargo.toml`): `bdk_bitcoind_rpc` (workspace), `bdk_wallet` (`2.3`, `rusqlite`), `bip39`, `bitcoin` (workspace), `clap`, `dns_nostr_token` (for `Label`), `nostr-sdk`.

## Subdirectories & System Boundaries
- src: [./src/README.md](./src/README.md) - crate root, `main` composition root and layer declarations.
- src/adapters: [./src/adapters/README.md](./src/adapters/README.md) - `EnvironmentConfig`, `RpcBlockchainAdapter`, `SqliteNameTokenWalletRepository`, and clap `CliController`.
- src/application: [./src/application/README.md](./src/application/README.md) - `CreateWalletUsecase`, `ImportWalletUsecase`, `GetReceiveAddressUsecase`, `GetBalanceUsecase`, `SendBitcoinUsecase`.
- src/domain: [./src/domain/README.md](./src/domain/README.md) - `DnsNostrWallet` entity, `SyncService`, `KeyDerivationPolicy`, `BlockchainPort`, and `NameTokenWalletRepository`.

## Global Invariants
- Dependency direction: `adapters` → `application` → `domain`.
- At most one wallet per database; create/import never overwrite an existing wallet and leave no `wallet.sqlite` on failed RPC initialization.
- Network is `regtest` by default; genesis hash is verified against the connected Bitcoin Core node on every command, and stored wallet network is verified against `DNS_NOSTR_NETWORK` on `address`/`balance`/`send`.
- BIP 44/84 coin type is `0'` on mainnet and `1'` otherwise.

## Global Side Effects
- Lazily creates and persists state in `<DNS_NOSTR_WALLET_DIR>/wallet.sqlite` (mode `0600` on Unix), including the mnemonic in plaintext.
- Communicates with Bitcoin Core over JSON-RPC using `DNS_NOSTR_BITCOIN_RPC_URL`, `DNS_NOSTR_BITCOIN_RPC_USER`, and `DNS_NOSTR_BITCOIN_RPC_PASSWORD`.
- Reads `DNS_NOSTR_NETWORK`, `DNS_NOSTR_WALLET_DIR`, and `DNS_NOSTR_BITCOIN_RPC_*`.

## System Verification
- Run: `cargo test -p dns_nostr_wallet`
