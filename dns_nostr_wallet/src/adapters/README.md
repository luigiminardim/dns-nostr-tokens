# adapters

## Overview
Outer-layer adapters: environment configuration, the SQLite implementation of `NameTokenWalletRepository`, and the clap command-line interface that translates commands into use-case calls and results into output text.

## Interface
- [`environment_config.rs`](./environment_config.rs): `EnvironmentConfig { network, wallet_dir }`, `from_environment()`, `from_lookup(impl Fn(&str) -> Option<String>)`, `db_path()`; `ConfigError::UnknownNetwork(String)`; constants `NETWORK_VARIABLE` (`DNS_NOSTR_NETWORK`), `WALLET_DIR_VARIABLE` (`DNS_NOSTR_WALLET_DIR`).
- [`sqlite_name_token_wallet_repository.rs`](./sqlite_name_token_wallet_repository.rs): `SqliteNameTokenWalletRepository::new(Connection)`, `in_memory() -> Result<Self, rusqlite::Error>`, implements `NameTokenWalletRepository` (`create`, `save`, `load`).
- [`cli.rs`](./cli.rs): clap `Cli`, `Command::Wallet`, `WalletCommand { Create, Import { mnemonic }, Address }`; `CliController::new(CreateWallet, ImportWallet, GetReceiveAddress)`, `execute(Command, &mut dyn Read) -> Result<String, CliError>`; `CliError { CreateWallet, ImportWallet, GetReceiveAddress, ReadingStdin }`.
- [`mod.rs`](./mod.rs) declares `cli`, `environment_config` and `sqlite_name_token_wallet_repository`.

## Invariants
- Network defaults to `Network::Regtest`; accepted values are `regtest`, `testnet`, `mainnet` (`Network::Bitcoin`); anything else is `ConfigError::UnknownNetwork`. (`signet` is not accepted here.)
- Wallet directory defaults to `./.data/dns-nostr-wallet`; the database file name is `wallet.sqlite`.
- `SqliteNameTokenWalletRepository` executes all writes across `dns_nostr_wallet (id INTEGER PRIMARY KEY CHECK (id = 0), mnemonic TEXT NOT NULL)` and BDK's `bdk_*` `ChangeSet` tables inside a single `rusqlite::Transaction`, and clears `wallet.take_staged()` only after `Transaction::commit()` succeeds.
- `SqliteNameTokenWalletRepository::load` validates that the mnemonic row and BDK `ChangeSet` are either both absent (`Ok(None)`) or both present (`Ok(Some(wallet))`), returning `WalletRepositoryError::CorruptedState` if only one is present or if stored data is invalid.
- `import` with no words reads the phrase from the supplied reader; otherwise the words are joined by spaces.
- `address` prints only the revealed address string.

## Side Effects
- `EnvironmentConfig::from_environment` reads process environment variables.
- `SqliteNameTokenWalletRepository` creates/queries/updates `dns_nostr_wallet` and `bdk_*` tables in SQLite, storing the mnemonic in plaintext.
- `create` prints the seed phrase in its output text; `import` may consume stdin.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
