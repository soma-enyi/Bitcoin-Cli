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
| `q` | Quit TUI |
| `Ctrl+C` | Force quit |

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
# - Press Enter to estimate

# Tab 7: Node Status (→)
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
