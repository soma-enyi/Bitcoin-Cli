# BTC-CLI Demo Script (3-5 min video)

## Setup
```bash
# Build the project
cargo build --release
btc --version
btc --help
```

## Scene 1: Offline Operations (~1.5 min)

**"Let's generate a Bitcoin key and derive some addresses."**

```bash
# Key generation
btc key generate --type ecdsa

# Generate mnemonic
btc mnemonic new --words 24

# Derive a child key
btc mnemonic new --words 12 | btc mnemonic to-xprv -
# Copy xprv from above, then:
btc derive "m/84'/1'/0'/0/0" --addr-type p2wpkh

# Validate the address we just generated
btc address validate tb1q... # from above
```

**Voice-over:** "All of these operations run offline—no blockchain required. That's why they're instant."

## Scene 2: Address Types (~1 min)

**"BTC-CLI supports all major address types."**

```bash
# Show all address types from one key
btc key generate --json | jq -r .public_key | xargs -I {} btc address from-pubkey {} --type all
```

**Voice-over:** "P2PKH for legacy, P2SH for backward-compatible SegWit, P2WPKH for native SegWit, and P2TR for Taproot."

## Scene 3: Transaction Decoding (~1 min)

**"Here's a real transaction from Bitcoin Core."**

```bash
# Paste a real tx hex
btc tx decode 0100...

# With JSON for scripting
btc tx decode 0100... --json | jq .inputs[0]
```

**Voice-over:** "We can decode any transaction, see all the inputs, outputs, witness data, and calculate the fee."

## Scene 4: TUI (~1 min)

**"There's also an interactive mode."**

```bash
btc tui
# Navigate: ← → arrow keys, q to quit
# Show tabs: Keys & Mnemonic, Derive, Addresses, etc.
```

**Voice-over:** "The TUI reuses all the same logic as the CLI—everything is accessible interactively."

## Scene 5: JSON Output (~30 sec)

**"All output is available as JSON for automation."**

```bash
btc mnemonic new --words 12 --json | jq .
btc config --json
```

**Voice-over:** "This makes it easy to integrate BTC-CLI into shell scripts and larger toolchains."

## Scene 6: Demo End

```bash
btc --help  # show the full tree again
```

**Voice-over:** "That's BTC-CLI—a developer's Swiss army knife for Bitcoin. All 13 MVP commands, fully tested, usable offline or with a local node."

---

## Recording Notes

- Use a 120-column terminal, 14pt monospace font for readability
- Slow down typing (1 char/100ms) so viewers can follow
- Use `sleep 1` between commands to let output settle
- Capture at 1080p 30fps or higher
- Add subtitle overlays with command descriptions
- Background: quiet, no distractions
- Keep the whole demo under 5 minutes (5:00 is the max)
