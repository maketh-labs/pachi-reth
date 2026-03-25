# Session Key System

## Overview

Pachi Chain natively supports session keys at the protocol level, enabling Web2-grade UX without external account abstraction infrastructure.

### Why L1 Native

| Aspect | ERC-4337 (Contract Layer) | L1 Native (This Design) |
|--------|--------------------------|-------------------------|
| EOA compatibility | Smart Account required | Existing EOA works directly |
| Gas overhead | High (UserOp parsing, EntryPoint) | Minimal (protocol-level verification) |
| Mempool complexity | Alt Mempool + Bundler needed | Existing mempool extended |
| Developer experience | Bundler/Paymaster infra required | JSON-RPC extension only |
| Security surface | EntryPoint contract dependency | Consensus-level verification |

### Design Principles

1. **EVM compatibility:** Existing Solidity contracts and tooling unaffected.
2. **Opt-in:** Only users of new TX types are affected.
3. **Composable:** Session Key and Gas Sponsor can be used independently or combined.
4. **Hash-only storage:** Session config stored off-chain; only hash + status on-chain (optimized for high session count).

---

## Limit Model (Shared Primitive)

Used across Session Key and Gas Sponsor systems for expressing "how much to allow."

```rust
struct Limit {
    limit_type: u8,    // 0=Unlimited, 1=Lifetime, 2=Allowance
    limit: u256,       // Quantity cap
    period: u64,       // Allowance only (seconds)
}
```

### Unlimited (limit_type = 0)
No restriction. High risk; operational guidance recommends avoiding. Protocol emits warning event.

### Lifetime (limit_type = 1)
Cumulative cap over the entire session or sponsor policy lifetime.

```
Tracking: used[limit_key] += consumed_amount
          require(used[limit_key] <= limit)
```

### Allowance (limit_type = 2)
Refilling cap per time period.

```
Tracking: current_window = block.timestamp / period
          if window[limit_key] != current_window:
              used[limit_key] = 0  // window transition -> reset
              window[limit_key] = current_window
          used[limit_key] += consumed_amount
          require(used[limit_key] <= limit)
```

### Limit State (On-chain)

```rust
struct LimitState {
    used: u256,        // Amount consumed in current window/lifetime
    last_window: u64,  // Allowance: last reset window number
}
```

Key structure:
```
limit_key = keccak256(owner_hash, limit_context, limit_index)
```

---

## SessionConfig (Policy -- Off-chain Storage)

```rust
struct SessionConfig {
    signer: address,                 // Session key address (ephemeral key)
    expires_at: u64,                 // Expiration (unix timestamp)
    fee_limit: Limit,                // Gas fee cap (total/periodic)
    call_policies: Vec<CallPolicy>,  // Whitelisted contract function calls
    transfer_policies: Vec<TransferPolicy>,  // Whitelisted native transfers
    metadata: bytes,                 // (optional) App-specific metadata
    salt: bytes32,                   // (optional) Prevent duplicate policy registration
}

struct CallPolicy {
    target: address,                 // Target contract
    selector: bytes4,                // Target function
    value_limit: Limit,              // Native value cap for this function
    max_value_per_use: u256,         // Max native value per single call
    constraints: Vec<Constraint>,    // Per-argument constraints
}

struct Constraint {
    index: u8,                       // Calldata argument index (0-based)
    condition: u8,                   // Comparison condition (see below)
    ref_value: bytes32,              // Reference value for comparison
    limit: Limit,                    // Cumulative/periodic cap on this argument's value
}

// Constraint condition enum:
//   0 = Unconstrained   (no condition, limit only)
//   1 = Equal           (arg == ref_value)
//   2 = Greater         (arg >  ref_value)
//   3 = Less            (arg <  ref_value)
//   4 = GreaterEqual    (arg >= ref_value)
//   5 = LessEqual       (arg <= ref_value)
//   6 = NotEqual        (arg != ref_value)

struct TransferPolicy {
    target: address,                 // Transfer recipient
    max_value_per_use: u256,         // Max per transfer
    value_limit: Limit,              // Total transfer cap
}
```

**v1 constraint limitation:** Only static ABI types (uint, int, address, bool, bytesN) are supported. Dynamic types (bytes, string, arrays) use indirect ABI offset references and are deferred to future versions.

---

## Session Hash & On-chain Storage

### Hash Computation

```
session_hash = keccak256(abi.encode(authorizer, sessionConfig))
```

### On-chain Record

Policy content is NOT stored on-chain. Only session_hash and status.

```rust
struct SessionRecord {
    status: u8,          // 0=Active, 1=Revoked, 2=Expired
    authorizer: address, // Delegating wallet
    signer: address,     // Session key address (for fast lookup)
    expires_at: u64,     // Expiration (for fast status transition)
    created_at: u64,     // Creation time
}
```

State transitions:
```
Active -> Revoked (via revokeSession(), irreversible)
Active -> Expired (block.timestamp > expires_at)

Priority: Revoked > Expired > Active
```

### Session Slot Management

Each authorizer has a maximum of **N** session slots (genesis config, default 10).

```
createSession:
  1. Empty slot available -> place there
  2. No empty slot -> find Revoked or Expired slot
     2a. Found -> replace
     2b. Not found -> revert("MAX_SESSIONS_REACHED")

Replacement priority: Revoked first (oldest), then Expired (earliest expiry)
```

### Session Nonce

Session keys use a separate nonce space from the authorizer.

```
nonce_key = keccak256(authorizer, session_hash)
session_nonce[nonce_key] = 0, 1, 2, ...
```

Main wallet nonce and session nonce are fully independent -- concurrent transactions do not block each other.

---

## Session Validation Flow

```
SessionTx received (mempool or block execution)
|
+-- 1. session_hash existence check
|     -> Not found -> reject
|
+-- 2. Status check
|     +-- Revoked -> reject
|     +-- Expired (block.timestamp > expires_at) -> reject
|
+-- 3. Signer signature verification
|     -> ecrecover(tx_hash, sig) != session.signer -> reject
|
+-- 4. Session nonce check
|     -> tx.nonce != session_nonce[nonce_key] -> reject
|
+-- 5. Policy verification (hash policy included in tx against session_hash)
|     -> keccak256(encode(authorizer, policy)) != session_hash -> reject
|
+-- 6. feeLimit verification
|     -> Estimated gas exceeds fee limit -> reject
|
+-- 7. Call type branching
|     |
|     +-- [Contract Call] tx.data.length >= 4
|     |   +-- Find matching (tx.to, tx.data[:4]) in callPolicies
|     |   |   -> Not found -> reject("CALL_NOT_ALLOWED")
|     |   +-- tx.value <= maxValuePerUse?
|     |   +-- valueLimit check (Limit model)
|     |   +-- constraints check (per Constraint)
|     |       +-- condition comparison: arg[index] <condition> refValue
|     |       +-- constraint.limit check (Limit model)
|     |
|     +-- [Transfer] tx.data.length == 0 or < 4
|         +-- Find matching tx.to in transferPolicies
|         +-- tx.value <= maxValuePerUse?
|         +-- valueLimit check (Limit model)
|
+-- 8. Verification passed -> Execute
|     +-- msg.sender = authorizer (NOT the session key)
|     +-- Normal EVM execution
|
+-- 9. Post-Execution state updates
      +-- session_nonce[nonce_key]++
      +-- feeLimit usage update
      +-- valueLimit usage update
      +-- constraint limit usage update
```

### Constraint Verification Detail

```
Function: play(uint8 mode, uint256 amount, address recipient)
Calldata layout:
  [0:4]    = selector
  [4:36]   = mode       (index 0, uint8 padded to 32 bytes)
  [36:68]  = amount     (index 1)
  [68:100] = recipient  (index 2)

Argument extraction:
  arg_value = calldata[4 + index * 32 : 4 + (index + 1) * 32]
```

---

## Precompile: SessionRegistry (0x0800)

```
Address: 0x0000000000000000000000000000000000000800

Write Functions:
  createSession(SessionConfig config) -> bytes32 sessionHash
  revokeSession(bytes32 sessionHash)

View Functions:
  getSession(bytes32 sessionHash) -> (status, authorizer, signer, expiresAt, createdAt)
  getActiveSessions(address authorizer) -> bytes32[]
  isValid(bytes32 sessionHash) -> bool

Events:
  SessionCreated(bytes32 indexed sessionHash, address indexed authorizer, address signer, uint64 expiresAt)
  SessionRevoked(bytes32 indexed sessionHash, address indexed authorizer)
  SessionReplaced(bytes32 indexed oldHash, bytes32 indexed newHash, address indexed authorizer)
```

---

## Transaction Type: SessionTx (0x04)

```
SessionTx = 0x04 || rlp([
  chain_id,
  nonce,                        // session key nonce
  max_priority_fee_per_gas,
  max_fee_per_gas,
  gas_limit,
  to,
  value,
  data,
  access_list,

  // -- Session extension fields --
  session_hash,                 // bytes32
  authorizer,                   // address
  session_config,               // bytes: ABI-encoded SessionConfig (for hash verification)

  // -- Signature --
  session_key_v, session_key_r, session_key_s
])

Gas payer: authorizer
msg.sender: authorizer
```

Session policy is included in the tx (300B-4KB) for hash verification. `MAX_POLICY_SIZE = 4KB`.

---

## Gas Cost Overhead

| Item | Additional Gas | Description |
|------|---------------|-------------|
| SessionTx base overhead | 15,000 | Hash verification + policy parsing + limit checks |
| Constraint verification (each) | 500 | Argument extraction + comparison + limit check |
| Session creation | 60,000 | SessionRecord + slot write |
| Session revocation | 10,000 | Status update |
| Limit state update (each) | 5,000 | SSTORE (warm) |

---

## Genesis Configuration

```json
{
  "sessionConfig": {
    "maxSessionsPerAccount": 10,
    "maxSessionDuration": 2592000,
    "maxCallPoliciesPerSession": 20,
    "maxConstraintsPerCallPolicy": 10,
    "maxTransferPoliciesPerSession": 10,
    "maxPolicySize": 4096,
    "sessionRegistryAddress": "0x0000000000000000000000000000000000000800"
  }
}
```

---

## On-chain Storage Layout

```
Session Record:
  Key: keccak256("session_record", session_hash)
  Value: rlp(SessionRecord { status, authorizer, signer, expiresAt, createdAt })

Session Slots:
  Key: keccak256("session_slots", authorizer)
  Value: rlp(bytes32[N])

Session Nonce:
  Key: keccak256("session_nonce", authorizer, session_hash)
  Value: uint64

Limit State:
  Key: keccak256("limit_state", session_hash, limit_context)
  Value: rlp(LimitState { used, lastWindow })
```
