# Pachi Chain Implementation Plan

## How to Use This Document

This is the master implementation plan. Each task has a checkbox. When a task is completed:
1. Mark it `[x]` in this file.
2. Ensure all tests pass (`cargo nextest run -p <crate>`).
3. Run `cargo +nightly clippy` and `cargo +nightly fmt --all` before considering it done.

Tasks within each layer are independent and can be worked on in parallel. Layers must be completed in order (Layer 0 before Layer 1, etc.).

---

## Phase 1: Testnet

### Layer 0: Foundation

No external dependencies. Pure types and algorithms.

#### L0-1: `pachi-primitives`

Crate: `crates/pachi/primitives/`

- [x] Create crate with Cargo.toml, register in workspace
- [x] Define `Limit` struct (limit_type, limit, period) with serialization
- [x] Define `LimitState` struct (used, last_window) with serialization
- [x] Define `Constraint` struct (index, condition, ref_value, limit)
- [x] Define `ConditionType` enum (Unconstrained, Equal, Greater, Less, GreaterEqual, LessEqual, NotEqual)
- [x] Define `CallPolicy` struct
- [x] Define `TransferPolicy` struct
- [x] Define `AssetId` enum (BTC=0x01 through DAI=0x06, with SUPPORTED_ASSETS constant)
- [x] Define `Confidence` enum (High=0, Medium=1, Degraded=2, Unavailable=3)
- [x] Define `AssetState` enum (Available { price, timestamp, confidence }, Unavailable)
- [x] Define `PriceSnapshot` and `SnapshotEntry` structs
- [x] Define `SystemAddress` constant (0x0...0)
- [x] Define Pachi-specific constants (STALENESS_THRESHOLD, MIN_ACTIVE_SOURCES, SPREAD_*_BPS, etc.)
- [x] Implement `LimitEngine`: check_and_update(limit, limit_state, amount, block_timestamp) -> Result
- [x] Implement `ConstraintEngine`: verify_constraint(constraint, calldata) -> Result
- [x] Unit tests for LimitEngine (all three modes, edge cases, window transitions)
- [x] Unit tests for ConstraintEngine (all 7 conditions, argument extraction from calldata)

**Acceptance criteria:** `cargo nextest run -p pachi-primitives` passes, all types serialize/deserialize correctly, Limit and Constraint engines are fully tested.

#### L0-2: `pachi-hardforks`

Crate: `crates/pachi/hardforks/`

- [x] Create crate with Cargo.toml
- [x] Define `PachiHardfork` enum (Phase1, Phase2)
- [x] Implement hardfork activation logic (by block number or timestamp)
- [x] Define genesis config extensions (session config params, sponsor config params, oracle params)
- [x] Unit tests

#### L0-3: `pachi-vrf-core`

Crate: `crates/pachi/vrf/core/`

- [x] Create crate with Cargo.toml
- [x] Implement ECVRF using secp256k1 (k256 crate)
  - [x] `vrf_compute(secret_key, seed) -> (random_value, proof)`
  - [x] `vrf_verify(public_key, seed, random_value, proof) -> bool`
  - [x] Key generation utility
- [x] Unit tests: compute/verify round-trip, determinism, invalid proof rejection
- [x] Benchmark: verify < 1ms target

**Acceptance criteria:** Pure crypto functions pass all tests, no I/O dependencies.

---

### Layer 1: Precompile State Machines

Depends on: Layer 0 (`pachi-primitives`).

These crates implement the state read/write logic for each precompile. They do NOT yet wire into the EVM; that happens in Layer 3.

#### L1-1: `pachi-oracle-precompile`

Crate: `crates/pachi/oracle/precompile/`

- [x] Create crate
- [x] Implement storage layout (2 slots per asset: price + packed timestamp/confidence)
- [x] Implement `get_price(state, asset_id)` -> (price, timestamp, confidence) with staleness check
- [x] Implement `get_price_batch(state, asset_ids)` -> Vec<(price, timestamp, confidence)>
- [x] Implement `is_supported(asset_id)` -> bool
- [x] Implement `apply_oracle_update(state, snapshot)` (write new prices, handle Unavailable)
- [x] Implement circuit breaker logic
- [x] Gas cost calculation functions
- [x] Unit tests for all read/write paths, staleness, circuit breaker, Unavailable handling

#### L1-2: `pachi-vrf-precompile`

Crate: `crates/pachi/vrf/precompile/`

- [x] Create crate
- [x] Implement VRF_COMPUTE precompile logic (system-only access check)
- [x] Implement VRF_VERIFY precompile logic (public)
- [x] Implement Dealer state machine:
  - [x] `request_vrf(state, msg_sender, seed, prepaid_gas)` -> key
  - [x] `fulfill(state, key, random_value, proof)` -> Result
  - [x] `get_result(state, key)` -> Option<bytes32>
  - [x] `is_fulfilled(state, key)` -> bool
- [x] Duplicate request rejection (same key)
- [x] Gas prepayment and refund logic
- [x] Unit tests for full lifecycle, duplicate rejection, access control

#### L1-3: `pachi-session-precompile`

Crate: `crates/pachi/session/precompile/`

- [x] Create crate
- [x] Implement SessionRegistry state operations:
  - [x] `create_session(state, authorizer, config)` -> session_hash
  - [x] `revoke_session(state, authorizer, session_hash)` -> Result
  - [x] `get_session(state, session_hash)` -> SessionRecord
  - [x] `get_active_sessions(state, authorizer)` -> Vec<session_hash>
  - [x] `is_valid(state, session_hash, block_timestamp)` -> bool
- [x] Slot management (find empty, replace revoked/expired)
- [x] Session nonce read/write
- [x] Full session validation flow (Steps 1-7 from spec)
- [x] Unit tests: create/revoke lifecycle, slot exhaustion/replacement, nonce management, policy validation, constraint checking

#### L1-4: `pachi-sponsor-precompile`

Crate: `crates/pachi/sponsor/precompile/`

- [x] Create crate
- [x] Implement SponsorHub state operations:
  - [x] `register_policy(state, sponsor, config)` -> Result
  - [x] `deactivate_policy(state, sponsor)` -> Result
  - [x] `deposit(state, sponsor, amount)` -> Result
  - [x] `withdraw(state, sponsor, amount)` -> Result
  - [x] `approve_mint(state, owner, sponsor)` -> Result
  - [x] `revoke_mint(state, owner, sponsor)` -> Result
  - [x] `transfer_ownership(state, current_owner, new_owner)` -> Result
- [x] View functions (get_balance, get_policy, get_sponsor_type, is_active, can_sponsor)
- [x] Dual-mode gas settlement:
  - [x] Deposit: balance lock, deduct actual, refund excess
  - [x] Mint: compute mint amount from actual gas, no balance ops
- [x] Pre-execution sponsor validation flow
- [x] Unit tests: registration, deposit/withdraw, mint/deposit mode transitions, governance, settlement for both modes

> **Note (L1-4):** Sponsor limit state keys use `keccak256("sponsor_limit", ...)` / `keccak256("sponsor_sender_limit", ...)` instead of the spec's shared `keccak256("limit_state", owner_hash, ...)` pattern. Session system follows the spec pattern. Functionally equivalent (no collision), but should be aligned to the shared pattern in Layer 3 (`pachi-evm`) if consistency is desired. See `crates/pachi/sponsor/precompile/src/storage_keys.rs`.

---

### Layer 2: Transaction Types

Depends on: Layer 0 (`pachi-primitives`).

#### L2-1: `pachi-tx`

Crate: `crates/pachi/tx/`

- [x] Create crate
- [x] Define `PachiTxType` enum (Session=0x04, Sponsored=0x05, SessionSponsored=0x06, System=0x50)
- [x] Implement `SessionTx` struct + RLP encode/decode
- [x] Implement `SponsoredTx` struct + RLP encode/decode
- [x] Implement `SessionSponsoredTx` struct + RLP encode/decode
- [x] Implement `PachiSystemTx` struct + RLP encode/decode (subtypes: OracleUpdate, VRFfulfill)
- [x] Implement EIP-2718 envelope extension (Decodable2718, Encodable)
- [x] Implement `alloy_consensus::Transaction` trait for each type
- [x] Implement `SignedTransaction` trait (signature recovery) for session/sponsored types
- [x] Implement signer recovery for SessionTx (recover session key address, not authorizer)
- [x] Unit tests: encode/decode round-trips, signature recovery, invalid envelope rejection
- [x] Fuzz tests for RLP decoding (malformed input)

---

### Layer 3: EVM Integration

Depends on: Layer 1 + Layer 2.

#### L3-1: `pachi-evm`

Crate: `crates/pachi/evm/`

- [ ] Create crate
- [ ] Create `PachiPrecompiles`: register all 5 precompiles into `PrecompilesMap`
  - [ ] 0x0101 VRF_COMPUTE (delegates to pachi-vrf-precompile)
  - [ ] 0x0102 VRF_VERIFY
  - [ ] 0x0800 SessionRegistry
  - [ ] 0x0801 SponsorHub
  - [ ] 0x0802 PriceOracle
- [ ] Create `PachiEvmFactory` implementing `EvmFactory` trait
- [ ] Create `PachiEvmConfig` implementing `ConfigureEvm` trait
- [ ] Implement pre-execution handler:
  - [ ] SessionTx: validate session, check policy, verify signature
  - [ ] SponsoredTx: validate sponsor, lock balance (Deposit) or check limits (Mint)
  - [ ] SessionSponsoredTx: session first, then sponsor
  - [ ] Set msg.sender appropriately per tx type
- [ ] Implement post-execution handler:
  - [ ] SessionTx: update nonce, update limits
  - [ ] SponsoredTx (Deposit): deduct actual gas, refund excess, update limits
  - [ ] SponsoredTx (Mint): mint native token to sequencer, update limits
  - [ ] SessionSponsoredTx: session limits + sponsor settlement
- [ ] Integration tests: execute each tx type through EVM, verify state changes

---

### Layer 4: Block Building & Validation

Depends on: Layer 3.

#### L4-1: `pachi-oracle-engine`

Crate: `crates/pachi/oracle/engine/`

- [ ] Create crate
- [ ] Implement WebSocket connection manager (5 sources)
  - [ ] Binance WS connector
  - [ ] Coinbase WS connector
  - [ ] OKX WS connector
  - [ ] Bybit WS connector
  - [ ] Kraken WS connector
- [ ] Implement reconnection logic with backoff
- [ ] Implement Aggregator (staleness filter -> outlier trim -> median -> confidence)
- [ ] Implement Local Price State (per-asset AssetState)
- [ ] Implement `generate_snapshot()` -> PriceSnapshot for block building
- [ ] Implement heartbeat (1s aggregation cycle)
- [ ] Unit tests: aggregator with various source scenarios
- [ ] Integration tests: mock WebSocket sources, verify snapshot generation

#### L4-2: `pachi-payload`

Crate: `crates/pachi/payload/`

- [ ] Create crate
- [ ] Implement `PachiPayloadBuilder` wrapping `EthereumPayloadBuilder`
- [ ] After user tx execution:
  - [ ] Scan receipts/logs for VRF requestVRF events
  - [ ] For each request: call VRF_COMPUTE, generate fulfill system tx
  - [ ] Verify fulfill tx is positioned after its request tx
- [ ] Generate OracleUpdate system tx from Oracle Engine snapshot
- [ ] Append system txs at block end (VRF fulfills first, then OracleUpdate)
- [ ] Handle edge cases: no VRF requests, all oracle sources down
- [ ] Integration tests: build blocks with VRF requests, verify system tx injection

#### L4-3: `pachi-consensus`

Crate: `crates/pachi/consensus/`

- [ ] Create crate
- [ ] Implement `PachiConsensus` wrapping `EthBeaconConsensus`
- [ ] `validate_block_pre_execution`:
  - [ ] OracleUpdate system tx must exist
  - [ ] All supported assets present in OracleUpdate
  - [ ] No unsupported assets
  - [ ] Engine version match
  - [ ] Per-asset validation (tolerance check, timestamp check)
  - [ ] VRF fulfill ordering (each fulfill after its request)
  - [ ] VRF fulfill only for successful requestVRF txs
  - [ ] Custom tx type validation (SessionTx, SponsoredTx structure)
- [ ] `validate_block_post_execution`:
  - [ ] Oracle state correctly applied
  - [ ] VRF results correctly stored
- [ ] Unit tests: valid/invalid block scenarios, missing OracleUpdate, wrong ordering

#### L4-4: `pachi-pool`

Crate: `crates/pachi/pool/`

- [ ] Create crate
- [ ] SessionTx mempool validation:
  - [ ] Session exists and is Active
  - [ ] Session not expired
  - [ ] Signer signature valid
  - [ ] Session nonce correct
  - [ ] Policy compliance (target, selector, constraints, limits)
  - [ ] Fee limit not exceeded
- [ ] SponsoredTx mempool validation:
  - [ ] Sponsor active
  - [ ] Policy valid
  - [ ] Sender allowed
  - [ ] Balance sufficient (Deposit) or limits OK (Mint)
- [ ] SessionSponsoredTx: combined validation
- [ ] Session nonce-based ordering
- [ ] Unit tests: acceptance/rejection scenarios for each tx type

---

### Layer 5: Node Assembly

Depends on: all layers.

#### L5-1: `pachi-node`

Crate: `crates/pachi/node/`

- [ ] Create crate
- [ ] Define `PachiNode` implementing `NodeTypes`
- [ ] Implement `PachiExecutorBuilder` -> returns `PachiEvmConfig`
- [ ] Implement `PachiPayloadServiceBuilder` -> returns `PachiPayloadBuilder`
- [ ] Implement `PachiConsensusBuilder` -> returns `PachiConsensus`
- [ ] Wire all components via `ComponentsBuilder`
- [ ] Add Oracle Engine startup to node lifecycle
- [ ] Integration test: start node, produce blocks, verify system txs

#### L5-2: `pachi-rpc`

Crate: `crates/pachi/rpc/`

- [ ] Create crate
- [ ] `session_*` namespace:
  - [ ] `session_prepareCreate` -> tx data + session hash
  - [ ] `session_getSession` -> session status
  - [ ] `session_listSessions` -> active sessions for authorizer
  - [ ] `session_simulateTx` -> policy validation + remaining limits
- [ ] `sponsor_*` namespace:
  - [ ] `sponsor_getInfo` -> balance, status, type, config
  - [ ] `sponsor_canSponsor` -> eligibility check
  - [ ] `sponsor_estimateGas` -> gas estimate with sponsor overhead
- [ ] `oracle_*` namespace:
  - [ ] `oracle_getPrice` -> current price data
  - [ ] `oracle_getEngineStatus` -> engine health, source status
- [ ] Integration tests with running node

#### L5-3: `pachi-genesis`

Crate: `crates/pachi/genesis/`

- [ ] Create crate
- [ ] Define Pachi genesis config struct (extends Ethereum genesis)
- [ ] Initialize precompile state at genesis:
  - [ ] Oracle: all assets at (0, 0, Unavailable)
  - [ ] VRF: public key initialized
  - [ ] SessionRegistry: empty
  - [ ] SponsorHub: owner set from genesis config
- [ ] Apply session/sponsor parameter defaults from genesis config
- [ ] Unit tests: genesis state initialization

---

### E2E Testing

After all layers are complete:

- [ ] Full node startup and block production
- [ ] VRF lifecycle: placeBet -> requestVRF -> fulfill (same block) -> claim
- [ ] Session key lifecycle: create -> use -> expire/revoke
- [ ] Gas sponsor lifecycle: register -> deposit -> sponsor txs -> withdraw
- [ ] Combined: Session + Sponsor tx
- [ ] Oracle: prices update per block, Unavailable handling
- [ ] Edge cases: empty blocks, max session slots, sponsor balance exhaustion

---

## Phase 2: Mainnet (after Phase 1 complete)

- [ ] IBFT2 consensus engine implementation
- [ ] DKG protocol for threshold VRF key generation
- [ ] Threshold VRF (partial exchange via internal P2P)
- [ ] VRF_COMPUTE precompile internal update (single key -> threshold)
- [ ] Slashing contract and staking mechanism
- [ ] Oracle consensus validation strengthening (multi-validator cross-check)
- [ ] Seasonal key rotation (grace period, key destruction, new DKG)
- [ ] Sequencer onboarding/offboarding procedures

---

## Optional Enhancements (post-launch)

- [ ] Dynamic type constraints (bytes, string, arrays)
- [ ] Session policy registry (on-chain policy caching to reduce SessionTx calldata)
- [ ] Batched session transactions
- [ ] Cross-contract constraints
- [ ] Sponsor marketplace
- [ ] Session key rotation (signer change, state preserved)
- [ ] Governance upgrade (single owner -> multisig -> validator vote)
- [ ] Oracle Unchanged optimization (skip redundant price updates)
- [ ] Median-of-Reporters oracle enhancement
