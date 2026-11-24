# Run wallet CLI subcommands (prepends `wallet` subcommand; preserves stdin and DNS_NOSTR_* env vars)
[positional-arguments]
wallet *args:
    #!/usr/bin/env sh
    set -eu
    exec cargo run -q -p dns_nostr_wallet -- wallet "$@"

# Run dns_nostr_server (honors all DNS_NOSTR_* environment variables)
[positional-arguments]
serve *args:
    #!/usr/bin/env sh
    set -eu
    exec cargo run -p dns_nostr_server -- "$@"

# Fund a regtest address from the local miner wallet and mine 1 confirmation block
[positional-arguments]
fund btc address:
    #!/usr/bin/env sh
    set -eu
    cli() {
        docker compose exec -T bitcoin-core bitcoin-cli -regtest -rpcuser=rpcuser -rpcpassword=rpcpassword "$@"
    }
    if ! cli listwallets | grep -Fq '"miner"'; then
        if cli listwalletdir | grep -Eq '"name"[[:space:]]*:[[:space:]]*"miner"'; then
            cli loadwallet "miner" >/dev/null
        else
            cli createwallet "miner" >/dev/null
        fi
    fi
    miner_addr=$(cli -rpcwallet=miner getnewaddress)
    height=$(cli getblockcount)
    if [ "$height" -lt 101 ]; then
        cli -rpcwallet=miner generatetoaddress $((101 - height)) "$miner_addr" >/dev/null
    fi
    txid=$(cli -rpcwallet=miner sendtoaddress "$2" "$1")
    cli -rpcwallet=miner generatetoaddress 1 "$miner_addr" >/dev/null
    confirmed_height=$(cli getblockcount)
    echo "Sent $1 BTC to $2 (txid: $txid, confirmed at height $confirmed_height)"

# Run unit tests (default), manage Docker test infrastructure (up/down), or run end-to-end tests (e2e)
[positional-arguments]
test *args:
    #!/usr/bin/env sh
    set -eu
    if [ "$#" -eq 0 ]; then
        exec cargo test --workspace
    fi
    if [ "$#" -eq 1 ]; then
        case "$1" in
            up)
                exec docker compose up -d --wait
                ;;
            down)
                exec docker compose down
                ;;
            e2e)
                just test up
                cargo build -p dns_nostr_server -p dns_nostr_wallet
                exec cargo run -p end_to_end
                ;;
        esac
    fi
    echo "Usage: just test [up|down|e2e]" >&2
    exit 1
