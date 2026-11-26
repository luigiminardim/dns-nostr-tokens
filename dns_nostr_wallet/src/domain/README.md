# domain

## Overview
Pure in-memory wallet entity, synchronization domain service, stateless key derivation policy, and synchronous domain ports. `DnsNostrWallet` wraps an in-memory `bdk_wallet::Wallet` (BIP 84 receive/change keychains) and `master_key: Xpriv`, staging state changes through `bdk_wallet::ChangeSet` without any SQL or RPC crate dependency. `SyncService` coordinates tip anchoring and incremental sync between `DnsNostrWallet` and `BlockchainPort`. `KeyDerivationPolicy` encapsulates all BIP 32 / BIP 44 / BIP 84 key derivation rules operating on `master_key: &Xpriv`. `NameTokenWalletRepository` and `BlockchainPort` define the synchronous outer-layer contracts.

## Interface
- [`mod.rs`](./mod.rs): declares `pub mod blockchain_port`, `pub mod dns_nostr_wallet`, `pub mod key_derivation_policy`, `pub mod name_token_wallet_repository`, `pub mod sync_service`.
- [`dns_nostr_wallet.rs`](./dns_nostr_wallet.rs):
  - `DnsNostrWallet::new(&Mnemonic, Network) -> Result<Self, WalletError>`, `from_loaded(bdk_wallet::Wallet, Xpriv) -> Self`
  - Accessors: `network() -> Network`, `genesis_hash() -> BlockHash`, `latest_checkpoint() -> CheckPoint`, `unconfirmed_txids() -> Vec<Txid>`, `spendable_balance() -> Amount`
  - Mutations: `reveal_next_bitcoin_address(&mut self) -> AddressInfo`, `apply_tip_anchor(&mut self, Vec<BlockId>) -> Result<(), WalletError>`, `apply_block_connected_to(&mut self, &Block, u32, BlockId) -> Result<(), WalletError>`, `apply_evicted_txs(&mut self, Vec<(Txid, u64)>)`, `record_broadcasted_transaction(&mut self, Transaction, u64)`, `build_and_sign_transaction(&mut self, Address, Amount, FeeRate) -> Result<Transaction, WalletError>`
  - Staging: `staged(&self) -> Option<&ChangeSet>`, `take_staged(&mut self) -> Option<ChangeSet>`
  - `WalletError { BdkCreate, Bip32, ChainUpdate, InsufficientFunds { needed, available, immature }, TxBuild, Signing, Finalization }`
- [`blockchain_port.rs`](./blockchain_port.rs):
  - `BlockchainPort` trait: `fetch_tip_anchor`, `sync`, `estimate_fee_rate`, `broadcast`
  - `BlockchainError { GenesisMismatch { expected, actual }, Rpc(String), ChainUpdate(String), Broadcast(String) }`
  - `#[cfg(test)] pub struct FakeBlockchain`
- [`sync_service.rs`](./sync_service.rs):
  - `SyncService`: `tip_anchor(&self, &mut DnsNostrWallet, &dyn BlockchainPort) -> Result<(), SyncServiceError>`, `sync(&self, &mut DnsNostrWallet, &dyn BlockchainPort) -> Result<(), SyncServiceError>`
  - `SyncServiceError { Blockchain(BlockchainError), Wallet(String) }`
- [`key_derivation_policy.rs`](./key_derivation_policy.rs):
  - `KeyDerivationPolicy::bitcoin_external_descriptor`, `bitcoin_internal_descriptor`, `name_token_utxo_key`, `nostr_key`
  - `KeyDerivationError { InvalidChildIndex(u32), Bip32(bitcoin::bip32::Error), InvalidNostrSecretKey }`
- [`name_token_wallet_repository.rs`](./name_token_wallet_repository.rs):
  - `NameTokenWalletRepository` trait: `create`, `save`, `load`
  - `WalletRepositoryError { WalletAlreadyExists, CorruptedState(String), Storage(String) }`

## Invariants
- Standard Bitcoin addresses use BIP 84 descriptors (`Bip84<Xpriv>`, `KeychainKind::External`/`Internal`).
- `spendable_balance` and `build_and_sign_transaction` include `confirmed` and wallet-originated `trusted_pending` (change) outputs while excluding unconfirmed external outputs and immature coinbase outputs (`< 100` confirmations).
- `build_and_sign_transaction` requires all PSBT inputs to finalize before extracting the signed transaction.
- Name-Token UTXO keys use `m/44'/coin'/0'/0x6E616D65/index` (`index < 2^31`); Nostr keys use `m/44'/1237'/0'/0x6E616D65/hash31(label)`.
- The master key, descriptors, and seed are private; `Debug` for `DnsNostrWallet` prints only `network`.

## Side Effects
- None. All operations in `domain` are pure and in-memory.

## Verification
- Run: `cargo test -p dns_nostr_wallet`
