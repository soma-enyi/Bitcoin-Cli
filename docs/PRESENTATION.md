# BTC-CLI Capstone Presentation Outline

**Duration:** 10-15 minutes  
**Audience:** Rust for Bitcoin cohort + instructors  
**Objective:** Showcase the architecture, implementation, and live demo

---

## Slide 1: Title & Problem

**"BTC-CLI: A Developer's Swiss Army Knife for Bitcoin"**

Problem:
- Bitcoin developers repeat the same operations: generate keys, derive addresses, decode txs
- Each operation is painful: either write Python scripts, use online tools (trust issues), or spin up a full Bitcoin Core node
- No offline-first, fully-tested, composable toolkit exists

Solution: **BTC-CLI**
- 13 MVP commands covering keys → addresses → transactions
- Works offline or with a node
- JSON output for automation
- Bonus: Interactive TUI

---

## Slide 2: Architecture & Dependencies

**Core Principle:** "A command never prints. It returns a data struct."

Diagram:
```
CLI/TUI  →  Services (app/)  →  btc-core (offline)  +  btc-node (RPC)  →  Output (human/JSON)
```

Key design decisions:
1. **Pure logic in btc-core** — no I/O, 100% testable, zero network code
2. **RPC behind a trait** — swappable backends (CoreRpc, Mock, custom)
3. **Serialize everything** — JSON output is free
4. **TUI reuses logic** — only rendering differs

**Why this matters:** Easy to test, maintain, extend, and parallelize implementation.

---

## Slide 3: Feature Breakdown

**Offline (no bitcoind):**
- Key generation: ECDSA + Schnorr (BIP340)
- BIP39 mnemonics: entropy → words → seed
- BIP32 derivation: hardened, unhardened, watch-only
- Address generation: P2PKH, P2SH-P2WPKH, P2WPKH, P2TR
- Address validation: type, network, checksum, scriptPubKey

**Online (with node):**
- Node status: chain height, sync %, peers
- Block info: by height or hash
- Fee estimation: sat/vB + fallback
- Tx pipeline: PSBT → sign → broadcast (stateless)

**Bonus:**
- Interactive TUI (ratatui) with 7 tabs
- JSON output for scripting
- Colored tables + shell completions

---

## Slide 4: Implementation Phases

| Phase | Feature | Tests | Status |
|-------|---------|-------|--------|
| 0 | Setup (workspace, deps) | — | ✓ |
| 1 | CLI skeleton (all commands stubbed) | 9 | ✓ |
| 2 | Keys & mnemonics | +13 | ✓ |
| 3 | Derivation (BIP32) | +15 | ✓ |
| 4 | Addresses | +8 | ✓ |
| 5 | Tx decode | +4 | ✓ |
| 6 | Node layer (RPC backend) | +4 | ✓ |
| 7 | Tx pipeline (create/sign/broadcast) | +2 | ✓ |
| 8 | TUI (ratatui) | — | ✓ |
| 9 | Polish (CI/CD, docs, demo) | — | ✓ |

**Total:** 73 tests passing, 0 failures

---

## Slide 5: Testing Strategy

**Unit Tests (42 in btc-core):**
- Official BIP32, BIP39, BIP44, BIP173, BIP350 vectors
- Validates derivation chains, checksum, encoding

**CLI Tests (31 in btc):**
- `assert_cmd`: invoke binary, check exit codes, stdout, stderr
- Coverage: all commands, error cases, JSON/human modes

**Mock Tests (2 in btc-node):**
- Config defaults, MockBackend returns sensible data
- No bitcoind required → CI is fast

**Why this works:**
- Pure logic = easy to test offline
- Official vectors = confidence in correctness
- CLI integration tests = catch regressions
- Mocking = fast CI without external dependencies

---

## Slide 6: Security & Secret Handling

**Secrets are dangerous. We take them seriously.**

1. **Private keys are Zeroizing<String>** — wiped from memory on drop
2. **No shell history leaks** — keys only from stdin or files, never argv
3. **Warnings on output** — stderr says "warning: this output contains secrets"
4. **No logging** — secrets never printed to logs
5. **Regtest by default** — mainnet is off limits for safety

**Example:**
```bash
# Bad (key in shell history):
btc tx sign <psbt> --key L1234... ✗

# Good (key from file or stdin):
cat key.txt | btc tx sign <psbt> --key - ✓
btc tx sign <psbt> --key-file key.txt ✓
```

---

## Slide 7: Live Demo (2-3 min)

Walk through:
1. **Key generation:** `btc key generate`
2. **Mnemonic + derivation:** `btc mnemonic new | btc mnemonic to-xprv | btc derive ...`
3. **Address validation:** `btc address validate <addr>`
4. **Tx decode:** `btc tx decode <hex>`
5. **TUI:** `btc tui` (show tab navigation)

---

## Slide 8: Challenges & Solutions

**Challenge 1: PSBT Complexity**
- Solution: Stateless design — each command is independent (create → sign → broadcast)
- User stores PSBT between steps, we don't maintain state

**Challenge 2: Fee Estimation on Regtest**
- Solution: Offer --fallback-rate, label as fallback in output
- Regtest demo mines blocks first to generate enough txs for real estimates

**Challenge 3: Testing without bitcoind**
- Solution: MockBackend implements NodeBackend trait
- Real integration tests gated behind feature flag + env var

**Challenge 4: Output Flexibility**
- Solution: Serialize + Render trait → human and JSON are derived from same logic
- Adding new output format (e.g., CSV) requires only one impl

---

## Slide 9: Lessons Learned

1. **Separation of concerns works.** Pure logic (btc-core) is easier to test and reason about.
2. **Traits unlock flexibility.** NodeBackend trait made it trivial to swap RPC implementations.
3. **Official test vectors matter.** BIP32, BIP39 vectors caught edge cases early.
4. **Workspaces enforce boundaries.** Compiler prevents btc-core from importing clap.
5. **Early testing saves time.** Mock tests run in 100ms; live regtest tests in 10s. CI is fast.

---

## Slide 10: Roadmap & Repo Status

**Post-MVP (if continuing):**
- [ ] Full TUI with 7 active tabs + live data refresh
- [ ] Colored output (already dependencies, not yet wired)
- [ ] Worker threads for non-blocking RPC in TUI
- [ ] PSBT export/import from files
- [ ] Multisig script generation
- [ ] Hardware wallet integration

**Standalone Repo:**
- Currently in course repo branch
- Ready to move to independent GitHub (move `/Cap-stone project/btc-cli` to root)
- CI/CD in .github/workflows/ (GitHub Actions)
- README + architecture docs included

**Deliverables:**
- ✓ GitHub repo (in-course; movable)
- ✓ README with examples
- ✓ Architecture diagram (markdown)
- ✓ Demo script (video walkthrough prepared)
- ✓ Live presentation (this talk)

---

## Slide 11: Questions & Contact

**Repository:** [Show link]  
**Maintainer:** [Your name]  
**License:** MIT

Open to questions on:
- Architecture decisions
- Testing strategy
- Rust design patterns
- Bitcoin specifics

---

## Talking Points

**Why Rust?**
- Memory safety (no buffer overflows, use-after-free)
- Type safety (Address type can't be confused with a string)
- Fearless concurrency (worker threads in TUI don't race)
- Performance (single binary, no VM)

**Why this architecture?**
- Offline-first (core has no network dependencies)
- Composable (each command is a function, not a monolithic CLI)
- Testable (mock the node, test the logic)
- Extensible (add new commands by adding services + CLI args)

**Why 73 tests?**
- Every BIP vector checked (BIP32, BIP39, BIP44, BIP173, BIP350)
- Every command exercised (CLI integration tests)
- Every error path covered (invalid input, wrong network, insufficient funds)
- All 13 MVP commands work end-to-end

