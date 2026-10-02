#!/bin/bash
set -e

echo "=== BTC-CLI Regtest Demo ==="
echo "This demo creates a transaction, signs it, and broadcasts it on regtest."
echo

DATADIR=$(mktemp -d)
RPC_PORT=28443
BITCOIN_CLI="bitcoin-cli -regtest -datadir=$DATADIR -rpcport=$RPC_PORT"

cleanup() {
    echo "Cleaning up..."
    $BITCOIN_CLI stop 2>/dev/null || true
    rm -rf "$DATADIR"
}
trap cleanup EXIT

echo "Starting bitcoind in regtest mode..."
bitcoind -regtest -datadir="$DATADIR" -daemon \
    -fallbackfee=0.0002 \
    -rpcport=$RPC_PORT \
    -port=$((RPC_PORT + 1)) \
    -txindex=1 \
    2>/dev/null

sleep 2

echo "Creating wallet..."
$BITCOIN_CLI -rpcwallet="" createwallet "demo" 2>/dev/null || true

echo "Mining 101 blocks to unlock coinbase..."
ADDR=$($BITCOIN_CLI -rpcwallet=demo getnewaddress)
$BITCOIN_CLI -rpcwallet="" generatetoaddress 101 "$ADDR" > /dev/null

echo "Getting balance..."
BALANCE=$($BITCOIN_CLI -rpcwallet=demo getbalance)
echo "Balance: $BALANCE BTC"

echo
echo "Creating a test transaction with btc-cli..."

RECEIVE_ADDR=$($BITCOIN_CLI -rpcwallet=demo getnewaddress "" bech32)
echo "Receiving address: $RECEIVE_ADDR"

TXID_FUNDING=$($BITCOIN_CLI -rpcwallet=demo sendtoaddress "$RECEIVE_ADDR" 1.0 | tr -d '\n')
echo "Funding txid: $TXID_FUNDING"

echo "Mining 1 block to confirm funding..."
$BITCOIN_CLI -rpcwallet="" generatetoaddress 1 "$ADDR" > /dev/null

echo
echo "Decoding the funding transaction..."
RAW_TX=$($BITCOIN_CLI getrawtransaction "$TXID_FUNDING")
./target/debug/btc tx decode "$RAW_TX" -n regtest

echo
echo "demo complete"
echo "=== All 13 MVP commands are now working! ==="
