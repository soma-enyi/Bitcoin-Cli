# BTC-Cli — Project Requirements & Build Plan

A command-line toolkit (plus a ratatui TUI) for common Bitcoin tasks: keys, mnemonics, derivation,
addresses, transactions and chain queries. Rust for Bitcoin Cohort — Capstone Project 6.

---

## 0. Capstone criteria (source of truth)

### Purpose (MVP)
A developer "Swiss army knife" for the terminal: quick access to everyday Bitcoin operations for
scripting, testing and learning. Many commands work fully offline (key generation, address
derivation, transaction decoding); others connect to a node (broadcasting, fetching blocks, fee
estimation). Readable output by default, JSON on request.

### MVP features
- [ ] `key generate` — generate a private/public key pair
- [ ] `mnemonic new` — generate a BIP39 mnemonic
- [ ] `derive` — derive child keys/addresses from an xpub or xprv and a path
- [ ] `address from-pubkey` — create addresses of different types from a public key
- [ ] `address validate` — check an address and report its type and network
- [ ] `tx decode` — decode raw transaction hex
- [ ] `tx create`, `tx sign`, `tx broadcast` — build, sign and send a simple transaction
- [ ] `block info` — fetch block details by height or hash
- [ ] `fee estimate` — get a fee rate estimate from a node
- [ ] `--network` flag (regtest, signet, testnet4)
- [ ] `--json` flag for machine-readable output
- [ ] TUI mode (ratatui) — bonus, but **we are doing it**

### Required crates
`bitcoin`, `bip39`, `bitcoincore-rpc` (**we use its maintained successor `corepc-client`**, see Risks), `clap` (derive API), `ratatui`.

### Deliverables
- [ ] GitHub repo
- [ ] README
- [ ] Architecture diagram
- [ ] Demo video
- [ ] Live presentation

---

## 1. Housekeeping (before any code)

- [ ] **Separate repo.** This folder currently lives inside the course repo
  (`rust-for-bitcoin-2.0`, branch `mesoma/rfb-assignment-01`). Move it out, or `git init` it
  separately and ignore it in the parent — otherwise capstone commits leak into the assignment branch.
- [ ] **Rename the package** `BTC-Cli` → `btc-cli` (snake/kebab case; avoids warnings). Binary name: `btc`.
- Local environment: Rust 1.91, Bitcoin Core v30 (brew) — regtest available locally.

---

## 2. Core design principle

> **A command never prints. It returns a data struct. A separate layer decides how to show it.**

```
                ┌──────────── Front ends ────────────┐
   argv ──►  clap CLI (cli/)          ratatui TUI (tui/)
                └──────────┬───────────────┬─────────┘
                           ▼               ▼
                 ┌──────────────────────────────────┐
                 │   services / use cases (app/)    │  ← one function per feature
                 └──────┬─────────────────┬─────────┘
                        ▼                 ▼
          ┌────────────────────┐  ┌───────────────────────────┐
          │ core (offline,     │  │ node: trait NodeBackend   │
          │ pure, no I/O)      │  │  └─ CoreRpcBackend (RPC)  │──► bitcoind
          │ keys, bip39, bip32,│  │  └─ MockBackend (tests)   │
          │ address, tx, psbt  │  └───────────────────────────┘
          └────────────────────┘
                        │ returns Serialize structs
                        ▼
          output/: Human (tables, colours) | Json | TUI widgets
```

Why:
- **`--json` is nearly free.** Every result derives `Serialize`; JSON output is one `serde_json::to_string_pretty`.
- **The TUI reuses everything.** Same services as the CLI; only rendering differs. No Bitcoin logic is written twice.
- **Offline and online code stay separate.** `core` has no network code — easy to test, runs without a node.
- **The node is behind a trait.** RPC crates can be swapped; tests use a mock with no bitcoind.

**Dependency rule:** `cli/tui → app → core/node`, never backwards.

---

## 3. Project structure (Cargo workspace — ✅ decided)

```
btc-cli/
├── Cargo.toml                 # [workspace]
├── crates/
│   ├── btc-core/              # lib: pure Bitcoin logic, thiserror
│   │   └── src/{lib, network, keys, mnemonic, derive, address, tx/{decode,build,sign}, script, error}.rs
│   ├── btc-node/              # lib: NodeBackend trait, RPC impl, config/auth
│   │   └── src/{lib, backend, rpc, config, error}.rs
│   └── btc/                   # bin: CLI + TUI, anyhow
│       └── src/
│           ├── main.rs        # parse → dispatch → render → exit code
│           ├── cli/           # clap structs only (args.rs, commands/*.rs)
│           ├── app/           # services: glue core + node, return result structs
│           ├── output/        # Render trait, human.rs, json.rs
│           └── tui/           # app.rs (state), event.rs, ui/*.rs (screens)
├── tests/                     # CLI integration tests (assert_cmd), regtest tests
├── scripts/regtest-demo.sh    # reproducible demo for the video
└── docs/
    ├── ProjectRequirements.md # this file
    └── architecture.md
```

A workspace enforces the layer boundaries (`core` cannot accidentally import `clap`) and maps
directly onto the architecture diagram. Fallback: a single crate with the same folders as modules,
split later — keep the same dependency direction either way.

---

## 4. Crates

| Purpose | Crate | Notes |
|---|---|---|
| Bitcoin types | `bitcoin` 0.32 (feature `rand-std`) | keys, BIP32, addresses, tx, PSBT, script |
| Mnemonics | `bip39` 2.x (feature `rand`) | |
| CLI | `clap` 4 (`derive`, `env`) + `clap_complete` | completions are a cheap extra |
| RPC | `corepc-client` | ✅ decided — maintained successor to `bitcoincore-rpc`; use the version module matching Core v30 (or nearest) |
| TUI | `ratatui` + `crossterm` | |
| Errors | `thiserror` (libs), `anyhow` (bin) | |
| Config | `serde`, `serde_json`, `toml`, `dotenvy`, `directories` | |
| Output | `comfy-table`, `owo-colors` | |
| Security | `zeroize` | wipe seeds/keys on drop |
| Testing | `assert_cmd`, `predicates`, `insta`, `corepc-node` | `corepc-node` spawns a throwaway bitcoind |

---

## 5. Command tree (MVP + extras)

```
btc [--network regtest|signet|testnet4] [--json] [--rpc-url ..] [--rpc-cookie ..|--rpc-user/--rpc-pass] <cmd>

key generate [--type ecdsa|schnorr]        # WIF, pubkey hex, x-only
mnemonic new [--words 12|24] [--passphrase]
mnemonic to-xprv <words> [--passphrase]    # extra: bridges mnemonic → derive
derive <xprv|xpub> <path> [--count N] [--addr-type ..]
address from-pubkey <hex> [--type p2pkh|p2sh-p2wpkh|p2wpkh|p2tr|all]
address validate <addr>
tx decode <hex|-> [--prevouts-from-node]   # "-" = read from stdin (pipeable)
tx create --input txid:vout[:amount] --to addr:sats [--change addr] --fee-rate N  → PSBT
tx sign <psbt> --key <wif|xprv> | --key-file <path>                              → finalized hex
tx broadcast <hex>
block info <height|hash>
fee estimate [--target 1..1008] [--mode economical|conservative]
node status                                # extra: chain, tip, sync %, peers
config                                     # extra: show resolved network + RPC settings (never the password)
tui
completions <shell>                        # extra
```

### Global behaviour (decide once, apply everywhere)
- **Exit codes:** `0` ok · `1` user/input error · `2` node/network error.
- **With `--json`, errors are JSON too:** `{"error": {"kind": "...", "message": "..."}}` on stdout.
- **Mainnet is refused** for `tx sign` / `tx broadcast` (or only behind an explicit flag).
- **Config precedence:** flags > env vars / `.env` > `~/.config/btc/config.toml` > per-network defaults.
  - Env vars (done in Phase 1): `BTC_NETWORK`, `BTC_RPC_URL`, `BTC_RPC_COOKIE`, `BTC_RPC_USER`, `BTC_RPC_PASSWORD`.
  - RPC auth combinations are validated after parsing (not with clap `requires`/`conflicts_with`,
    which ignore env-var values) and fail with error kind `invalid_config`.
  - Default RPC ports: regtest `18443`, signet `38332`, testnet4 `48332`.
  - Cookie auth by default; user/pass optional.

---

## 6. The tx create → sign → broadcast pipeline

The hardest MVP feature. The CLI is **stateless** (it is not a wallet), so **PSBT** is the
interchange format between steps:

```
tx create  ──PSBT(base64)──►  tx sign  ──raw hex──►  tx broadcast ──► txid
  inputs + outputs            add signature,          sendrawtransaction
  + witness_utxo              finalize, extract
```

- `create` needs each input's amount + scriptPubKey (required for SegWit signing and fee math):
  from `--input txid:vout:amount` or fetched from the node (`gettxout` / `getrawtransaction`).
- Fee = fee_rate × estimated vsize. MVP: **P2WPKH inputs only**. Stretch: P2TR key-path spends.
- `sign` uses rust-bitcoin's `Psbt::sign` with a key map, then finalizes and extracts.
- Sanity checks: reject dust outputs, absurd fees, and inputs < outputs + fee.

---

## 7. TUI design (ratatui)

- **Architecture:** `App { screen, per-screen state, input mode, status bar }` + `Action` enum +
  `update(action)` / `draw(frame)` loop (Elm-style) — keeps it testable.
- **Screens (tabs):**
  - Keys & Mnemonic
  - Derive (live-updating address list as the path changes)
  - Address tools
  - Tx decoder (paste hex → browse inputs/outputs as a tree)
  - Block explorer (tip + last N blocks, auto-refresh)
  - Fees dashboard
  - Node status
- **Never block the UI on the node.** RPC clients are blocking → run calls on a worker thread,
  send results back over `std::sync::mpsc`. No Tokio needed.
- **Offline mode:** no node → online tabs show "node unreachable"; offline tools keep working.
- Every screen calls the same `app/` services as the CLI.

---

## 8. Phases (step by step)

Each phase ends with something working, demoable and committed.

### Phase 0 — Setup ✅
- [x] Workspace layout, dependencies (bitcoin 0.32 shared with corepc-client; `client_sync::v30` available)
- [x] fmt, clippy (`-D warnings`) and tests pass locally
- Repo, CI and README moved to Phase 9
- **Learn:** Cargo workspaces
- **Done when:** CI green on an empty skeleton

### Phase 1 — Skeleton ✅
- [x] Full clap command tree + global flags (`--network`/`-n`, `--json`, RPC flags, all with `BTC_*` env vars)
- [x] `Network` enum (`btc-core`, mainnet unrepresentable) + clap-side `NetworkArg`
- [x] `Render` trait (human + JSON) and `emit` / `emit_error`
- [x] Error → exit code mapping (`AppError`: kind + exit code; clap usage errors remapped from 2 to 1)
- [x] Every command stubbed with "not implemented"
- [x] Real commands: `config` (resolved network + RPC settings) and `completions`
- [x] Tests: 3 core, 2 node, 9 CLI integration (assert_cmd); clippy clean
- **Learn:** clap derive, trait objects vs generics
- **Done when:** `btc --help` shows the full tree; the `--json` path works

### Phase 2 — Keys & mnemonics ✅
- [x] `key generate` (`--type ecdsa|schnorr`): WIF, private hex, compressed or x-only public key + parity
- [x] `mnemonic new` (`--words 12..24`, `--passphrase`): numbered words, entropy/checksum sizes, root tprv/tpub/fingerprint
- [x] `mnemonic to-xprv` (words or `-` for stdin, `--passphrase`): root tprv/tpub/fingerprint
- [x] Secret handling (Risk 4): `Zeroizing` on every secret string and the seed, `-` reads from stdin
  (only one argument per command), stderr warning in human mode (`Render::contains_secrets`)
- [x] Tests: official BIP39 vectors (entropy → words → root key), key = 1 → generator point G,
  testnet WIF, bad checksum rejected, uppercase accepted; 8 new CLI tests
- **Learn:** secp256k1, WIF, compressed vs x-only keys; BIP39 entropy → checksum → words → PBKDF2 seed
- **Note:** `bitcoin`'s `SecretKey`/`Xpriv` are `Copy` and can't be wiped reliably; we zeroize every
  copy we own (strings, seed bytes)

### Phase 3 — Derivation ✅
- [x] `derive <tprv|tpub|-> <path>`: accepts `m/84'/…`, `m/84h/…` or relative `0/0`
- [x] `--count N` derives consecutive children (last index +1, +2, …); table output for N > 1
- [x] `--addr-type`, or inferred from the path purpose (44'→p2pkh, 49'→p2sh-p2wpkh, 84'→p2wpkh,
  86'→p2tr), else p2wpkh; the output says where the type came from
- [x] xpub-only restrictions: hardened steps from a tpub fail with an explanation; mainnet xprv/xpub rejected
- [x] Warning when a BIP44-style path uses coin type ≠ 1' on a test network
- [x] `btc-core/src/address.rs` (address building from a public key), ready for Phase 4
- [x] Tests: BIP32 test vector 1 (all levels), BIP44/49/84/86 first-address vectors, watch-only
  (account tpub + `0/7` = full path from root), count, warnings; 5 new CLI tests
- **Learn:** BIP32 (chain code; hardened needs the private key), BIP44/49/84/86 purposes

### Phase 4 — Addresses ✅
- [x] `address from-pubkey <hex> [--type …|all]`: accepts compressed (33 B), uncompressed (65 B,
  P2PKH only) and x-only (32 B, P2TR only) keys; `all` lists what the key can't make and why
- [x] `address validate <addr>`: type, encoding (base58check / bech32 / bech32m), witness version,
  payload, scriptPubKey hex + asm, every network it's valid on, explanatory notes
- [x] Wrong-network addresses fail with error kind `wrong_network` (exit 1); malformed ones with
  `invalid_input` and the real cause (full error chain; BIP350 wrong-checksum-variant hint)
- [x] Tests: BIP173/BIP350 valid + invalid vectors (verified independently in Python), generator
  point G addresses, uppercase bech32, shared base58 test prefixes; 5 new CLI tests incl. a
  from-pubkey → validate round trip on all three networks
- **Learn:** P2PKH / P2SH / P2WPKH / P2TR, bech32 vs bech32m, taproot tweak

### Phase 5 — Tx decode ✅
- [x] `tx decode <hex|->`: txid / wtxid, version, segwit, RBF (BIP125), coinbase, locktime (+ whether
  it's enforced), size / base size / weight / vsize
- [x] Inputs: outpoint, spend type (p2pkh, p2sh-p2wpkh/-p2wsh, p2wpkh, p2wsh, p2tr key/script path —
  inferred from scriptSig/witness, or certain when prevouts are known), scriptSig hex+asm, witness,
  sequence meaning (RBF, BIP68 relative locktime), BIP34 coinbase height
- [x] Outputs: value, script type (incl. p2pk, multisig, p2a, op_return, future witness versions),
  address for the selected network, scriptPubKey hex+asm, OP_RETURN data as text, witness commitment
- [x] Fee + sat/vB computed when prevouts are supplied (core API ready; `--prevouts-from-node`
  returns `not_implemented` until Phase 6)
- [x] Friendly errors: txid or PSBT pasted instead of hex, trailing bytes; line breaks ignored
- [x] `btc-core/src/script.rs` (script classification), reusable for Phase 7
- [x] Tests: real regtest txs built by Bitcoin Core v30 (`crates/btc-core/testdata/regtest_txs.json`,
  regenerate with `scripts/make_tx_fixtures.py`); every field compared with Core's decode,
  fee equals Core's; 4 new CLI tests
- **Learn:** tx serialization, SegWit marker/flag, weight units

### Phase 6 — Node layer
- [ ] Config + auth (cookie, user/pass, `.env`, config file)
- [ ] `NodeBackend` trait + RPC implementation + mock
- [ ] `node status`, `block info`, `fee estimate`
- [ ] Fee fallback (Risk 3): surface node errors, `--fallback-rate`, label fallback in output
- **Learn:** JSON-RPC, cookie auth, `estimatesmartfee`
- **Done when:** works against local regtest; clean error when the node is down

### Phase 7 — Tx pipeline
- [ ] `tx create` → `tx sign` → `tx broadcast`
- [ ] `tx sign` reads keys via stdin / `--key-file` (Risk 4)
- [ ] `scripts/regtest-demo.sh` (create node wallet, mine 101, fund our address, spend it, mine 1)
- **Learn:** UTXOs, PSBT roles, sighash, fee math
- **Done when:** full round trip on regtest, confirmed after mining a block

### Phase 8 — TUI
- [ ] Tabs, worker thread, all screens listed in §7
- **Learn:** event loops, immediate-mode rendering
- **Done when:** every MVP feature is reachable from the TUI

### Phase 9 — Polish & deliverables
- [ ] Coloured tables (shell completions already done in Phase 1)
- [ ] Standalone repo (moved from Phase 0)
- [ ] CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (moved from Phase 0)
- [ ] Architecture diagram (`docs/architecture.md`)
- [ ] README with usage examples
- [ ] Demo video
- [ ] Presentation slides
- **Done when:** the demo script runs from a clean start

**Priority:** Phases 0–7 are the MVP and must ship. The TUI is the differentiator — start it as
soon as Phase 6 lands, not in the final week.

---

## 9. Testing strategy

- **`core` unit tests** — official BIP32, BIP39, BIP84, BIP173, BIP350 vectors (proves correctness, not just "it runs").
- **`app` tests** — against `MockBackend`: fast, no bitcoind, run in CI.
- **CLI tests** — `assert_cmd` + `insta` snapshots of human and JSON output.
- **Regtest integration tests** — `corepc-node` spawns a throwaway bitcoind. Gate behind
  `#[ignore]` or a feature so CI stays fast; locally set `BITCOIND_EXE` to the brew binary.

---

## 10. Risks to check early

1. ✅ **RPC crate vs Core v30 — resolved: `corepc-client`.** `bitcoincore-rpc` is in maintenance
   mode and some response structs no longer match newer Core versions (e.g.
   `getblockchaininfo.warnings` changed in v28). `corepc-client` ships version-specific clients and
   types (`client_sync::vNN`); pick the module matching Core v30 (or the nearest available) and
   confirm on day one of Phase 6. The `NodeBackend` trait still isolates this choice.
2. ✅ **testnet4 in `bitcoin` 0.32 — resolved.** `bitcoin` 0.32.102 has `Network::Testnet4`;
   `btc_core::Network` maps to it directly.
3. ✅ **Fee estimation on regtest — adopted as a requirement.** `estimatesmartfee` usually returns
   "insufficient data" on regtest. `fee estimate` must: report the node's `errors` clearly (not
   crash), offer a fallback (`--fallback-rate <sat/vB>`, default 1 sat/vB on regtest, labelled as a
   fallback in both human and JSON output), and the demo script generates transactions + blocks
   first so real estimates appear. Tracked in Phase 6 and Phase 7.
4. ✅ **Secret handling — adopted as a requirement.**
   - Wrap seeds, mnemonics and private keys in `Zeroizing<…>` (or types implementing `ZeroizeOnDrop`).
   - Never log or `Debug`-print secrets (custom `Debug` impls that redact).
   - Print a warning (stderr, human mode) whenever a private key / mnemonic is shown.
   - Every command taking a secret accepts it via stdin (`-`) or `--key-file <path>` instead of
     argv, since argv lands in shell history and `ps` output.
   - Tracked in Phase 2 (keys/mnemonics), Phase 3 (xprv) and Phase 7 (`tx sign`).

---

## 11. Open decisions

| Decision | Recommendation | Status |
|---|---|---|
| Workspace vs single crate | Workspace (`btc-core`, `btc-node`, `btc`) | ✅ decided |
| `corepc-client` vs `bitcoincore-rpc` | `corepc-client` | ✅ decided |
| Mainnet signing/broadcast | Blocked (or explicit flag only) | ☐ |
| Team split (if multiple people) | By layer: `core` / `node` + tx pipeline / TUI + output | ☐ |
