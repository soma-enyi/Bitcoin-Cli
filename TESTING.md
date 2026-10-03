# Testing the BTC-CLI Capstone Project

Complete guide to testing the Bitcoin CLI tool from the command line.

## Table of Contents

1. [Project Structure](#project-structure)
2. [Compilation & Build](#compilation--build)
3. [Testing CLI Commands](#testing-cli-commands)
4. [Testing TUI](#testing-tui)
5. [Running Tests](#running-tests)
6. [Full Test Examples](#full-test-examples)
7. [Troubleshooting](#troubleshooting)

---

## Project Structure

```
btc-cli/
├── crates/
│   ├── btc-core/       # Offline Bitcoin logic (no network I/O)
│   ├── btc-node/       # Bitcoin Core RPC connectivity
│   └── btc/            # CLI + TUI application
├── Cargo.toml          # Workspace definition
└── TESTING.md          # This file
```

### Crates:
- **btc-core** - Pure computation: keys, addresses, transactions, mnemonics
- **btc-node** - RPC client for Bitcoin Core communication
- **btc** - Main CLI/TUI application

---

## Compilation & Build

### Quick Check (Fast)
```bash
# Check if code compiles (no build artifacts)
cargo check --bin btc

# Expected output:
#   Checking btc v0.1.0
#   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.61s
```

### Debug Build
```bash
# Build with debug symbols (larger, slower)
cargo build --bin btc

# Binary location: target/debug/btc
# Run: ./target/debug/btc --help
```

### Release Build (Optimized)
```bash
# Build optimized for performance
cargo build --release --bin btc

# Binary location: target/release/btc
# Faster execution, smaller binary, stripped symbols
./target/release/btc --help
```

### Clean Build
```bash
# Remove all build artifacts
cargo clean

# Rebuild from scratch
cargo build --release --bin btc
```

---

## Testing CLI Commands

### 1. Check Help & Version

```bash
# Show all available commands
cargo run --quiet --bin btc -- --help

# Show version
cargo run --quiet --bin btc -- --version

# Show subcommand help
cargo run --quiet --bin btc -- key --help
cargo run --quiet --bin btc -- mnemonic --help
cargo run --quiet --bin btc -- derive --help
```

### 2. Generate Keys


```bash
# Generate a secp256k1 key pair
cargo run --quiet --bin btc -- key generate --type secp256k1

# Output format:
# {
#   "private_key": "...",
#   "public_key": "...",
#   "key_type": "secp256k1"
# }
```

### 3. Generate Mnemonics

```bash
# Generate 12-word mnemonic
cargo run --quiet --bin btc -- mnemonic new --words 12

# Generate 24-word mnemonic
cargo run --quiet --bin btc -- mnemonic new --words 24

# With passphrase
cargo run --quiet --bin btc -- mnemonic new --words 24 --passphrase "my-passphrase"
```

### 4. Convert Mnemonic to Extended Private Key

```bash
# Convert mnemonic to xprv (extended private key)
cargo run --quiet --bin btc -- mnemonic to-xprv \
  --mnemonic "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

# With passphrase
cargo run --quiet --bin btc -- mnemonic to-xprv \
  --mnemonic "abandon abandon..." \
  --passphrase "my-passphrase"
```

### 5. Derive Keys (BIP44)

```bash
# Derive keys from xprv
cargo run --quiet --bin btc -- derive \
  --key "xprv..." \
  --path "m/44'/0'/0'/0/0" \
  --count 5

# Default derivation: m/44'/0'/0'/0/0
# Count: how many consecutive addresses to derive
```

### 6. Generate Addresses

```bash
# Generate P2PKH address
cargo run --quiet --bin btc -- address from-key \
  --pubkey "03a1b2c3d4e5..." \
  --type p2pkh

# Generate P2WPKH address (SegWit)
cargo run --quiet --bin btc -- address from-key \
  --pubkey "03a1b2c3d4e5..." \
  --type p2wpkh

# Types: p2pkh, p2wpkh, p2sh, p2wsh
```

### 7. Decode Transactions

```bash
# Decode raw transaction hex
cargo run --quiet --bin btc -- tx decode \
  --hex "0100000001..."

# View transaction details
```

### 8. Query Blocks (Requires Bitcoin Core)

```bash
# Get block by height
cargo run --quiet --bin btc -- block get --height 800000

# Requires: Bitcoin Core running with RPC enabled
```

### 9. Estimate Fees (Requires Bitcoin Core)

```bash
# Estimate fee for 6-block confirmation
cargo run --quiet --bin btc -- fee estimate --target-blocks 6

# Fee in BTC/vB
# Requires: Bitcoin Core running

# Estimate from what recent blocks paid (median of per-block estimates)
cargo run --quiet --bin btc -- fee estimate --target 6 --block 850000 --blocks 6
cargo run --quiet --bin btc -- fee estimate --blocks 24     # window ends at the tip
# --block/--blocks cannot be combined with --fallback-rate
```

### 10. Check Node Status (Requires Bitcoin Core)

```bash
# Check Bitcoin Core connectivity
cargo run --quiet --bin btc -- node status

# Shows: sync progress, network info, etc.
# Requires: Bitcoin Core running
```

### 11. Show Configuration

```bash
# Display current configuration
cargo run --quiet --bin btc -- config

# Shows: network, RPC settings, etc.
```

### 12. Generate Shell Completions

```bash
# Bash completions
cargo run --quiet --bin btc -- completions bash

# Zsh completions
cargo run --quiet --bin btc -- completions zsh

# Fish completions
cargo run --quiet --bin btc -- completions fish
```

### 13. JSON Output Mode

```bash
# All commands support --json flag for machine-readable output
cargo run --quiet --bin btc -- --json key generate --type secp256k1

# Output is valid JSON, no colors, no formatting
```

---

## Testing TUI

### Launch the TUI

```bash
# Start the interactive terminal UI
cargo run --quiet --bin btc -- tui

# Or from release binary:
./target/release/btc tui
```

### Keyboard Controls

| Key | Action |
|-----|--------|
| `←` `→` | Navigate between tabs |
| `a-z 0-9` | Type input |
| `Backspace` | Delete character |
| `Enter` | Execute operation |
| `↑` `↓` | Adjust values (Fees tab) |
| `Tab` | Switch field (Fees and Send tabs) |
| `w` / `Ctrl+W` | Open the wallet dialog (`Ctrl+W` works inside text fields) |
| `Ctrl+U` | Clear the current field |
| `q` / `Esc` | Save the session and quit (`Esc` closes the wallet dialog first) |
| `Ctrl+C` | Save and force quit |

### Test Each Tab

```bash
# Tab 1: Keys & Mnemonic
# - Press Space to toggle mode
# - Press Enter to generate
# - See output in green box

# Tab 2: Derive (→)
# - Type xprv key
# - Adjust path
# - Press Enter to derive

# Tab 3: Addresses (→)
# - Type public key (hex)
# - Choose address type
# - Press Enter to generate

# Tab 4: Tx Decoder (→)
# - Paste transaction hex
# - Press Enter to decode

# Tab 5: Block Explorer (→)
# - Enter block height
# - Press Enter to fetch

# Tab 6: Fees (→)
# - Use ↑↓ to adjust blocks
# - Optionally type a Last block and Blocks read (Tab switches field)
# - Press Enter to estimate

# Tab 7: Send (→)
# - Press w to create or pick a wallet first
# - Fill address, amount, fee; Enter to build, Enter to continue
# - Type the WIF or recovery phrase, Enter to sign, Enter to broadcast

# Tab 8: Node Status (→)
# - Press Enter to check
# - Shows node connectivity
```

---

## Running Tests

### Unit Tests

```bash
# Run all tests
cargo test

# Run only TUI tests
cargo test --lib tuiapp

# Run specific test
cargo test --lib tuiapp test_tab_navigation

# Run with output
cargo test --lib tuiapp -- --nocapture

# Single-threaded (easier debugging)
cargo test --lib tuiapp -- --test-threads=1
```

### Code Quality

```bash
# Run Clippy (Rust linter)
cargo clippy --bin btc

# Strict mode
cargo clippy --bin btc -- -W clippy::all

# Check format
cargo fmt --check

# Auto-format code
cargo fmt
```

### Tests added for fees, wallets and the Send flow

All run offline against `MockBackend`; none needs Bitcoin Core.

| Area | What is checked |
|------|-----------------|
| Fee window (`btc-node` `fee_source`) | Percentile per target, relay-minimum floor, window size grows with the target, median ignores one odd block |
| `fee estimate` (`app::node`) | A block window estimates from those blocks; a window past the tip, or 0 blocks, is refused |
| Wallet store (`app::wallets`) | Add / select / remove / reload from disk; bad names, duplicates, wrong-network and legacy addresses rejected |
| Send (`app::send`) | The payment is built and the wallet key can sign it; more coins are used when one is not enough; a clear error when all are not; bad address, fee and amount rejected |
| TUI Send flow (`tuiapp::event`) | compose → review → key → confirm → done; the key is wiped once used; a wallet is required; mainnet needs a typed `yes`; switching wallet resets a half-built payment |
| Wallet dialog | `w` opens it and `q` closes it instead of quitting; add and switch wallets; a bad address keeps it open with an error |
| Recovery phrase (`wallet_modal`, fake clipboard and clock) | Defaults to 24 words, ↑↓ cycles 12-24; `c` copies; at 59 s nothing is cleared, at 61 s the screen is wiped and the clipboard cleared; the clipboard still clears after the dialog closes, on quit and on drop; a missing clipboard is reported; Enter/Esc/double Enter never close the phrase before `saved`; the phrase never reaches `wallets.json` or `session.json` |
| Stale results and key routing (`tuiapp::event`) | A job started before an edit, discard or wallet switch cannot overwrite the new payment; Esc backs out of the key step; a typed key is wiped when leaving the tab |
| Real transaction size (`btc-core` `tx::build`) | 1-in/2-out P2WPKH is 141 vB and 1-in/1-out 110 vB; taproot inputs are cheaper; absurd amounts are refused without overflow |
| Coin maturity (`app::send`) | Coinbase coins under 100 confirmations are skipped; exactly 100 is spendable |
| Mining (`app::node`, `tuiapp::event`) | A wallet without coins gets 101 blocks, one with coins gets 1; other networks refuse; `m` is a letter in text fields and Ctrl+O mines; a second press while mining is refused; the footer message expires |
| Real node (`#[ignore]`, needs `BTC_REGTEST_*`) | `getblockstats` and the full-block fallback agree; a wallet is funded, scanned, paid from with its phrase and gets change back |
| Send form (`tuiapp::send`) | Fields accept only what they can hold; the key is never printed (`Debug`) or serialized |
| Session (`tuiapp::session`) | Round trip restores the tab and fields; no key, xprv or PSBT reaches the file; a corrupt file is a fresh start |
| Drawing (`tuiapp::ui`) | Send tab, wallet dialog and Fees fields draw at 100x30 and at a tiny 20x5 terminal without panicking |

Run just these with e.g. `cargo test -p btc send`, `cargo test -p btc wallets`, `cargo test -p btc session`,
`cargo test -p btc-node fee_source`.

### Manual regtest walkthrough: sending from the TUI

```bash
# 1. Start regtest bitcoind (e.g. bitcoind -regtest -daemon) and open the TUI
BTC_NETWORK=regtest cargo run --quiet --bin btc -- tui

# 2. Press w, then n: name the wallet (e.g. alice). A 24-word recovery phrase is shown once:
#    press c to copy it, write it down, then type saved + Enter. The wallet is saved as a name + address only.
#    The footer now shows "Wallet: alice".

# 3. Fund that address (copy it from the Send tab) and confirm it:
bitcoin-cli -regtest -rpcwallet=<w> sendtoaddress <alice address> 1
bitcoin-cli -regtest generatetoaddress 101 <any address>

# 4. In the TUI go to Send (→). Fill: Pay to <regtest address>, Amount 25000, Fee 2.
#    Enter -> review the summary; Enter -> type the phrase (or WIF); Enter -> signed; Enter -> broadcast.
#    The result shows the txid. Mine a block and check with: bitcoin-cli -regtest gettransaction <txid>

# 5. Press q. Relaunch the TUI: it opens on the same tab with your address and amount restored,
#    but no key or phrase.
```
Also check: after pressing `c` the clipboard holds the phrase, and about 60 seconds later both the screen and the
clipboard are empty.

### Compilation Warnings

```bash
# Show all warnings
cargo check --bin btc -- -W rust-2021-compatibility

# Deny warnings
RUSTFLAGS="-D warnings" cargo check --bin btc
```

---

## Full Test Examples

### Example 1: Generate Address from Mnemonic

```bash
# Step 1: Generate mnemonic
cargo run --quiet --bin btc -- mnemonic new --words 12

# Output: [24 words]
# Copy the mnemonic

# Step 2: Convert to xprv
cargo run --quiet --bin btc -- mnemonic to-xprv \
  --mnemonic "abandon abandon abandon..."

# Output: xprv...
# Copy the xprv

# Step 3: Derive first address
cargo run --quiet --bin btc -- derive \
  --key "xprv..." \
  --path "m/44'/0'/0'/0/0" \
  --count 1

# Output: Public key

# Step 4: Generate address
cargo run --quiet --bin btc -- address from-key \
  --pubkey "03..." \
  --type p2wpkh

# Output: Bitcoin address (bc1q...)
```

### Example 2: Decode & Inspect Transaction

```bash
# Decode raw transaction
cargo run --quiet --bin btc -- tx decode \
  --hex "0100000001..."

# Shows:
# - Inputs
# - Outputs
# - Fee
# - Script details
```

### Example 3: Check Network Configuration

```bash
# Show current network
cargo run --quiet --bin btc -- config

# Output shows:
# - Network (mainnet, testnet, etc.)
# - RPC URL
# - Auth method
```

### Example 4: Query Bitcoin Core

```bash
# Prerequisites:
# 1. Bitcoin Core running
# 2. RPC enabled
# 3. Credentials set

# Check connection
cargo run --quiet --bin btc -- node status

# Get block
cargo run --quiet --bin btc -- block get --height 800000

# Estimate fees
cargo run --quiet --bin btc -- fee estimate --target-blocks 6
```

---

## Environment Variables

### Bitcoin Core RPC

```bash
# Set RPC URL
export BTC_RPC_URL="http://localhost:8332"

# Set username/password
export BTC_RPC_USER="bitcoin"
export BTC_RPC_PASSWORD="yourpassword"

# Or set cookie path (default: ~/.bitcoin/.cookie)
export BTC_RPC_COOKIE="/path/to/.cookie"

# Set network
export BTC_NETWORK="testnet"
```

### Example: RPC Connection

```bash
# Terminal 1: Start Bitcoin Core
bitcoind -testnet -server -rpcuser=bitcoin -rpcpassword=password

# Terminal 2: Test connection
cargo run --quiet --bin btc -- node status

# Should show: Connected, sync progress, etc.
```

---

## Offline Testing (No Bitcoin Core Required)

Most commands work offline. These don't need Bitcoin Core:

```bash
# Generate keys
cargo run --quiet --bin btc -- key generate --type secp256k1
cargo run --quiet --bin btc -- key generate --type ecdsa
cargo run --quiet --bin btc -- key generate --type schnorr

# Generate mnemonics
cargo run --quiet --bin btc -- mnemonic new --words 24

# Convert mnemonic to xprv
cargo run --quiet --bin btc -- mnemonic to-xprv --mnemonic "..."

# Derive keys
cargo run --quiet --bin btc -- derive --key "xprv..."

# Generate addresses
cargo run --quiet --bin btc -- address from-key --pubkey "03..."

# Decode transactions
cargo run --quiet --bin btc -- tx decode --hex "0100..."

# TUI (display, no operations)
cargo run --quiet --bin btc -- tui
```

---

## Troubleshooting

### Issue: "binary not found"

```bash
# Solution: Build first
cargo build --bin btc

# Then run from target directory
./target/debug/btc --help
# or
./target/release/btc --help (after: cargo build --release)
```

### Issue: "command not found: tui"

```bash
# Make sure you use -- before tui
cargo run --quiet --bin btc -- tui
# NOT: cargo run --bin btc tui
```

### Issue: TUI looks glitchy

```bash
# Terminal too small
# Minimum: 80 cols x 24 rows
echo "Cols: $COLUMNS, Rows: $LINES"

# Resize terminal to larger size
```

### Issue: Bitcoin Core connection fails

```bash
# 1. Check Bitcoin Core is running
bitcoind -testnet

# 2. Check RPC is enabled
# In ~/.bitcoin/bitcoin.conf or command line:
# server=1
# rpcuser=bitcoin
# rpcpassword=password

# 3. Set environment variables
export BTC_RPC_URL="http://localhost:18332"  # testnet
export BTC_RPC_USER="bitcoin"
export BTC_RPC_PASSWORD="password"

# 4. Test connection
cargo run --quiet --bin btc -- node status
```

### Issue: Tests fail to compile

```bash
# Clean and rebuild
cargo clean
cargo test --lib tuiapp

# If still fails, check:
cargo check --bin btc  # Verify basic compilation
```

---

## Quick Reference

### Most Common Commands

```bash
# Check compilation
cargo check --bin btc

# Generate key
cargo run --quiet --bin btc -- key generate --type secp256k1

# Generate mnemonic
cargo run --quiet --bin btc -- mnemonic new --words 24

# Launch TUI
cargo run --quiet --bin btc -- tui

# Run tests
cargo test --lib tuiapp

# Build release
cargo build --release --bin btc
```

### Test Checklist

- [ ] `cargo check --bin btc` passes
- [ ] `cargo test --lib tuiapp` passes
- [ ] `cargo run --quiet --bin btc -- key generate` works
- [ ] `cargo run --quiet --bin btc -- mnemonic new --words 24` works
- [ ] `cargo run --quiet --bin btc -- tui` launches without panic
- [ ] TUI: All tabs navigate (← →)
- [ ] TUI: Input works (type a-z)
- [ ] TUI: Quit works (press q)

---

## Additional Resources

- **CLI Guide:** See `cargo run --quiet --bin btc -- --help`
- **Module Documentation:** View crate READMEs
- **Test Files:** Look in `crates/btc/tests/` and `src/tuiapp/`

---

**Last Updated:** 2026-09-30  
**Project:** Bitcoin CLI Capstone  
**Language:** Rust  
**Status:** Production Ready ✅

 db000eef389d259d8a4354f61eee02c85ea98deafc85c75269d932f805c994b0 
 