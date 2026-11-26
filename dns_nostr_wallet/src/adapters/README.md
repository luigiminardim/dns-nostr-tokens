# adapters

## Overview
Outer-layer adapters: environment configuration, Bitcoin Core JSON-RPC blockchain adapter, lazy SQLite repository adapter, and clap CLI controller.

## Interface
- [`environment_config.rs`](./environment_config.rs): `EnvironmentConfig { network, wallet_dir, bitcoin_rpc_url, bitcoin_rpc_user, bitcoin_rpc_password }`, `from_environment()`, `from_lookup(...)`, `db_path()`, custom `Debug` redacting password; `ConfigError { UnknownNetwork, EmptyValue, InvalidUrl }`; constants `NETWORK_VARIABLE`, `WALLET_DIR_VARIABLE`, `BITCOIN_RPC_URL_VARIABLE`, `BITCOIN_RPC_USER_VARIABLE`, `BITCOIN_RPC_PASSWORD_VARIABLE`.
- [`rpc_blockchain_adapter.rs`](./rpc_blockchain_adapter.rs): `RpcBlockchainAdapter::new(rpc_url, rpc_user, rpc_password)`, custom `Debug` redacting password, implements `BlockchainPort` using `bdk_bitcoind_rpc` (`Emitter::next_block`, `get_block_hash(0)`, `get_best_block_hash`, `get_block_header_info`, `get_mempool_entry`, `estimate_smart_fee`, `get_mempool_info`, `send_raw_transaction`).
- [`sqlite_name_token_wallet_repository.rs`](./sqlite_name_token_wallet_repository.rs): `SqliteNameTokenWalletRepository::open(PathBuf)`, `new(Connection)`, `in_memory() -> Result<Self, bdk_wallet::rusqlite::Error>`, implements `NameTokenWalletRepository`.
- [`cli_controller.rs`](./cli_controller.rs): clap `Cli`, `Command::Wallet`, `WalletCommand { Create, Import { mnemonic }, Address, Balance, Send { address, amount_btc } }`; `CliController::new(...)`, `execute(Command, &mut dyn Read) -> Result<String, CliControllerError>`; `CliControllerError`.
- [`mod.rs`](./mod.rs) declares `cli_controller`, `environment_config`, `rpc_blockchain_adapter`, and `sqlite_name_token_wallet_repository`.

## Invariants
- `EnvironmentConfig` defaults to `regtest`, `./.data/dns-nostr-wallet`, `http://127.0.0.1:18443`, `rpcuser`, `rpcpassword`, validates HTTP(S) URLs and non-empty credentials, preserves raw password whitespace, and redacts `bitcoin_rpc_password` in `Debug`.
- `RpcBlockchainAdapter` verifies genesis hash before tip anchoring or syncing, walks at most 10 linked headers for tip anchoring, evicts unconfirmed self-sent txs only on RPC error code `-5`, converts `BTC/kvB` fee estimates to `sat/kwu` with ceiling division (`div_ceil(4)`), and enforces a `1 sat/vB` (`250 sat/kwu`) floor.
- `SqliteNameTokenWalletRepository::load` never creates directories or files, opens existing files with `SQLITE_OPEN_READ_WRITE` (no `SQLITE_OPEN_CREATE`), runs read-only `DEFERRED` queries, restores in-memory signers via `extract_keys()`, and maps missing tables or mismatched descriptors to `CorruptedState`.
- `SqliteNameTokenWalletRepository::create` creates `wallet.sqlite` atomically with `0600` permissions on Unix and commits mnemonic + BDK `ChangeSet` in one transaction.
- `CliController` emits bare values on stdout for `address` (`<address>`), `balance` (`<decimal_btc>`), and `send` (`<txid>`).

## Side Effects
- Reads process environment variables, performs SQLite file/DB I/O, executes Bitcoin Core JSON-RPC requests, and reads stdin / formats stdout.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
