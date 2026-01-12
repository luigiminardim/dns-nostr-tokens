# 🪙 BDK Descriptor Wallet

This Rust project demonstrates how to use the [Bitcoin Dev Kit (BDK)](https://bitcoindevkit.org/) with Bitcoin Core in Regtest mode to:

* Generate a fresh descriptor-based wallet
* Automatically sync with a Bitcoin Core node over RPC
* Send and receive transactions
* Store wallet data using a `sled` database

## 🔧 Prerequisites

* [Rust](https://www.rust-lang.org/tools/install)
* [Bitcoin Core](https://bitcoincore.org/en/download/) running in **regtest mode** with RPC enabled
* [Docker](https://www.docker.com/) for running Bitcoin Core in a container
* A `.env` file configured with your RPC credentials

## 📦 Dependencies

* `bdk`
* `bdk::bitcoincore_rpc`
* `dotenvy`
* `sled`
* `hex`
* `dirs-next`

## 📁 Setup

### 1. Install Dependencies

```bash
cargo build
```

PS: You might have to update your `~/.cargo/registry/src/index.crates.io/bitcoin-rpc-json-0.15.0/src/lib.rs` line `1022` from `pub warnings: String` to `pub warnings: Vec<String>` depending on which Bitcoin Code version you're running.

### 2. Run Bitcoin Core in Regtest Mode

Start `bitcoin-core` using Docker.

### 3. Create `.env` File

Create a `.env` file in the project root, use the `.env.example` file as a template.

> `DESCRIPTOR_PASSWORD` is optional; it is used to encrypt the generated mnemonic.

## 🚀 Run the Program

```bash
cargo run
```

On the first run:

* Generates a new BIP84 (SegWit) descriptor-based wallet
* Syncs with Bitcoin Core via RPC
* Funds the wallet from the `miner` wallet
* Creates and broadcasts a transaction back to the miner

Console output includes:

* Generated wallet descriptors (only on first run)
* Miner and wallet balances

## 💡 How It Works

* **Wallet Creation**: If no descriptors are set in the environment, it generates a BIP84 mnemonic and derives two descriptors:

  * Receive path: `m/84h/1h/0h/0`
  * Change path: `m/84h/1h/0h/1`

* **Storage**: Uses `sled` for local persistent storage under `~/.bdk-db`.

* **Wallet Naming**: Uses a deterministic wallet name based on descriptor content.

* **Transaction Flow**:

  1. Miner wallet funds the BDK wallet.
  2. BDK wallet sends funds back to a new address from the miner.
  3. Both transactions are confirmed using `generate_to_address`.

## 🧪 Testing Tips

You can inspect balances or transactions with:

```bash
bitcoin-cli -regtest -rpcuser=<user> -rpcpassword=<password> -rpcwallet=miner getbalance
bitcoin-cli -regtest -rpcuser=<user> -rpcpassword=<password> -rpcwallet=miner listtransactions
```

## 📂 Directory Structure

```text
.
├── src/
│   └── main.rs     # Main Rust file containing wallet logic
├── .env            # RPC credentials and optional descriptor values
├── Cargo.toml      # Rust project configuration
└── README.md       # This file
```

## 🔐 Security Warning

**Do not use this code on mainnet.** It is intended for educational and testing purposes in regtest or testnet environments only.
