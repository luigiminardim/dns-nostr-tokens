# application

## Overview
Use cases, one per file (`*_usecase.rs`), each with its own error enum. They orchestrate `DnsNostrWallet`, `SyncService`, `NameTokenWalletRepository`, and `BlockchainPort`.

## Interface
- [`create_wallet_usecase.rs`](./create_wallet_usecase.rs): `CreateWalletUsecase::new(Network, Arc<dyn NameTokenWalletRepository>, Arc<dyn BlockchainPort>, Arc<SyncService>)`, `execute(&self) -> Result<Mnemonic, CreateWalletUsecaseError>`; `CreateWalletUsecaseError { Blockchain, Sync, WalletAlreadyExists, Wallet }`.
- [`import_wallet_usecase.rs`](./import_wallet_usecase.rs): `ImportWalletUsecase::new(...)`, `execute(&self, &str) -> Result<(), ImportWalletUsecaseError>`; `ImportWalletUsecaseError { Blockchain, Sync, InvalidMnemonic, WalletAlreadyExists, Wallet }`.
- [`get_receive_address_usecase.rs`](./get_receive_address_usecase.rs): `GetReceiveAddressUsecase::new(...)`, `execute(&self) -> Result<AddressInfo, GetReceiveAddressUsecaseError>`; `GetReceiveAddressUsecaseError { WalletNotFound, NetworkMismatch, Blockchain, Sync, Wallet }`.
- [`get_balance_usecase.rs`](./get_balance_usecase.rs): `GetBalanceUsecase::new(...)`, `execute(&self) -> Result<Amount, GetBalanceUsecaseError>`; `GetBalanceUsecaseError { WalletNotFound, NetworkMismatch, Blockchain, Sync, Wallet }`.
- [`send_bitcoin_usecase.rs`](./send_bitcoin_usecase.rs): `SendBitcoinUsecase::new(...)`, `execute(&self, address: &str, amount_btc: &str) -> Result<Txid, SendBitcoinUsecaseError>`; `SendBitcoinUsecaseError { InvalidAddress, AddressNetworkMismatch, InvalidAmount, DustAmount, WalletNotFound, NetworkMismatch, InsufficientFunds, Blockchain, Sync, Wallet, BroadcastSucceededButPersistFailed }`.
- [`mod.rs`](./mod.rs) declares all five use-case modules.

## Invariants
- `CreateWalletUsecase` and `ImportWalletUsecase` verify no wallet exists before making any RPC calls, and only call `repository.create` after tip anchoring or height-`0` sync succeeds.
- `GetReceiveAddressUsecase`, `GetBalanceUsecase`, and `SendBitcoinUsecase` verify `wallet.network() == self.network` before making any RPC calls.
- `SendBitcoinUsecase` validates recipient address, network, positive fixed-point BTC amount (`<= 8` decimals), and dust threshold before loading the wallet or calling RPC; performs a pre-broadcast `save` to preserve sync progress and a post-broadcast `save` to persist spent inputs and `trusted_pending` change (reporting `BroadcastSucceededButPersistFailed { txid, reason }` if the second save fails).

## Side Effects
- Delegates persistence to `NameTokenWalletRepository` and Bitcoin Core RPC calls to `BlockchainPort` / `SyncService`.
- `CreateWalletUsecase` consumes OS randomness (`Mnemonic::generate`); `SendBitcoinUsecase` reads wall-clock Unix time (`SystemTime::now`) and broadcasts signed transactions.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
