# adapters

## Overview
Outer-layer adapters: environment configuration and the clap command-line interface that translates commands into use-case calls and results into output text.

## Interface
- [`environment_config.rs`](./environment_config.rs): `EnvironmentConfig { network, wallet_dir }`, `from_environment()`, `from_lookup(impl Fn(&str) -> Option<String>)`, `db_path()`; `ConfigError::UnknownNetwork(String)`; constants `NETWORK_VARIABLE` (`DNS_NOSTR_NETWORK`), `WALLET_DIR_VARIABLE` (`DNS_NOSTR_WALLET_DIR`).
- [`cli.rs`](./cli.rs): clap `Cli`, `Command::Wallet`, `WalletCommand { Create, Import { mnemonic }, Address { index } }` (`--index`); `CliController::new(CreateWallet, ImportWallet, GetReceiveAddress, Arc<Mutex<Connection>>)`, `execute(Command, &mut dyn Read) -> Result<String, CliError>`; `CliError { CreateWallet, ImportWallet, GetReceiveAddress, ReadingStdin }`.
- [`mod.rs`](./mod.rs) declares `cli` and `environment_config`.

## Invariants
- Network defaults to `Network::Regtest`; accepted values are `regtest`, `testnet`, `mainnet` (`Network::Bitcoin`); anything else is `ConfigError::UnknownNetwork`. (`signet` is not accepted here.)
- Wallet directory defaults to `./.data/dns-nostr-wallet`; the database file name is `wallet.sqlite`.
- `import` with no words reads the phrase from the supplied reader; otherwise the words are joined by spaces.
- `CliController` holds the connection mutex for the duration of one command.
- `address` prints only the address string.

## Side Effects
- `EnvironmentConfig::from_environment` reads process environment variables.
- `create` prints the seed phrase in its output text; `import` may consume stdin.
- Commands read/write the wallet SQLite database through the use cases.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
