# On-chain Storage Layout Reference

All Pachi features use deterministic storage key derivation based on keccak256 hashing.

---

## PriceOracle (0x0802)

2 slots per asset. No history.

```
slot(asset_id * 2)       -> price: uint256 (8 decimals)
slot(asset_id * 2 + 1)   -> packed {
    timestamp:  uint64   [bits 0:63]
    confidence: uint8    [bits 64:71]
}
```

---

## Dealer / VRF State

```
VRF Request:
  Key: keccak256("vrf_request", key)
  Value: rlp({
    requester: address,       // game contract that called requestVRF
    seed: bytes32,            // original seed
    status: u8,               // 0=Pending, 1=Fulfilled
    random_value: bytes32,    // filled on fulfill
    block_number: u64,        // block where request was made
    prepaid_gas: u256,        // gas prepaid for fulfill
  })

VRF Public Key:
  Key: keccak256("vrf_public_key")
  Value: bytes (compressed public key)

VRF Nonce (system):
  Key: keccak256("vrf_nonce")
  Value: u64
```

---

## SessionRegistry (0x0800)

```
Session Record:
  Key: keccak256("session_record", session_hash: bytes32)
  Value: rlp(SessionRecord {
    status: u8,          // 0=Active, 1=Revoked, 2=Expired
    authorizer: address,
    signer: address,
    expires_at: u64,
    created_at: u64,
  })

Session Slots (per authorizer):
  Key: keccak256("session_slots", authorizer: address)
  Value: rlp(bytes32[N])   // array of session_hashes, N = maxSessionsPerAccount

Session Nonce:
  Key: keccak256("session_nonce", authorizer: address, session_hash: bytes32)
  Value: u64
```

---

## SponsorHub (0x0801)

```
Sponsor Record:
  Key: keccak256("sponsor_record", sponsor: address)
  Value: rlp(SponsorRecord {
    status: u8,            // 0=Active, 1=Deactivated
    sponsor_type: u8,      // 0=Deposit, 1=Mint
    balance: u256,         // Deposit mode balance
    config: SponsorConfig, // Full policy
    created_at: u64,
  })

Governance Owner:
  Key: keccak256("sponsor_hub_owner")
  Value: address
```

---

## Shared Limit State

Used by both Session and Sponsor systems.

```
Limit State:
  Key: keccak256("limit_state", owner_hash: bytes32, limit_context: bytes, limit_index: u8)
  Value: rlp(LimitState {
    used: u256,           // consumed in current window or lifetime
    last_window: u64,     // Allowance: last reset window number
  })
```

### Owner Hash Derivation

| System | owner_hash |
|--------|-----------|
| Session | `session_hash` |
| Sponsor (global) | `keccak256(sponsor_address)` |
| Sponsor (per-sender) | `keccak256(sponsor_address, sender_address)` |

### Limit Context Examples

| Context | Encoding |
|---------|----------|
| Session fee limit | `"fee"` |
| Session call value limit | `"call", target, selector` |
| Session constraint limit | `"call_constraint", target, selector, index` |
| Session transfer limit | `"transfer", target` |
| Sponsor global fee limit | `"global_fee"` |
| Sponsor global tx limit | `"global_tx"` |
| Sponsor per-sender fee limit | `"sender_fee", sender` |
| Sponsor per-sender tx limit | `"sender_tx", sender` |
