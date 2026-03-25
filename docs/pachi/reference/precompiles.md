# Precompile Reference

## Address Map

| Address | Name | Access | Gas |
|---------|------|--------|-----|
| `0x0000..0101` | VRF_COMPUTE | System account only | 3,000 (fixed) |
| `0x0000..0102` | VRF_VERIFY | Public | 1,500 (fixed) |
| `0x0000..0800` | SessionRegistry | Public | Varies (see below) |
| `0x0000..0801` | SponsorHub | Public (owner-only for governance) | Varies (see below) |
| `0x0000..0802` | PriceOracle | Public reads, system-only writes | Varies (see below) |

---

## VRF_COMPUTE (0x0101)

**System-only.** Reverts if called by non-system account.

### Input
```
Phase 1: abi.encode(secret_key_index, seed)
Phase 2: abi.encode(partial_shares[], seed)
```

### Output
```
abi.encode(random_value: bytes32, proof: bytes)
```

### Gas: 3,000 (fixed)

---

## VRF_VERIFY (0x0102)

**Public.** Anyone can verify a VRF result.

### Input
```
abi.encode(public_key: bytes, seed: bytes32, random_value: bytes32, proof: bytes)
```

### Output
```
abi.encode(valid: bool)
```

### Gas: 1,500 (fixed)

---

## SessionRegistry (0x0800)

### Functions

| Function | Selector | Gas | Access |
|----------|----------|-----|--------|
| `createSession(SessionConfig)` | TBD | 60,000 | Public (authorizer signs tx) |
| `revokeSession(bytes32)` | TBD | 10,000 | Authorizer only |
| `getSession(bytes32)` | TBD | 2,600 | Public (view) |
| `getActiveSessions(address)` | TBD | 2,600 + 200/session | Public (view) |
| `isValid(bytes32)` | TBD | 2,600 | Public (view) |

### Events

```
SessionCreated(bytes32 indexed sessionHash, address indexed authorizer, address signer, uint64 expiresAt)
SessionRevoked(bytes32 indexed sessionHash, address indexed authorizer)
SessionReplaced(bytes32 indexed oldHash, bytes32 indexed newHash, address indexed authorizer)
```

---

## SponsorHub (0x0801)

### Functions

| Function | Selector | Gas | Access |
|----------|----------|-----|--------|
| `registerPolicy(SponsorConfig)` | TBD | 100,000+ | Public |
| `deactivatePolicy()` | TBD | 10,000 | Sponsor only |
| `deposit()` | TBD | 20,000 | Deposit-mode sponsor |
| `withdraw(uint256)` | TBD | 20,000 | Deposit-mode sponsor |
| `approveMint(address)` | TBD | 10,000 | Owner only |
| `revokeMint(address)` | TBD | 10,000 | Owner only |
| `transferOwnership(address)` | TBD | 10,000 | Owner only |
| `getBalance(address)` | TBD | 2,600 | Public (view) |
| `getPolicy(address)` | TBD | 2,600+ | Public (view) |
| `getSponsorType(address)` | TBD | 2,600 | Public (view) |
| `isActive(address)` | TBD | 2,600 | Public (view) |
| `canSponsor(address,address,address,bytes4)` | TBD | 5,000 | Public (view) |
| `owner()` | TBD | 2,600 | Public (view) |

### Events

```
Deposited(address indexed sponsor, uint256 amount, uint256 newBalance)
Withdrawn(address indexed sponsor, uint256 amount, uint256 newBalance)
PolicyRegistered(address indexed sponsor, uint8 sponsorType, uint64 validUntil)
PolicyDeactivated(address indexed sponsor)
MintApproved(address indexed sponsor, address indexed approvedBy)
MintRevoked(address indexed sponsor, address indexed revokedBy)
Sponsored(address indexed sponsor, address indexed sender, bytes32 txHash, uint256 gasCost, uint8 sponsorType)
OwnershipTransferred(address indexed previousOwner, address indexed newOwner)
```

---

## PriceOracle (0x0802)

### Functions

| Function | Selector | Gas | Access |
|----------|----------|-----|--------|
| `getPrice(uint8)` | TBD | 200 | Public (view) |
| `getPriceBatch(uint8[])` | TBD | 200 + 100*len | Public (view) |
| `isSupported(uint8)` | TBD | 100 | Public (view) |

### Behavior

- Unsupported assetId -> revert
- Unavailable asset -> returns `(lastPrice, lastTimestamp, 3)` (does NOT revert)
- Never-set asset (post-genesis) -> returns `(0, 0, 3)`

### Supported Assets

| ID | Pair | Notes |
|----|------|-------|
| `0x01` | BTC/USD | |
| `0x02` | ETH/USD | |
| `0x03` | SOL/USD | |
| `0x04` | USDC/USD | Depeg detection |
| `0x05` | USDT/USD | Depeg detection |
| `0x06` | DAI/USD | Depeg detection |

Price format: `uint256` with 8 decimal places.
