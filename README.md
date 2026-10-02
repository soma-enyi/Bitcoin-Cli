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

### Node connection and choosing a network
Copy `.env.example` to `.env` (loaded automatically; real environment variables win). Pick the network with
`BTC_NETWORK` or `-n`; node settings are per network (`BTC_MAINNET_RPC_URL`, `BTC_MAINNET_RPC_API_KEY`,
`BTC_REGTEST_RPC_COOKIE`, ...) and replace the plain `BTC_RPC_*` variables when present. The tool refuses to run if the
node's chain does not match the selected network. `~` is not expanded in `.env`, so use absolute paths.
Node commands exit 2 with a hint if the node is unreachable.

### TUI (Interactive)
```bash
btc tui
# Navigate with ← → arrow keys, q to quit
# Tabs: Keys, Derive, Addresses, Tx Decoder, Blocks, Fees, Node
```

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

- [x] Full TUI with 7 tabs running the real services (RPC runs on a background thread)
- [x] Real Bitcoin Core RPC backend; `.env` loaded automatically
- [x] PSBT `analyze` / `combine`; multisig `create` / `analyze` (addresses verified against `bitcoin-cli createmultisig`)
- [x] `tx create` / `tx sign` / `tx broadcast` (P2WPKH + taproot key-path), verified against Bitcoin Core
- [ ] Legacy P2PKH / P2SH inputs, multi-key PSBT signing and finalize
- [ ] Colored CLI output (comfy-table + owo-colors)
- [ ] Hardware wallet integration

## Security

- **Secrets**: Private keys, mnemonics zeroized on drop (Zeroizing<>)
- **Offline mode**: Core logic has zero network code
- **Regtest by default**: mainnet is opt-in via `--network mainnet` / `BTC_NETWORK`; keys, addresses and transaction outputs are checked against the selected network
- **RPC auth**: Supports cookie and user/pass; flags never logged

## License

MIT — See LICENSE file

---

Built at Rust for Bitcoin Cohort  
Capstone Project 6  
