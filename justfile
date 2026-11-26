# Run wallet CLI subcommands (prepends `wallet` subcommand; preserves stdin and DNS_NOSTR_* env vars)
[positional-arguments]
wallet *args:
    cargo run -p dns_nostr_wallet -- wallet "$@"

# Run dns_nostr_server (honors all DNS_NOSTR_* environment variables)
[positional-arguments]
serve *args:
    @cargo run -p dns_nostr_server -- "$@"

# Build all workspace crates
build:
    cargo build --workspace

# Run offline workspace unit tests (no Docker needed)
test:
    cargo test --workspace

# Start containers, build server and wallet binaries, and run the full-stack E2E suite
test-e2e: containers-up
    cargo build -p dns_nostr_server -p dns_nostr_wallet
    cargo run -p end_to_end

# Start Bitcoin Core (regtest) and Nostr relay containers and wait until healthy
containers-up:
    docker compose up -d --wait

# Stop containers while preserving Docker volumes
containers-down:
    docker compose down

# Stop containers, wipe Docker volumes, and delete the local server SQLite database
containers-reset:
    docker compose down -v
    rm -f "${DNS_NOSTR_DB_PATH:-./data/name-tokens.sqlite}" "${DNS_NOSTR_DB_PATH:-./data/name-tokens.sqlite}-wal" "${DNS_NOSTR_DB_PATH:-./data/name-tokens.sqlite}-shm"

# Pass-through to regtest bitcoin-cli inside the bitcoin-core container
[positional-arguments]
bitcoin-cli *args:
    @docker compose exec -T bitcoin-core bitcoin-cli -regtest -rpcuser="${DNS_NOSTR_BITCOIN_RPC_USER:-rpcuser}" -rpcpassword="${DNS_NOSTR_BITCOIN_RPC_PASSWORD:-rpcpassword}" "$@"

# Ensure the regtest "miner" wallet exists and is loaded
[private]
_ensure-miner:
    #!/usr/bin/env sh
    set -eu
    if ! just bitcoin-cli listwallets | grep -Fq '"miner"'; then
        if just bitcoin-cli listwalletdir | grep -Eq '"name"[[:space:]]*:[[:space:]]*"miner"'; then
            just bitcoin-cli loadwallet "miner" >/dev/null
        else
            just bitcoin-cli createwallet "miner" >/dev/null
        fi
    fi

# Print a regtest address from the local miner wallet
miner-address: _ensure-miner
    @just bitcoin-cli -rpcwallet=miner getnewaddress

# Print the spendable BTC balance of the local miner wallet
miner-balance: _ensure-miner
    @just bitcoin-cli -rpcwallet=miner getbalance

# Mine regtest block(s) to the local miner wallet and print bare block hash(es)
miner-mineblock count='1':
    #!/usr/bin/env sh
    set -eu
    case "{{count}}" in
        ''|*[!0-9]*)
            echo "Invalid block count '{{count}}': expected a positive integer" >&2
            exit 1
            ;;
    esac
    if [ "{{count}}" -le 0 ]; then
        echo "Invalid block count '{{count}}': expected a positive integer" >&2
        exit 1
    fi
    miner_addr=$(just miner-address)
    just bitcoin-cli -rpcwallet=miner generatetoaddress "{{count}}" "$miner_addr" \
        | tr -d '[]" ,' \
        | grep -v '^$'

# Fund a regtest address from the miner wallet (auto-mines 1 confirmation block unless --no-mine)
[positional-arguments]
miner-fund address btc *flags:
    #!/usr/bin/env sh
    set -eu
    address="$1"
    btc="$2"
    shift 2
    confirm=true
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --no-mine)
                confirm=false
                ;;
            *)
                echo "Usage: just miner-fund <address> <btc> [--no-mine]" >&2
                exit 1
                ;;
        esac
        shift
    done
    if ! awk -v v="$btc" 'BEGIN { exit !(v ~ /^[0-9]+(\.[0-9]+)?$/ && v + 0 > 0) }'; then
        echo "Invalid BTC amount: $btc" >&2
        exit 1
    fi
    just _ensure-miner
    height=$(just bitcoin-cli getblockcount)
    if [ "$height" -lt 101 ]; then
        just miner-mineblock $((101 - height)) >/dev/null
    fi
    mined_topup=0
    while balance=$(just bitcoin-cli -rpcwallet=miner getbalance) && \
          awk -v bal="$balance" -v need="$btc" 'BEGIN { exit !(bal + 0 < need + 0.01) }'; do
        if [ "$mined_topup" -ge 500 ]; then
            echo "Miner wallet balance ($balance BTC) remained below $btc BTC after mining 500 blocks (likely regtest subsidy halving exhaustion). Run: just containers-reset" >&2
            exit 1
        fi
        just miner-mineblock 10 >/dev/null
        mined_topup=$((mined_topup + 10))
    done
    txid=$(just bitcoin-cli -rpcwallet=miner sendtoaddress "$address" "$btc")
    if [ "$confirm" = "true" ]; then
        just miner-mineblock 1 >/dev/null
    fi
    echo "$txid"

# Query the local dns_nostr_server via dig (honors DNS_NOSTR_BIND_ADDR and DNS_NOSTR_ZONE_ORIGIN)
dns-query label type='TXT':
    #!/usr/bin/env sh
    set -eu
    bind_addr="${DNS_NOSTR_BIND_ADDR:-0.0.0.0:1053}"
    case "$bind_addr" in
        \[*\]:*)
            host="${bind_addr#\[}"
            host="${host%%\]*}"
            port="${bind_addr##*\]:}"
            [ "$host" = "::" ] && host="::1"
            ;;
        *:*)
            host="${bind_addr%:*}"
            port="${bind_addr##*:}"
            [ "$host" = "0.0.0.0" ] && host="127.0.0.1"
            ;;
        *)
            echo "Invalid DNS_NOSTR_BIND_ADDR: $bind_addr" >&2
            exit 1
            ;;
    esac
    origin="${DNS_NOSTR_ZONE_ORIGIN:-nostr.dns.name.}"
    origin="${origin#.}"
    case "$origin" in
        *.) ;;
        *) origin="${origin}." ;;
    esac
    exec dig "@$host" -p "$port" "{{label}}.${origin}" "{{type}}"
