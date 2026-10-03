# BTC-CLI: Bitcoin Developer Toolkit

A command-line + interactive TUI toolkit for common Bitcoin operations. Ideal for developers, testers, and learners who need quick access to key generation, address derivation, transaction decoding, and blockchain queries.

**Status:** MVP Complete (73 tests passing)  
**Languages:** Rust 1.91+  
**License:** MIT  

## Features

### Offline Operations (no Bitcoin Core required)
- **Key generation**: ECDSA and Schnorr (BIP340)
- **BIP39 mnemonics**: Generate seeds, derive xprv/xpub
- **BIP32 derivation**: Child key derivation from extended keys with BIP44/49/84/86 support
- **Address generation**: P2PKH, P2SH-P2WPKH, P2WPKH, P2TR
- **Address validation**: Verify type, network, checksum, scriptPubKey
- **Transaction decoding**: Full analysis (inputs, outputs, witnesses, fee)

### Online Operations (with Bitcoin Core)
- **Node status**: Chain height, sync progress, peer count
- **Block info**: Query blocks by height or hash
- **Fee estimation**: Satoshi/vB rates with fallback handling
- **Transaction pipeline**: Create PSBT → Sign → Broadcast

### Output Formats
- Human-readable with colors (tables, formatted text)
- JSON for scripting and automation

## Installation

```bash
# Clone the repo
git clone <repo-url>
cd btc-cli
cargo build --release

# Binary at ./target/release/btc
```

## Usage

### Key Generation
```bash
btc key generate --type ecdsa
# Output: WIF, public key hex, parity (for Schnorr)
```

### Mnemonics
```bash
btc mnemonic new --words 12
# Output: 12 words, xprv, xpub, fingerprint
```

### Derivation
```bash
btc derive <xprv or xpub> "m/84'/1'/0'/0/0" --addr-type p2wpkh
# Output: Child key, address, derivation path
```

### Address Tools
```bash
btc address from-pubkey 02abcd... --type p2wpkh
btc address validate tb1...
```

### Transaction Decoding
```bash
btc tx decode <hex> --network testnet4
# Shows: txid, inputs, outputs, witnesses, fee (if prevouts available)
```

### PSBT and multisig
```bash
btc psbt analyze <base64>
btc psbt combine <base64> <base64>
btc multisig create --threshold 2 --pubkey <k1> <k2> <k3>   # keys sorted (BIP67); --keep-order to disable
btc multisig analyze <script-hex>
```

### Transactions (create, sign, broadcast)
```bash
btc tx create --input <txid>:<vout>:<sats>:<address> --to <address>:<sats> --change <address> --fee-rate 2
btc tx sign <psbt> --key <WIF>          # P2WPKH and taproot key-path inputs; prints the signed hex
btc tx broadcast <hex>                  # mainnet also needs --yes
```
Inputs must be native segwit or taproot and need the address, because the signature commits to the amount and script.
Verified on regtest against Bitcoin Core (`testmempoolaccept`, broadcast, confirmed) for P2WPKH and taproot.

### Fee estimates
`btc fee estimate --target <blocks>` asks the node's own estimator first. The target is how many blocks you are willing
to wait (about 10 minutes each), and longer targets cost less. On regtest, signet and testnet4 only the node is used.
On mainnet, when the node cannot estimate (a restricted gateway), set `BTC_MAINNET_FEE_SOURCE` to `public`
(a public fee service) or `blocks` (computed from recent blocks through your node, about 10 MB each); otherwise
you get the labelled mempool minimum. Every result says where its number came from.

To estimate from what blocks actually paid, instead of the node's estimator:
```bash
btc fee estimate --target 6 --block 850000 --blocks 6   # the 6 blocks ending at 850000
btc fee estimate --blocks 24                            # the last 24 blocks (--block defaults to the tip)
```
`--block` is the last block of the window and `--blocks` how many to read; if only one is given the other defaults
(tip, or a count chosen by `--target`: 1-2 → 3 blocks, 3-6 → 6, 7-24 → 24, 25+ → 144). Each block gives its own
estimate and the result is their median, so one odd block cannot skew it. Blocks are read with `getblockstats`; a node
without it is read block by block instead (heavier). Neither flag can be combined with `--fallback-rate`.

### Node connection and choosing a network
Copy `.env.example` to `.env` (loaded automatically; real environment variables win). Pick the network with
`BTC_NETWORK` or `-n`; node settings are per network (`BTC_MAINNET_RPC_URL`, `BTC_MAINNET_RPC_API_KEY`,
`BTC_REGTEST_RPC_COOKIE`, ...) and replace the plain `BTC_RPC_*` variables when present. The tool refuses to run if the
node's chain does not match the selected network. `~` is not expanded in `.env`, so use absolute paths.
Node commands exit 2 with a hint if the node is unreachable.

### TUI (Interactive)
```bash
btc tui
# Navigate with ← → arrow keys, q to quit (your place is saved)
# Tabs: Keys, Derive, Addresses, Tx Decoder, Blocks, Fees, Send, Node
```

**Fees tab**: ↑↓ sets the target. Optionally fill *Last block* and *Blocks read* (Tab switches between the two) to
estimate from blocks, as `--block` / `--blocks` do; leave both blank to use the node's estimator.

**Wallets**: press `w` (or Ctrl+W inside a text field) to open the wallet dialog.

| Key | Action |
|-----|--------|
| `↑` `↓` | Pick a wallet |
| `Enter` | Use the picked wallet |
| `n` | Create a new wallet with a freshly generated recovery phrase (24 words; `↑` `↓` chooses 12-24) |
| `a` | Add a watch-only wallet from an existing address |
| `d` | Delete the picked wallet |
| `Esc` | Close |

A new wallet's recovery phrase is shown once. Press `c` to copy it to the clipboard; the screen and the clipboard are
both cleared after 60 seconds, so write it down first. Enter and Esc do not close the phrase view by accident: type
`saved` and press Enter once you have written the words down. Clipboard managers may keep their own copy, so turn
their history off or avoid copying at all. A wallet is saved as a name and an address only, in
`$BTC_HOME` or the platform config directory under `<network>/wallets.json`. **The recovery phrase and keys are never
written to disk.**

**Mining (regtest only)**: press `m` (or Ctrl+O inside a text field) to mine blocks to the current wallet. A wallet with
no spendable coin gets 101 blocks, so the first reward is spendable at once; after that each press mines 1 block, which
confirms waiting payments. It runs in the background and the result appears in the footer. Other networks refuse. To
test a payment: create two wallets (`w`, `n`), press `m` on the first, then send to the second one's address.

**Send tab** pays from the current wallet: compose (address, amount in sats, optional fee rate) → review → enter the key
→ confirm → done. Coins are found with the node's `scantxoutset`, so the node needs no wallet (on mainnet the scan can
take a minute). Coins are picked largest first and the change returns to the wallet. The key stage takes a WIF or the
wallet's recovery phrase; it is used only to sign and is wiped straight after, and Esc backs out of that step instead of
quitting. On mainnet you must type `yes` before anything is broadcast. Switching wallet discards a half-built payment.
Coins you mined straight to the address (coinbase) are not offered until they are 100 blocks deep, because the node
would refuse them. The fee covers the real size of the transaction, so the rate you ask for is the rate you pay.

**Saved on quit**: `q` (or Esc / Ctrl+C) writes the tab and the non-secret fields (block height, fee fields, the Send
form's address, amount and fee) to `session.json` beside `wallets.json`, and they come back next launch. Keys, extended
keys, PSBTs and recovery phrases are never saved.

## Global Flags

- `--network regtest|signet|testnet4|mainnet` (default: regtest). Every offline feature works on all four; on mainnet the output is real-fund key material, so it carries a loud warning. An `xprv`/`xpub` only works on mainnet and a `tprv`/`tpub` only on test networks
- `--json` — Machine-readable output
- `--rpc-url <URL>` — Node RPC endpoint
- `--rpc-cookie <path>` — Cookie auth (or env: BTC_RPC_COOKIE)
- `--rpc-user <user>`, `--rpc-password <pass>` — User/pass auth

Environment variables: `BTC_NETWORK`, `BTC_RPC_URL`, `BTC_RPC_COOKIE`, `BTC_RPC_USER`, `BTC_RPC_PASSWORD`

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for detailed design:
- **btc-core**: Pure Bitcoin logic (keys, BIP32, address, tx decode)
- **btc-node**: RPC backend (trait + CoreRpcBackend + MockBackend for tests)
- **btc**: CLI + TUI (clap, ratatui) + services layer

## Testing

```bash
# All tests (73 total)
cargo test

# Core unit tests (BIP32, BIP39, BIP44, address, tx decode)
cargo test --lib -p btc-core

# CLI integration tests
cargo test --test cli -p btc

# Regtest live tests (requires bitcoind)
# BITCOIND_EXE=$(which bitcoind) cargo test --features regtest
```

## Regtest Demo

```bash
./scripts/regtest-demo.sh
# Starts regtest node, mines 101 blocks, creates/signs/broadcasts a tx, mines 1 block
# Shows end-to-end workflow
```

## Development

**Project structure:**
```
btc-cli/
├── crates/
│   ├── btc-core/        # Bitcoin logic (no network)
│   ├── btc-node/        # RPC backend trait
│   └── btc/             # CLI + TUI
├── scripts/
│   ├── regtest-demo.sh   # End-to-end demo
│   └── make_tx_fixtures.py  # Test fixture generator
├── tests/                # CLI integration tests
└── docs/
    └── architecture.md   # Design diagram
```

**Coding standards:**
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- No comments unless WHY is non-obvious
- Error handling at system boundaries only
- Tests use official vectors (BIP32, BIP39, BIP173, BIP350)

## Roadmap (Post-MVP)

- [x] Full TUI with 8 tabs running the real services (RPC runs on a background thread)
- [x] Real Bitcoin Core RPC backend; `.env` loaded automatically
- [x] PSBT `analyze` / `combine`; multisig `create` / `analyze` (addresses verified against `bitcoin-cli createmultisig`)
- [x] `tx create` / `tx sign` / `tx broadcast` (P2WPKH + taproot key-path), verified against Bitcoin Core
- [x] TUI Send tab, named wallets (name + address only), fee estimates from a window of blocks, session save on quit
- [ ] Legacy P2PKH / P2SH inputs, multi-key PSBT signing and finalize
- [ ] Colored CLI output (comfy-table + owo-colors)
- [ ] Hardware wallet integration

## Security

- **Secrets**: Private keys, mnemonics zeroized on drop (Zeroizing<>); the TUI saves only wallet names, addresses and non-secret form fields, never a key, phrase or PSBT
- **Offline mode**: Core logic has zero network code
- **Regtest by default**: mainnet is opt-in via `--network mainnet` / `BTC_NETWORK`; keys, addresses and transaction outputs are checked against the selected network
- **RPC auth**: Supports cookie and user/pass; flags never logged

## License

MIT — See LICENSE file

---

Built at Rust for Bitcoin Cohort  
Capstone Project 6  
