# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is **Pachi Chain** — a custom EVM-compatible L1 blockchain built on reth, targeting blockchain-based gambling infrastructure. The base is a fork of reth (high-performance Ethereum execution client in Rust), extended with three native features:

1. **Native Price Oracle** — Precompile-based price feeds from 5 CEX WebSockets (0x0802)
2. **VRF (Verifiable Random Function)** — Cryptographically provable randomness for games (0x0101, 0x0102)
3. **Session Key + Gas Sponsor** — Web2-grade UX with delegated signing and gas sponsorship (0x0800, 0x0801)

All Pachi-specific code lives in `crates/pachi/`. Core reth code is NOT modified.

### Documentation Map

| Document | Purpose |
|----------|---------|
| `docs/pachi/architecture.md` | Crate structure, dependency graph, integration points |
| `docs/pachi/specs/oracle.md` | Oracle system full spec |
| `docs/pachi/specs/vrf.md` | VRF system full spec |
| `docs/pachi/specs/session-key.md` | Session Key system full spec |
| `docs/pachi/specs/gas-sponsor.md` | Gas Sponsor system full spec |
| `docs/pachi/specs/tx-types.md` | Transaction types reference (0x04, 0x05, 0x06, 0x50) |
| `docs/pachi/reference/precompiles.md` | Precompile addresses, interfaces, gas costs |
| `docs/pachi/reference/storage-layout.md` | All on-chain storage key/value layouts |
| `docs/pachi/implementation/plan.md` | Master implementation plan with task checklists |

**Always read the relevant spec before implementing a feature.** Each spec is self-contained.

## Essential Commands

```bash
# Format (always nightly)
cargo +nightly fmt --all

# Lint (nightly clippy, all features, deny warnings)
cargo +nightly clippy --workspace --lib --examples --tests --benches --all-features -- -D warnings

# Run all tests (use nextest)
cargo nextest run --workspace

# Run a single crate's tests
cargo nextest run -p pachi-primitives

# Run a single test by name
cargo nextest run -p pachi-primitives my_test_name

# Run tests with cargo test (alternative, also runs doc tests)
cargo test -p pachi-primitives my_test_name

# Check the whole workspace compiles
cargo check --workspace --all-features

# Check only Pachi crates compile
cargo check -p pachi-primitives -p pachi-vrf-core  # add more as they're created

# Build debug binary
cargo build --bin reth

# Build release binary
cargo build --bin reth --release

# Full pre-PR check (lint + docs + tests)
make pr

# TOML formatting (required when Cargo.toml files change)
make lint-toml    # uses dprint

# Dependency lint (required when dependencies change)
zepter            # assume installed
```

## Workspace Configuration

- **Rust edition**: 2024
- **MSRV**: 1.93
- **Formatter**: nightly rustfmt
- **Test runner**: cargo-nextest preferred over cargo test

## Architecture (Big Picture)

### reth Core Data Flow

The base reth codebase follows a **staged sync** architecture with modular, trait-heavy design:

1. **Networking** (`crates/net/`): P2P discovery, peer management, block/tx propagation
2. **Pipeline / Stages** (`crates/stages/`): Staged sync for initial sync
3. **Engine** (`crates/engine/`): Consensus Engine handling Engine API calls
4. **EVM / Execution** (`crates/evm/`, `crates/revm/`): Transaction execution via revm
5. **Storage** (`crates/storage/`): MDBX + static files (NippyJar)
6. **Trie** (`crates/trie/`): Merkle Patricia Trie for state root
7. **RPC** (`crates/rpc/`): JSON-RPC server (jsonrpsee-based)
8. **Payload Builder** (`crates/payload/`): Block building
9. **Node Builder** (`crates/node/builder/`): Component assembly and startup

Ethereum-specific implementations live in `crates/ethereum/`. Pachi extends this pattern with `crates/pachi/`.

### Pachi Crate Structure

```
crates/pachi/
+-- primitives/          Shared types: Limit, Constraint, AssetId, Confidence, etc.
+-- hardforks/           Pachi hardfork definitions (Phase 1, Phase 2)
+-- vrf/core/            Pure ECVRF cryptography (secp256k1, k256 crate)
+-- oracle/precompile/   PriceOracle precompile (0x0802)
+-- oracle/engine/       WebSocket price collection + Aggregator
+-- vrf/precompile/      VRF precompiles (0x0101, 0x0102) + Dealer state
+-- session/precompile/  SessionRegistry precompile (0x0800)
+-- sponsor/precompile/  SponsorHub precompile (0x0801)
+-- tx/                  Custom tx types (0x04, 0x05, 0x06, 0x50)
+-- evm/                 PachiEvmConfig: precompile registration + handlers
+-- payload/             PachiPayloadBuilder: system tx injection
+-- consensus/           PachiConsensus: custom block validation
+-- pool/                Mempool extensions for custom tx types
+-- rpc/                 JSON-RPC extensions (session_*, sponsor_*, oracle_*)
+-- node/                PachiNode: component assembly
+-- genesis/             Genesis config extensions
```

### Implementation Layer Dependencies

```
Layer 0: primitives, hardforks, vrf/core  (no deps)
Layer 1: oracle/precompile, vrf/precompile, session/precompile, sponsor/precompile  (deps: L0)
Layer 2: tx  (deps: L0)
Layer 3: evm  (deps: L1 + L2)
Layer 4: oracle/engine, payload, consensus, pool  (deps: L3)
Layer 5: node, rpc, genesis  (deps: all)
```

**Implement bottom-up. Complete each layer before starting the next.**

### Key reth Traits to Implement

| reth Trait | Pachi Type | Crate |
|-----------|-----------|-------|
| `ConfigureEvm` | `PachiEvmConfig` | `pachi-evm` |
| `PayloadBuilder` | `PachiPayloadBuilder` | `pachi-payload` |
| `FullConsensus` + `Consensus` + `HeaderValidator` | `PachiConsensus` | `pachi-consensus` |
| `NodeTypes` | `PachiNode` | `pachi-node` |

### Precompile Address Map

| Address | Name | Access |
|---------|------|--------|
| `0x0101` | VRF_COMPUTE | System-only |
| `0x0102` | VRF_VERIFY | Public |
| `0x0800` | SessionRegistry | Public |
| `0x0801` | SponsorHub | Public (governance: owner-only) |
| `0x0802` | PriceOracle | Public reads, system writes |

### Transaction Type Map

| Type | Name | Signer | Gas Payer | msg.sender |
|------|------|--------|-----------|------------|
| `0x04` | SessionTx | Session key | Authorizer | Authorizer |
| `0x05` | SponsoredTx | Sender EOA | Sponsor | Sender |
| `0x06` | SessionSponsoredTx | Session key | Sponsor | Authorizer |
| `0x50` | PachiSystemTx | Unsigned | Free | System addr |

## Development Workflow

### MANDATORY: Implementation Protocol

**This protocol is NON-NEGOTIABLE. Every layer/crate implementation MUST follow it. No exceptions.**

#### Before coding

1. **Read MEMORY.md** and all referenced memory files. Apply every rule.
2. **Read EVERY relevant spec doc** (`docs/pachi/specs/`) line-by-line.
3. **Count and record** the exact number of functions, events, gas costs, and validation rules in the spec.
4. Write down: "Spec requires X functions, Y events, Z validation rules."
5. Read existing Layer code that this layer depends on — understand the ACTUAL API.

#### During coding

6. Implement ALL items from the spec. Never defer any without explicit user approval.
7. For each function: verify gas cost, ABI encoding, error handling match the spec.
8. Never use "simplified", "deferred", or "placeholder" implementations. If the spec says to do it, do it fully.
9. If something is genuinely impossible at this layer, explain the EXACT technical reason — not "it's complex."

#### Before marking done (3-stage delivery)

10. **Stage 1 — Implement**: All code and tests.
11. **Stage 2 — Self-audit**: Cross-reference spec docs against code. Count: implemented vs spec. Must match.
12. **Stage 3 — Report**: Present a spec cross-reference table to the user. Never say "done" without this table.

**Checklist format for the report:**

```
| Spec Item | Implemented | Status | If ⚠️, exact technical reason |
```

Only mark `plan.md` checkboxes `[x]` for items that are FULLY implemented. Items deferred to a later layer stay `[ ]` with a note explaining why.

#### What counts as "done"

- ✅ "Compiles + tests pass + spec table matches 100%" = done
- ❌ "Compiles + basic tests pass" = NOT done
- ❌ "Deferred to Layer N+1 because it's complex" = NOT acceptable
- ✅ "Cannot implement at this layer because `default_ethereum_payload()` owns the BlockBuilder and doesn't expose hook points" = acceptable (with the explanation)

### Working Incrementally Across Sessions

Work is tracked in `docs/pachi/implementation/plan.md`. Each session:

1. **Read plan.md** to find the next unchecked task.
2. **Read the relevant spec** under `docs/pachi/specs/`.
3. **Read MEMORY.md** and apply all stored rules.
4. **Implement** the task following the Implementation Protocol above.
5. **Self-review**: Run `cargo +nightly fmt --all && cargo +nightly clippy -p <crate> -- -D warnings`.
6. **Test**: Run `cargo nextest run -p <crate>` and ensure all tests pass.
7. **Self-audit**: Cross-reference spec vs code, produce spec table.
8. **Mark done**: Update the checkbox in `plan.md` to `[x]` ONLY for fully implemented items.

### Creating a New Pachi Crate

When creating a new crate under `crates/pachi/`:

1. Create the directory structure with `src/lib.rs`.
2. Create `Cargo.toml` following existing reth crate patterns (check `crates/ethereum/` for examples).
3. Register the crate in the workspace root `Cargo.toml` under `[workspace.members]`.
4. Add appropriate dependencies from the workspace.
5. Run `cargo check -p <new-crate>` to verify it compiles.

### Self-Verification Checklist

Before considering any task complete, run ALL of these:

```bash
# 1. Format
cargo +nightly fmt --all

# 2. Clippy on the crate
cargo +nightly clippy -p <crate-name> --all-features -- -D warnings

# 3. Tests pass
cargo nextest run -p <crate-name>

# 4. Workspace still compiles
cargo check --workspace --all-features
```

If any step fails, fix it before moving on.

## Code Conventions

### Rust Style

- **Type ordering in files**: Primary type (matching filename) goes first, then public auxiliary types, then public traits, then private types/helpers.
- **File operations**: Use `reth_fs_util` instead of `std::fs` for better error context.
- **Async**: Don't block async tasks — use `spawn_blocking` for CPU-intensive or blocking I/O.
- **Parallelism**: Use rayon for CPU-bound parallel work, tokio for I/O.
- **Logging**: Use `tracing` with targets: `tracing::debug!(target: "pachi::oracle", ?value, "description");`
- **Pachi logging targets**: Use `pachi::oracle`, `pachi::vrf`, `pachi::session`, `pachi::sponsor`, `pachi::evm`, `pachi::payload`, `pachi::consensus`.

### Comments

Only comment non-obvious behavior, WHY decisions, constraints, safety requirements. Don't describe what changed or restate code in English.

### Language

All code, comments, documentation, and commit messages must be in **English**.

### Commit Messages

Follow conventional format: `feat(pachi):`, `fix(pachi):`, `chore(pachi):`, `test(pachi):`

Include the sub-feature when relevant: `feat(pachi/oracle):`, `feat(pachi/vrf):`, `feat(pachi/session):`, `feat(pachi/sponsor):`

## CI / Pre-PR Checklist

1. `cargo +nightly fmt --all` — formatting
2. `cargo +nightly clippy --workspace --lib --examples --tests --benches --all-features -- -D warnings` — no warnings
3. Tests pass for affected crates
4. `cargo docs --document-private-items` — docs build
5. If CLI changed: `make update-book-cli`
6. If Cargo.toml changed: `zepter` and `make lint-toml`

## Important Restrictions

- **Never modify** files in `crates/storage/libmdbx-rs/mdbx-sys/libmdbx/` — vendored third-party code
- **Never hand-edit** CLI docs under `docs/vocs/docs/pages/cli/` — auto-generated
- **Never modify** core reth crates — all Pachi code goes in `crates/pachi/`
- Keep PRs small and focused (1 logical change per PR)
