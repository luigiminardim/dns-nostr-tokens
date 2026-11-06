# regtest_node

## Overview

Docker image for a local Bitcoin Core **regtest** node. It is the dev-infrastructure backend for `dns_nostr_server`, which reads Name-Token inscriptions over RPC. It contains no Rust code.

## Interface

- [`Dockerfile`](./Dockerfile): based on `bitcoin/bitcoin:28.1-alpine`. Copies `bitcoin.conf` to `/root/.bitcoin/bitcoin.conf` and `entry_point.sh` to `/entry_point.sh`, makes it executable, and sets it as `ENTRYPOINT`.
- [`bitcoin.conf`](./bitcoin.conf): `regtest=1`; in `[regtest]` sets `rpcuser=rpcuser`, `rpcpassword=rpcpassword`, `rpcbind=0.0.0.0`, `rpcallowip=0.0.0.0/0`.
- [`entry_point.sh`](./entry_point.sh): startup and mining script (`mine_block` function).
- RPC on container port 18443, published by the root `docker-compose.yml` as `localhost:18443`.
- P2P on container port 18444, published as `localhost:18444`.
- Compose service `bitcoin-core`; volume `bitcoin-core` mounted at `/root/.bitcoin`.
- Node wallet `miner` (created if missing); block rewards go to its bech32 addresses.
- Consumers hardcode `0.0.0.0:18443` and these credentials (see `dns_nostr_server`).

## Invariants

- Regtest only. The RPC credentials are public and RPC listens on all interfaces with all IPs allowed; never use on mainnet or expose publicly.
- `entry_point.sh` starts `bitcoind -daemon -datadir=/root/.bitcoin` and polls `bitcoin-cli getblockchaininfo` until RPC answers.
- It then runs `loadwallet "miner"`, and runs `createwallet "miner"` if that fails to report the wallet.
- It mines `for i in $(seq $CURRENT_BLOCK 100)` until the chain height reaches at least 101 (coinbase maturity: 101 blocks mined on a fresh node starting at height 0), then mines 1 block every 600 s in an infinite foreground loop.
- Chain and wallet state persist in the `bitcoin-core` volume; restarts only mine if `getblockcount` is `<= 100`. Reset with `docker compose down -v`.
- The script has a `#!/bin/sh` shebang but uses the bash-style `function mine_block {}`. It works under Alpine busybox ash; re-check if the base image changes.

## Side Effects

- Mines blocks, continuously changing chain height.
- Writes chain and wallet data to `/root/.bitcoin` (a Docker volume).

## Verification

- Run (from repo root):

```bash
docker compose up -d
docker compose exec bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword getblockcount   # expect >= 101 after startup
docker compose down        # add -v to wipe the chain
```

