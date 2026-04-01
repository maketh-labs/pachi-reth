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

- [x] Create crate
- [x] Create `PachiPrecompiles`: register all 5 precompiles into `PrecompilesMap`
  - [x] 0x0101 VRF_COMPUTE (delegates to pachi-vrf-precompile)
  - [x] 0x0102 VRF_VERIFY
  - [x] 0x0800 SessionRegistry
  - [x] 0x0801 SponsorHub
  - [x] 0x0802 PriceOracle
- [x] Create `PachiEvmFactory` implementing `EvmFactory` trait
- [x] Create `PachiEvmConfig` implementing `ConfigureEvm` trait
- [x] Implement pre-execution handler:
  - [x] SessionTx: validate session, check policy, verify signature
  - [x] SponsoredTx: validate sponsor, lock balance (Deposit) or check limits (Mint)
  - [x] SessionSponsoredTx: session first, then sponsor
  - [x] Set msg.sender appropriately per tx type
- [x] Implement post-execution handler:
  - [x] SessionTx: update nonce, update limits
  - [x] SponsoredTx (Deposit): deduct actual gas, refund excess, update limits
  - [x] SponsoredTx (Mint): mint native token to sequencer, update limits
  - [x] SessionSponsoredTx: session limits + sponsor settlement
- [x] Integration tests: execute each tx type through EVM, verify state changes

> **Note (L3-1): Implementation Details for Layer 4+ Consumers**
>
> **Architecture:**
> - `PachiEvmConfig` is a drop-in replacement for `EthEvmConfig`. Usage: `PachiEvmConfig::new(chain_spec)`.
> - `PachiEvmFactory` adds 5 precompiles on top of `Precompiles::prague()` via `DynPrecompile::new_stateful`.
> - `EvmStateBridge` wraps `EvmInternals` in `RefCell` to bridge `PachiState` ↔ revm journaled state.
> - All precompiles have full `EvmInternals` access (sload/sstore/log) through `PrecompileInput.internals`.
>
> **Handlers are NOT wired into the executor — Layer 4 must do this:**
> - `PreExecutionHandler` and `PostExecutionHandler` are pure functions on `&impl PachiState`.
> - Layer 4 (`pachi-payload`) must:
>   1. Detect custom tx types (0x04, 0x05, 0x06) in the executor pipeline.
>   2. Call `PreExecutionHandler::validate_*` before EVM execution.
>   3. Set `TxEnv.caller` to `authorizer` for SessionTx/SessionSponsoredTx.
>   4. Call `PostExecutionHandler::finalize_*` after EVM execution.
>   5. For `SettlementAction::MintRequired`, mint native tokens to fee recipient.
>   6. For `PachiSystemTx` (0x50), execute with gas_price=0 and sender=SYSTEM_ADDRESS.
>   7. Deduct SessionTx 15,000 gas overhead from gas_limit.
>   8. Emit `Sponsored` event after sponsor settlement (not emitted by L3 precompile).
>
> **Precompile function coverage (spec vs implementation):**
>
> | Precompile | Spec functions | Implemented | Notes |
> |-----------|---------------|-------------|-------|
> | PriceOracle (0x0802) | 3 | 3 (100%) | ABI matches Solidity interface |
> | VRF_COMPUTE (0x0101) | 1 | 1 (100%) | Phase 1: raw secret_key + seed |
> | VRF_VERIFY (0x0102) | 1 | 1 (100%) | Fixed 33-byte compressed pubkey |
> | SessionRegistry (0x0800) | 5 | 5 (100%) | createSession uses decomposed params (hash-only model) |
> | SponsorHub (0x0801) | 13 | 13+1 (100%) | +getValidUntil extra. registerPolicy accepts RLP |
>
> **SponsorConfig on-chain storage:**
> - `registerPolicy(bytes)` accepts RLP-encoded full `SponsorConfig` (allowed_senders, call_policies, transfer_policies, all limits, valid_until).
> - 5 scalar fields at `keccak256("sponsor_record", sponsor) + offset` (status, type, balance, created_at, valid_until).
> - Full config as RLP blob at `keccak256("sponsor_config", sponsor) + offset` (slot 0 = byte length, slot 1..N = data).
> - `getPolicy(address)` returns stored RLP bytes. `canSponsor(...)` deserializes and runs `SponsorValidator`.
> - RLP Encodable/Decodable implemented for all primitives + sponsor types. Client SDK must RLP-encode.
>
> **Event coverage:**
> - SessionRegistry: `SessionCreated` ✅, `SessionRevoked` ✅, `SessionReplaced` ✅ (emitted when slot replacement occurs during createSession).
> - SponsorHub: `Deposited` ✅, `Withdrawn` ✅, `PolicyRegistered` ✅, `PolicyDeactivated` ✅, `MintApproved` ✅, `MintRevoked` ✅, `OwnershipTransferred` ✅, `Sponsored` ❌ (Layer 4 responsibility — emitted per-tx after sponsor settlement).
>
> **Function selectors:** All computed as `keccak256(canonical_signature)[:4]`. See dispatch file constants.
>
> **`getPriceBatch` ABI:** Returns `(uint256[], uint64[], uint8[])` as three separate dynamic arrays with proper offset/length encoding.
>
> **Sponsor limit key deviation (from L1-4):**
> - Session: `keccak256("limit_state", session_hash, context)` (spec pattern).
> - Sponsor: `keccak256("sponsor_limit", sponsor, context)` / `keccak256("sponsor_sender_limit", sponsor, sender, context)`.
> - No collision. Alignment deferred to future if unified limit dashboard needed.
>
> **L1 visibility changes made in L3:**
> - `pachi_session_precompile::storage_keys` module → `pub` (was `pub(crate)`). `SessionValidationInput`, `limit_state_key` added to exports.
> - `pachi_sponsor_precompile`: `SponsorConfig`, `SponsorCallPolicy`, `SponsorTransferPolicy` added to exports.
> - `pachi_primitives`: `alloy-rlp` dependency added. `RlpEncodable`/`RlpDecodable` derives on `Limit`, `LimitState`, `Constraint`, `CallPolicy`, `TransferPolicy`.
> - `pachi_sponsor_precompile`: `alloy-rlp` dependency added. RLP derives on `SponsorConfig`, `SponsorCallPolicy`, `SponsorTransferPolicy`, `SponsorRecord`. `GAS_SPONSOR_DEACTIVATE` (10,000) added.
>
> **Design decisions for Phase 1:**
> - VRF_COMPUTE takes raw `(secret_key: bytes32, seed: bytes32)` instead of spec's `(secret_key_index, seed)`. Phase 1 sequencer holds the key in process directly. Phase 2 will change to threshold VRF format.
> - SessionRegistry `createSession(address,uint64,bytes32)` takes decomposed params instead of spec's `createSession(SessionConfig)`. This matches the "hash-only storage" model where full config stays off-chain and only the hash is on-chain. The client computes `session_hash = keccak256(abi.encode(authorizer, config))` before calling.

---

### Layer 4: Block Building & Validation

Depends on: Layer 3.

#### L4-1: `pachi-oracle-engine`

Crate: `crates/pachi/oracle/engine/`

- [x] Create crate
- [x] Implement WebSocket connection manager (5 sources)
  - [x] Binance WS connector
  - [x] Coinbase WS connector
  - [x] OKX WS connector
  - [x] Bybit WS connector
  - [x] Kraken WS connector
- [x] Implement reconnection logic with backoff
- [x] Implement Aggregator (staleness filter -> outlier trim -> median -> confidence)
- [x] Implement Local Price State (per-asset AssetState)
- [x] Implement `generate_snapshot()` -> PriceSnapshot for block building
- [x] Implement heartbeat (1s aggregation cycle)
- [x] Unit tests: aggregator with various source scenarios
- [x] Integration tests: mock WebSocket sources, verify snapshot generation

#### L4-2: `pachi-payload`

Crate: `crates/pachi/payload/`

> **Dependency on L3:** Use `PachiEvmConfig` instead of `EthEvmConfig` when creating the executor. The custom tx type routing (SessionTx/SponsoredTx/SessionSponsoredTx pre/post handlers) must be integrated into the payload builder's transaction execution loop. See `pachi_evm::handlers::{PreExecutionHandler, PostExecutionHandler}`.

- [x] Create crate
- [x] Implement `PachiPayloadBuilder` wrapping `EthereumPayloadBuilder`
- [ ] Wire custom tx type handling into execution loop:
  - [ ] Before executing SessionTx/SponsoredTx/SessionSponsoredTx: call PreExecutionHandler
  - [ ] Modify TxEnv.caller to authorizer for SessionTx/SessionSponsoredTx
  - [ ] After execution: call PostExecutionHandler, handle SettlementAction::MintRequired
  - [ ] Deduct SessionTx 15,000 gas overhead from gas_limit
  > **Blocked:** `default_ethereum_payload()` creates and owns the `BlockBuilder` internally. The tx execution loop is a monolithic closure with no hook points. These require a custom executor pipeline in L5 (`PachiExecutorBuilder`) that wraps the block executor with pre/post hooks per transaction.
- [ ] After user tx execution:
  - [ ] Scan receipts/logs for VRF requestVRF events
  > **Blocked:** Receipt/log access requires executor pipeline integration (receipts are available only after `BlockBuilder::finish()`).
  - [x] For each request: call VRF_COMPUTE, generate fulfill system tx (`SystemTxGenerator::vrf_fulfill_tx()`)
  - [ ] Verify fulfill tx is positioned after its request tx
- [x] Generate OracleUpdate system tx from Oracle Engine snapshot (`SystemTxGenerator::oracle_update_tx()`)
- [ ] Append system txs at block end (VRF fulfills first, then OracleUpdate)
  > **Blocked:** Requires `BlockBuilder::execute_transaction()` call before `finish()`. Only accessible in L5 custom executor.
- [x] Handle edge cases: no VRF requests, all oracle sources down
- [ ] Integration tests: build blocks with VRF requests, verify system tx injection

> **Note (L4-2): What's done and what's blocked**
>
> **Done (L4):**
> - `PachiPayloadBuilder` struct wrapping `EthereumPayloadBuilder` with `Arc<OracleEngine>` and `SystemTxGenerator`.
> - `SystemTxGenerator::oracle_update_tx(engine, nonce)` → JSON-serialized `PriceSnapshot`.
> - `SystemTxGenerator::vrf_fulfill_tx(key, seed, nonce)` → VRF compute + proof bytes.
> - `SystemTxGenerator::encode_system_tx(tx)` → EIP-2718 typed envelope.
>
> **Blocked (needs L5 custom executor):**
> - Pre/post execution handler calls for custom tx types (0x04, 0x05, 0x06).
> - System tx injection into the actual block body.
> - VRF request event scanning from receipts.
> - The technical reason: reth's `default_ethereum_payload()` is a monolithic function that creates and owns the `BlockBuilder`. The `PayloadBuilder::try_build()` trait method can only call this function — it cannot inject hooks into the tx execution loop. L5's `PachiExecutorBuilder` must create a custom executor that wraps each tx execution with pre/post hooks.

#### L4-3: `pachi-consensus`

Crate: `crates/pachi/consensus/`

- [x] Create crate
- [x] Implement `PachiConsensus` wrapping `EthBeaconConsensus`
- [x] `validate_block_pre_execution`:
  - [x] OracleUpdate system tx must exist
  - [x] All supported assets present in OracleUpdate
  - [x] No unsupported assets
  - [x] Engine version match
  - [x] Per-asset validation (zero price/timestamp sanity checks)
  - [ ] Per-asset tolerance check (VALIDATION_TOLERANCE_BPS=100)
  > **Phase 2:** Tolerance check requires each validator to run its own oracle engine and compare prices. Phase 1 single-sequencer trust model skips this.
  - [x] VRF fulfill ordering (each fulfill after its request)
  - [ ] VRF fulfill only for successful requestVRF txs
  > **Blocked:** Pre-execution validation cannot check tx success — receipts are only available post-execution. Must be validated in `validate_block_post_execution`.
  - [x] Custom tx type validation (SessionTx, SponsoredTx structure)
- [ ] `validate_block_post_execution`:
  - [ ] Oracle state correctly applied
  - [ ] VRF results correctly stored
  > **Blocked:** Pachi-specific post-execution checks require reading precompile storage slots from the executed state. Needs `StateProvider` access in the `FullConsensus::validate_block_post_execution` context. Currently delegates to Ethereum base validation only.
- [x] Unit tests: valid/invalid block scenarios, missing OracleUpdate, wrong ordering

> **Note (L4-3): What's done and what's blocked**
>
> **Done (L4):**
> - `PachiConsensus` wraps `EthBeaconConsensus` — all Ethereum validation delegated, Pachi rules added.
> - Pre-execution: system tx ordering, OracleUpdate existence/uniqueness, snapshot content (engine version, asset coverage, per-asset sanity: zero price/timestamp rejected), VRF fulfill before OracleUpdate.
> - Error types: `PachiConsensusError` with 9 variants (including `VrfFulfillAfterOracleUpdate`, `InvalidAssetPrice`).
> - 7 unit tests covering: valid snapshot, all-unavailable, wrong version, missing asset, zero price, zero timestamp, mixed entries.
>
> **L5 (needs executor context):**
> - VRF fulfill for successful-only requests: needs post-execution receipt access.
> - Post-execution oracle/VRF state verification: needs `StateProvider` in `validate_block_post_execution` context.
>
> **Phase 2:**
> - Per-asset tolerance check (`VALIDATION_TOLERANCE_BPS` = 100bps): requires each validator's own oracle engine data.

#### L4-4: `pachi-pool`

Crate: `crates/pachi/pool/`

> **Dependency on L3:** Pool validation can reuse L3's `PreExecutionHandler` for policy checks, or call L1 validators directly (`SessionValidator`, `SponsorValidator`). The pool does NOT need `EvmStateBridge` — it reads state from the provider, not from `EvmInternals`. Create a `ProviderStateBridge` that implements `PachiState` over the reth storage provider for pool-level validation.

- [x] Create crate
- [x] SessionTx mempool validation:
  - [x] Session exists and is Active
  - [x] Session not expired
  - [ ] Signer signature valid
  > **Not needed at pool level:** `ecrecover` is performed by reth's `EthTransactionValidator` during tx deserialization. Pool validator receives already-recovered sender. Session key → authorizer mapping is checked via `read_session_record()`.
  - [x] Session nonce correct
  - [ ] Policy compliance (target, selector, constraints, limits)
  > **Deferred to block building:** Full policy validation requires parsing the `session_config` bytes from the tx body and running `SessionValidator::validate()`. Pool-level validation is intentionally lightweight (existence + expiry + nonce) to avoid expensive RLP decode + storage reads per mempool insertion. Full validation runs in `PreExecutionHandler` at block building time.
  - [ ] Fee limit not exceeded
  > **Same reason:** Fee limit check requires `session_config` parsing.
- [x] SponsoredTx mempool validation:
  - [x] Sponsor active
  - [ ] Policy valid (target/selector check)
  > **Deferred to block building:** Requires RLP-decoding the on-chain `SponsorConfig` and matching against the tx's `to`/`data`. Pool-level checks existence + balance only.
  - [ ] Sender allowed (allowed_senders check)
  > **Same reason:** Requires `SponsorConfig` decode.
  - [x] Balance sufficient (Deposit) or limits OK (Mint)
- [x] SessionSponsoredTx: combined validation
- [ ] Session nonce-based ordering
  > **Requires `TransactionOrdering` trait impl:** Pool ordering is configured at pool creation time in L5 (`pachi-node`). The `PachiPoolValidator` validates nonces but doesn't control ordering priority.
- [x] Unit tests: acceptance/rejection scenarios for each tx type

> **Note (L4-4): What's done and what's blocked**
>
> **Done (L4):**
> - `ProviderStateBridge`: read-only `PachiState` over `StateProviderBox`.
> - `PachiPoolValidator::validate_session_tx()`: existence, active, expiry, authorizer, nonce.
> - `PachiPoolValidator::validate_sponsored_tx()`: existence (`created_at != 0`), active, expiry, balance.
> - `PachiPoolValidator::validate_session_sponsored_tx()`: combined.
> - 10 unit tests covering accept/reject scenarios.
>
> **Intentionally lightweight (not blocked, design decision):**
> - Policy compliance (target/selector/constraints): requires RLP decoding `session_config` or `SponsorConfig`. Pool validation prioritizes throughput — full validation runs at block building in `PreExecutionHandler`.
> - Signer signature: already handled by reth's `EthTransactionValidator` during tx deserialization.
> - Session nonce ordering: requires `TransactionOrdering` trait impl, configured in L5.
>
> **L1 visibility changes made in L4:**
> - `pachi_session_precompile::storage_keys`: all functions `pub(crate)` → `pub`.
> - `pachi_session_precompile::slots::read_session_record`: `pub(crate)` → `pub`, exported from lib.
> - `pachi_sponsor_precompile::storage_keys`: module `pub(crate)` → `pub`, all functions `pub`.

---

### Layer 5: Node Assembly

Depends on: all layers.

> **L4→L5 Handoff: Getting Started Guide**
>
> Before implementing L5, read these files to understand the APIs you'll be wiring together:
>
> **1. Custom executor pipeline (pre/post handlers):**
> - `pachi_evm::PreExecutionHandler` (`crates/pachi/evm/src/handlers/pre_execution.rs`)
>   - `validate_session_tx(state: &impl PachiState, input: &SessionValidationInput) -> Result<PreExecutionResult, PreExecutionError>`
>   - `validate_sponsored_tx(state: &mut impl PachiState, validation, config, max_gas_cost) -> Result<PreExecutionResult, PreExecutionError>`
>   - `validate_session_sponsored_tx(state, session_input, sponsor_input, sponsor_config, max_gas_cost) -> Result<PreExecutionResult, PreExecutionError>`
> - `pachi_evm::PostExecutionHandler` (`crates/pachi/evm/src/handlers/post_execution.rs`)
>   - `finalize_session_tx(state, authorizer, session_hash, actual_gas_cost, block_timestamp, fee_limit, to, calldata, value, call_policies, transfer_policies) -> PostExecutionResult`
>   - `finalize_sponsored_tx(state, sponsor, sender, actual_gas_cost, locked_amount, block_timestamp, config) -> Result<PostExecutionResult, SponsorPrecompileError>`
>   - `finalize_session_sponsored_tx(...)` — combined
> - `PreExecutionResult` has variants: `Session { authorizer }`, `Sponsored { sponsor, locked_amount }`, `SessionSponsored { authorizer, sponsor, locked_amount }`
> - `PostExecutionResult` has `settlement: Option<SettlementAction>` — for `MintRequired { amount }`, mint native tokens to fee recipient
>
> **2. System tx generation:**
> - `pachi_payload::SystemTxGenerator` (`crates/pachi/payload/src/system_tx.rs`)
>   - `new(chain_id: u64)` → `with_vrf_secret_key(key: [u8; 32])`
>   - `oracle_update_tx(&self, engine: &OracleEngine, nonce: u64) -> PachiSystemTx`
>   - `vrf_fulfill_tx(&self, key: B256, seed: B256, nonce: u64) -> Option<PachiSystemTx>`
>   - `encode_system_tx(tx: &PachiSystemTx) -> Bytes`
>
> **3. Oracle engine:**
> - `pachi_oracle_engine::OracleEngine` (`crates/pachi/oracle/engine/src/engine.rs`)
>   - `new()` → `start()` — spawns background WS connectors + heartbeat
>   - `generate_snapshot() -> PriceSnapshot` — always returns valid snapshot (all Unavailable if sources down)
>
> **4. Consensus:**
> - `pachi_consensus::PachiConsensus` (`crates/pachi/consensus/src/lib.rs`)
>   - `new(chain_spec: Arc<ChainSpec>)` — wraps `EthBeaconConsensus`, adds Pachi validation
>
> **5. Pool validation:**
> - `pachi_pool::PachiPoolValidator` (`crates/pachi/pool/src/validator.rs`)
>   - `validate_session_tx(state, session_hash, authorizer, nonce, block_timestamp)`
>   - `validate_sponsored_tx(state, sponsor, block_timestamp, estimated_gas_cost)`
> - `pachi_pool::ProviderStateBridge` — wraps `StateProviderBox` as read-only `PachiState`
>
> **6. Tx types to detect:**
> - `pachi_tx::PachiTxType::Session` (0x04), `Sponsored` (0x05), `SessionSponsored` (0x06), `System` (0x50)
> - Use `PachiTxType::is_pachi_type(tx.ty())` to check
>
> **Key reth examples to reference:**
> - `examples/custom-evm/src/main.rs` — custom EVM config pattern
> - `examples/custom-node-components/src/main.rs` — custom pool builder pattern
> - `crates/ethereum/node/src/` — EthereumNode as reference for PachiNode

#### L5-1: `pachi-node`

> **Dependency on L3:** `PachiNode` should use `PachiEvmConfig` as the EVM type. The `ExecutorBuilder` implementation returns `PachiEvmConfig::new(chain_spec)`. See `examples/custom-evm/src/main.rs` for the pattern.

Crate: `crates/pachi/node/`

- [ ] Create crate
- [ ] Define `PachiNode` implementing `NodeTypes`
- [ ] Implement `PachiExecutorBuilder` -> returns `PachiEvmConfig`
- [ ] Implement `PachiPayloadServiceBuilder` -> returns `PachiPayloadBuilder`
- [ ] Implement `PachiConsensusBuilder` -> returns `PachiConsensus`
- [ ] Wire all components via `ComponentsBuilder`
- [ ] Add Oracle Engine startup to node lifecycle
- [ ] **Custom executor pipeline for Pachi tx types (carried from L4):**
  - [ ] Detect custom tx types (0x04, 0x05, 0x06) in executor
  - [ ] Call `PreExecutionHandler::validate_session_tx` / `validate_sponsored_tx` / `validate_session_sponsored_tx` before EVM execution
  - [ ] Set `TxEnv.caller` to authorizer for SessionTx/SessionSponsoredTx
  - [ ] Call `PostExecutionHandler::finalize_*` after EVM execution
  - [ ] For `SettlementAction::MintRequired`, mint native tokens to fee recipient
  - [ ] Deduct SessionTx 15,000 gas overhead from gas_limit
  - [ ] Emit `Sponsored` event after sponsor settlement
- [ ] **System tx injection into blocks (carried from L4):**
  - [ ] After user tx execution, scan receipts/logs for VRF `requestVRF` events
  - [ ] For each request: call `SystemTxGenerator::vrf_fulfill_tx()`, execute via `BlockBuilder::execute_transaction()`
  - [ ] Call `SystemTxGenerator::oracle_update_tx()`, execute as last tx in block
  - [ ] Verify ordering: VRF fulfills before OracleUpdate, all after user txs
- [ ] **Post-execution consensus checks (carried from L4):**
  - [ ] In `validate_block_post_execution`: verify oracle precompile storage matches OracleUpdate snapshot
  - [ ] In `validate_block_post_execution`: verify VRF results correctly stored for each fulfill
  - [ ] VRF fulfill only for successful requestVRF txs (check receipts)
- [ ] Integration test: start node, produce blocks, verify system txs
- [ ] Integration test: SessionTx lifecycle (create session → execute via session key → verify authorizer as msg.sender)
- [ ] Integration test: SponsoredTx lifecycle (register sponsor → deposit → sponsor tx → verify settlement)
- [ ] Integration test: VRF lifecycle (requestVRF → fulfill in same block → verify result)

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
