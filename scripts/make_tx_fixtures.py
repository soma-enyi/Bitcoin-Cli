#!/usr/bin/env python3
"""Regenerates crates/btc-core/testdata/regtest_txs.json from a real Bitcoin Core node.

Builds, on a throwaway regtest node:
  funding       one input, outputs of every type (p2pkh, p2sh-p2wpkh, p2wpkh, p2tr)
  mixed_inputs  spends one output of each type, RBF, OP_RETURN "hello btc-cli"
  coinbase      the coinbase of the block that confirmed mixed_inputs
and records Core's own decode (txid, wtxid, size, vsize, weight, prevouts, fee) as the
expected values for the decoder tests.

Usage:
  DATADIR=$(mktemp -d)
  bitcoind -regtest -datadir=$DATADIR -daemon -fallbackfee=0.0002 -rpcport=28443 -port=28444 -txindex=1
  bitcoin-cli -regtest -datadir=$DATADIR -rpcport=28443 createwallet w
  bitcoin-cli -regtest -datadir=$DATADIR -rpcport=28443 generatetoaddress 101 \\
      $(bitcoin-cli -regtest -datadir=$DATADIR -rpcport=28443 getnewaddress)
  python3 scripts/make_tx_fixtures.py $DATADIR crates/btc-core/testdata/regtest_txs.json
  bitcoin-cli -regtest -datadir=$DATADIR -rpcport=28443 stop

Regenerating changes every txid, so update the txid asserted in crates/btc/tests/cli.rs.
"""
import json, subprocess, sys
S = sys.argv[1]; OUT = sys.argv[2]
def cli(*args, wallet=True):
    cmd = ["bitcoin-cli", "-regtest", f"-datadir={S}", "-rpcport=28443"] + (["-rpcwallet=w"] if wallet else []) + [str(a) for a in args]
    out = subprocess.check_output(cmd).decode().strip()
    try: return json.loads(out)
    except json.JSONDecodeError: return out

addrs = {t: cli("getnewaddress", "", t) for t in ["legacy", "p2sh-segwit", "bech32", "bech32m"]}
fund_txid = cli("send", json.dumps({a: 0.5 for a in addrs.values()}))["txid"]
cli("generatetoaddress", 1, cli("getnewaddress"), wallet=False)
fund = cli("getrawtransaction", fund_txid, 2, wallet=False)
inputs = [{"txid": fund_txid, "vout": o["n"]} for o in fund["vout"] if o["scriptPubKey"].get("address") in addrs.values()]
assert len(inputs) == 4
dest = cli("getnewaddress", "", "bech32m")
psbt = cli("walletcreatefundedpsbt", json.dumps(inputs),
           json.dumps([{dest: 1.9}, {"data": b"hello btc-cli".hex()}]),
           0, json.dumps({"add_inputs": False, "replaceable": True, "fee_rate": 5}))["psbt"]
signed = cli("walletprocesspsbt", psbt)["hex"]
mixed_txid = cli("sendrawtransaction", signed, wallet=False)
mixed = cli("getrawtransaction", mixed_txid, 2, wallet=False)
mixed_fee = cli("getmempoolentry", mixed_txid, wallet=False)["fees"]["base"]
blockhash = cli("generatetoaddress", 1, cli("getnewaddress"), wallet=False)[0]
coinbase_txid = cli("getblock", blockhash, wallet=False)["tx"][0]
coinbase = cli("getrawtransaction", coinbase_txid, 2, wallet=False)
mixed = cli("getrawtransaction", mixed_txid, 2, blockhash, wallet=False)

def fixture(tx, fee_btc=None):
    f = {k: tx[k] for k in ["hex", "txid", "hash", "size", "vsize", "weight", "version", "locktime"]}
    f["vin"] = [{"sequence": i["sequence"], "prevout": ({"value_sats": round(i["prevout"]["value"] * 1e8), "script_pubkey": i["prevout"]["scriptPubKey"]["hex"], "type": i["prevout"]["scriptPubKey"]["type"]} if "prevout" in i else None)} for i in tx["vin"]]
    f["vout"] = [{"value_sats": round(o["value"] * 1e8), "type": o["scriptPubKey"]["type"], "address": o["scriptPubKey"].get("address"), "asm": o["scriptPubKey"]["asm"]} for o in tx["vout"]]
    if fee_btc is not None: f["fee_sats"] = round(fee_btc * 1e8)
    return f

fixtures = {"funding": fixture(fund, fund.get("fee")), "mixed_inputs": fixture(mixed, mixed_fee), "coinbase": fixture(coinbase),
            "coinbase_height": cli("getblock", blockhash, wallet=False)["height"]}
json.dump(fixtures, open(OUT, "w"), indent=2)
for k in ["funding", "mixed_inputs", "coinbase"]:
    f = fixtures[k]; print(k, f["size"], f["vsize"], f["weight"], [v["prevout"]["type"] if v["prevout"] else None for v in f["vin"]], [o["type"] for o in f["vout"]], f.get("fee_sats"))
