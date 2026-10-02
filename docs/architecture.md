# BTC-CLI Architecture

## Core Principle

**"A command never prints. It returns a data struct. A separate layer decides how to show it."**

```
                  ┌──────────── Frontend ────────────┐
   argv ──►   clap CLI              ratatui TUI
    (args)   (crates/btc/cli/)      (crates/btc/tuiapp/)
                └──────────┬──────────────┬──────────┘
                           ▼              ▼
                ┌──────────────────────────────────┐
                │ Services / Use Cases (app/)      │ ← Business logic
                │ Returns: Serialize structs       │ ← Render-agnostic
                └──────────┬──────────────┬────────┘
                           ▼              ▼
     ┌──────────────────────────┐  ┌───────────────────┐
     │ btc-core (offline)       │  │ btc-node (RPC)    │
     │ ├─ keys: key gen         │  │ ├─ NodeBackend    │ ← trait
     │ ├─ mnemonic: BIP39       │  │ ├─ CoreRpcBackend │ ← corepc-client
     │ ├─ derive: BIP32         │  │ ├─ MockBackend    │ ← tests
     │ ├─ address: all types    │  │ └─ config/auth    │
     │ └─ tx: decode/build/sign │  └───────────────────┘
     └──────────────────────────┘           │
            │ (returns core types)          │
            └──────────────────────────────┬┘
                          ▼
              ┌────────────────────────┐
              │ Output Layer           │
              │ ├─ Render trait        │
              │ ├─ human: Table/Text   │
              │ └─ json: serde_json    │
              └────────────────────────┘
                        ▼
                   stdout/stderr
```

## Dependency Rules

**Important:** Dependencies only flow downward. Never circular.

```
btc (CLI/TUI)
  ↓
  app/          ← services glue everything
  ↓
  btc-core      ← pure logic, zero network, zero I/O
  ↓
  bitcoin crate ← external
```

This ensures:
1. **btc-core has zero network dependencies** → easy to test offline, fast CI
2. **btc-node isolation** → RPC crate can be swapped (bitcoincore-rpc → corepc-client → custom)
3. **JSON output is free** → every service returns Serialize
4. **TUI reuses everything** → only rendering differs

## Module Structure

### btc-core: Bitcoin Logic

**Purpose:** Pure, deterministic Bitcoin operations. No I/O.

```
btc-core/src/
├── lib.rs                  ← re-exports
├── network.rs              ← Network enum (Regtest, Signet, Testnet4)
├── keys.rs                 ← Key generation (ECDSA, Schnorr)
├── mnemonic.rs             ← BIP39 seed generation
├── derive.rs               ← BIP32 derivation (hardened, unhardened, watch-only)
├── address.rs              ← Address generation & validation (P2PKH, P2SH-P2WPKH, P2WPKH, P2TR)
├── script.rs               ← Script classification (pay-to-pubkey, multisig, OP_RETURN, etc.)
├── tx/
│   ├── decode.rs           ← Transaction parsing & analysis (inputs, outputs, witnesses)
│   ├── build.rs            ← PSBT creation with fee math (RBF, dust checks)
│   └── sign.rs             ← PSBT signing & finalization
└── error.rs                ← CoreError (thiserror)
```

**Tests:** 42 total
- Official BIP32, BIP39, BIP44, BIP173, BIP350 vectors
- All address types on all networks
- Real regtest transactions (vs. Bitcoin Core decode)

### btc-node: RPC Backend

**Purpose:** Abstract the node connection; enable mocking.

```
btc-node/src/
├── lib.rs
├── backend.rs              ← NodeBackend trait { node_status, block_info, fee_estimate }
├── rpc.rs                  ← CoreRpcBackend: connects via corepc-client
├── mock.rs                 ← MockBackend: returns fixed data (100 blocks, 100% sync, etc.)
├── config.rs               ← Config: network defaults, port resolution
└── error.rs                ← NodeError (thiserror)
```

**Design:**
- `NodeBackend` trait keeps RPC crate swappable
- `CoreRpcBackend` uses corepc-client v30 (matches Core v30)
- `MockBackend` for tests (no bitcoind required)
- Config resolves flags > env vars > defaults

### btc: CLI & TUI

**Purpose:** User interfaces + service glue.

```
btc/src/
├── main.rs                 ← Entry point
├── cli/                    ← clap command definitions
│   ├── mod.rs              ← Cli, GlobalArgs, Command enum
│   ├── key.rs              ← KeyCmd: generate
│   ├── mnemonic.rs         ← MnemonicCmd: new, to-xprv
│   ├── derive.rs           ← DeriveArgs: derive + count + addr-type
│   ├── address.rs          ← AddressCmd: from-pubkey, validate
│   ├── tx.rs               ← TxCmd: decode, create, sign, broadcast
│   ├── block.rs            ← BlockCmd: info
│   ├── fee.rs              ← FeeCmd: estimate
│   └── node.rs             ← NodeCmd: status
├── dispatch.rs             ← Routes CLI → services → output
├── app/                    ← Business logic services
│   ├── mod.rs              ← pub fn show_config, etc.
│   ├── keys.rs             ← pub fn generate(ctx)
│   ├── mnemonic.rs         ← pub fn new(ctx, words)
│   ├── derive.rs            ← pub fn derive(ctx, key, path)
│   ├── address.rs          ← pub fn from_pubkey, validate
│   ├── tx.rs               ← pub fn decode, create, sign, broadcast
│   ├── node.rs             ← pub fn status, block_info, fee_estimate
│   ├── input.rs            ← secret_arg, text_arg, stdin handling
│   ├── context.rs          ← Context { network, output_mode, rpc_config }
│   └── error.rs            ← AppError: Input, Core, Node, Io, Config
├── output/                 ← Rendering layer
│   ├── mod.rs              ← Render trait, emit(), emit_error()
│   └── ...                 ← (human, json, colored tables come here)
└── tuiapp/                 ← Interactive terminal (ratatui)
    ├── app.rs              ← AppState, Tab enum
    ├── event.rs            ← Key event handling
    ├── ui.rs               ← Draw frame
    └── mod.rs              ← Event loop
```

**Service Pattern:**
Each `app/X.rs` has:
```rust
pub fn some_command(ctx: &Context, args...) -> Result<OutputType, AppError> {
    // Business logic: use btc-core, btc-node, validate
    // Return serializable struct with Render impl
}
```

**Render Trait:**
```rust
pub trait Render: Serialize {
    fn render_human(&self) -> String;  // Pretty format
    fn contains_secrets(&self) -> bool { false }  // Warn if needed
}
```

Every output type implements this → `--json` is automatic.

## Data Flow Examples

### Example 1: `btc key generate`

```
CLI parse: KeyCmd::Generate { key_type: Schnorr }
         ↓
dispatch: app::keys::generate(ctx, Schnorr)
         ↓
         (calls btc_core::keys::generate)
         ↓
         Returns: KeyPairInfo { network, key_type, wif, hex, public_key, parity }
         ↓
         impl Render for KeyPairInfo:
           - render_human(): pretty-printed table
           - contains_secrets(): true (warn on stderr)
         ↓
output::emit(result, OutputMode::Human | Json)
         ↓
stdout: formatted output
stderr: "warning: this output contains secret key material"
```

### Example 2: `btc tx decode <hex> --prevouts-from-node`

```
CLI parse: TxCmd::Decode { hex: "0100...", prevouts_from_node: true }
         ↓
dispatch: app::tx::decode(ctx, hex, true)
         ↓
         (if prevouts_from_node: call ctx.node.block_info() for scriptPubKeys)
         ↓
         btc_core::tx::decode::decode(tx, network, prevouts)
         ↓
         Returns: DecodedTx { inputs: [...], outputs: [...], fee_sats, ... }
         ↓
output::emit(result, OutputMode::Json)
         ↓
stdout: {"inputs": [...], "outputs": [...], "fee_sats": 1234}
```

## Testing Strategy

### Unit Tests (40 in btc-core)
- BIP32 test vectors (derivation chain)
- BIP39 test vectors (entropy → words → seed)
- BIP44/49/84/86 first-address vectors
- BIP173/350 address vectors
- Real regtest tx fixtures (vs. Bitcoin Core decode)

**Run:** `cargo test -p btc-core`

### CLI Integration Tests (31 in btc)
- `assert_cmd`: Invoke binary, check exit code, stdout, stderr
- `insta`: Snapshot golden outputs
- Coverage: all commands, error cases, json/human modes, env vars

**Run:** `cargo test --test cli`

### Mock Tests (2 in btc-node)
- Config defaults
- MockBackend returns sensible data

**Run:** `cargo test -p btc-node`

### Live Regtest Tests (optional, gated)
- Requires bitcoind
- Creates wallets, mines blocks, funds addresses
- Tests full pipeline: create PSBT → sign → broadcast → mine → confirm

**Run:** `BITCOIND_EXE=$(which bitcoind) cargo test --features regtest`

## Error Handling

**CoreError** (btc-core):
- `InvalidInput("...")` — bad format, bad value
- `Mnemonic(bip39::Error)` — bad words, checksum
- `Bip32(bitcoin::bip32::Error)` — bad path, key derivation failed
- `InvalidAddress(...)` — bad encoding, bad checksum
- `WrongNetwork { address, actual, selected }` — address on wrong network

**AppError** (btc):
- `NotImplemented(&'static str)` — feature not done yet
- `Config(String)` — flags/env vars invalid
- `Input(String)` — user input error (parsed from CoreError)
- `Io(io::Error)` — file read, network error
- `Core(CoreError)` — from btc-core
- `Node(NodeError)` — from btc-node
- `Serialize(serde_json::Error)` — JSON encoding failed

**Exit codes:**
- `0` — Success
- `1` — User error (invalid input, config, not implemented)
- `2` — Network/node error (RPC unreachable, insufficient data)

## Security Boundaries

**Never cross into btc-core from outside:**
- No shell expansion
- No file paths (except --key-file which is app's job)
- No network calls

**Secret handling (app layer):**
- Read from stdin or file only (never argv)
- Wrap in `Zeroizing<String>`
- Warn on stderr if output contains secrets
- Never log or debug-print secrets

**Node layer (btc-node):**
- Blocking RPC (worker threads in TUI, not yet implemented)
- MockBackend for tests (no bitcoind needed in CI)
- Config auth: cookie + user/pass, validated after parsing

---

**Principle:** Each layer owns one concern. btc-core doesn't know about networks. app doesn't print. output formats are interchangeable.
