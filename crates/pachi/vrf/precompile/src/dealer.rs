//! Dealer state machine for VRF request lifecycle.
//!
//! Manages: request → fulfill → query.
//! Key = `keccak256(requester || seed)`.
//!
//! Storage layout uses the `PachiState` trait from `pachi-oracle-precompile`.

use alloy_primitives::{address, keccak256, Address, B256, U256};
use pachi_oracle_precompile::PachiState;
use pachi_vrf_core::{vrf_verify as crypto_verify, VrfProof};

use crate::error::VrfPrecompileError;

/// The Dealer contract address (VRF state lives here).
pub const DEALER_ADDRESS: Address = address!("0x0000000000000000000000000000000000000101");

/// Status of a VRF request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VrfStatus {
    /// Request is pending fulfillment.
    Pending = 0,
    /// Request has been fulfilled with a random value.
    Fulfilled = 1,
}

impl VrfStatus {
    /// Converts a `u8` to a `VrfStatus`.
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Pending),
            1 => Some(Self::Fulfilled),
            _ => None,
        }
    }
}

/// A VRF request record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfRequest {
    /// Game contract that called requestVRF.
    pub requester: Address,
    /// Original seed.
    pub seed: B256,
    /// Request status.
    pub status: VrfStatus,
    /// Random value (set on fulfill).
    pub random_value: B256,
    /// Block number where request was made.
    pub block_number: u64,
    /// Gas prepaid for fulfill.
    pub prepaid_gas: U256,
}

/// Storage slot computation helpers.
///
/// Each VRF request occupies 6 storage slots starting from a base computed
/// from `keccak256("vrf_request", key)`.
mod slots {
    use alloy_primitives::{keccak256, B256, U256};

    /// Computes the base storage slot for a VRF request.
    pub(super) fn request_base(key: &B256) -> U256 {
        let hash = keccak256([b"vrf_request".as_slice(), key.as_slice()].concat());
        U256::from_be_bytes(hash.0)
    }

    /// Slot offsets within a VRF request record.
    pub(super) const REQUESTER: U256 = U256::from_limbs([0, 0, 0, 0]);
    pub(super) const SEED: U256 = U256::from_limbs([1, 0, 0, 0]);
    pub(super) const STATUS: U256 = U256::from_limbs([2, 0, 0, 0]);
    pub(super) const RANDOM_VALUE: U256 = U256::from_limbs([3, 0, 0, 0]);
    pub(super) const BLOCK_NUMBER: U256 = U256::from_limbs([4, 0, 0, 0]);
    pub(super) const PREPAID_GAS: U256 = U256::from_limbs([5, 0, 0, 0]);
}

/// Dealer: manages VRF request lifecycle.
#[derive(Debug)]
pub struct Dealer;

impl Dealer {
    /// Computes the unique key for a VRF request.
    ///
    /// `key = keccak256(requester || seed)`
    pub fn compute_key(requester: Address, seed: B256) -> B256 {
        keccak256([requester.as_slice(), seed.as_slice()].concat())
    }

    /// Submits a new VRF request.
    ///
    /// Stores the request record and returns the key.
    /// Rejects duplicate requests (same key).
    pub fn request_vrf(
        state: &mut impl PachiState,
        requester: Address,
        seed: B256,
        prepaid_gas: U256,
        block_number: u64,
    ) -> Result<B256, VrfPrecompileError> {
        let key = Self::compute_key(requester, seed);

        // Check for duplicate
        if Self::exists(state, &key) {
            return Err(VrfPrecompileError::DuplicateRequest { key });
        }

        // Write request record
        let base = slots::request_base(&key);
        state.set_storage(
            DEALER_ADDRESS,
            base + slots::REQUESTER,
            U256::from_be_bytes(requester.into_word().0),
        );
        state.set_storage(DEALER_ADDRESS, base + slots::SEED, U256::from_be_bytes(seed.0));
        state.set_storage(
            DEALER_ADDRESS,
            base + slots::STATUS,
            U256::from(VrfStatus::Pending as u8),
        );
        state.set_storage(DEALER_ADDRESS, base + slots::RANDOM_VALUE, U256::ZERO);
        state.set_storage(DEALER_ADDRESS, base + slots::BLOCK_NUMBER, U256::from(block_number));
        state.set_storage(DEALER_ADDRESS, base + slots::PREPAID_GAS, prepaid_gas);

        Ok(key)
    }

    /// Fulfills a VRF request with a verified random value.
    ///
    /// Verifies the proof against the stored seed, then stores the result.
    pub fn fulfill(
        state: &mut impl PachiState,
        key: &B256,
        random_value: B256,
        proof: &VrfProof,
        public_key: &[u8; 33],
    ) -> Result<(), VrfPrecompileError> {
        let request = Self::get_request(state, key)?;

        if request.status == VrfStatus::Fulfilled {
            return Err(VrfPrecompileError::AlreadyFulfilled { key: *key });
        }

        // Verify proof against stored seed
        // The final_seed used for VRF is the key itself (keccak256(requester || seed))
        let valid = crypto_verify(public_key, key, &random_value, proof)
            .map_err(|e| VrfPrecompileError::CryptoError(e.to_string()))?;

        if !valid {
            return Err(VrfPrecompileError::ProofVerificationFailed);
        }

        // Update status and random_value
        let base = slots::request_base(key);
        state.set_storage(
            DEALER_ADDRESS,
            base + slots::STATUS,
            U256::from(VrfStatus::Fulfilled as u8),
        );
        state.set_storage(
            DEALER_ADDRESS,
            base + slots::RANDOM_VALUE,
            U256::from_be_bytes(random_value.0),
        );

        Ok(())
    }

    /// Gets the result of a fulfilled VRF request.
    ///
    /// Returns `None` if the request doesn't exist or is not fulfilled.
    pub fn get_result(state: &impl PachiState, key: &B256) -> Option<B256> {
        let request = Self::get_request(state, key).ok()?;
        if request.status != VrfStatus::Fulfilled {
            return None;
        }
        Some(request.random_value)
    }

    /// Checks whether a VRF request has been fulfilled.
    pub fn is_fulfilled(state: &impl PachiState, key: &B256) -> bool {
        Self::get_request(state, key).map(|r| r.status == VrfStatus::Fulfilled).unwrap_or(false)
    }

    /// Reads a VRF request from state.
    pub fn get_request(
        state: &impl PachiState,
        key: &B256,
    ) -> Result<VrfRequest, VrfPrecompileError> {
        if !Self::exists(state, key) {
            return Err(VrfPrecompileError::RequestNotFound { key: *key });
        }

        let base = slots::request_base(key);

        let requester_word = state.get_storage(DEALER_ADDRESS, base + slots::REQUESTER);
        let requester = Address::from_word(B256::from(requester_word.to_be_bytes()));

        let seed_val = state.get_storage(DEALER_ADDRESS, base + slots::SEED);
        let seed = B256::from(seed_val.to_be_bytes());

        let status_val = state.get_storage(DEALER_ADDRESS, base + slots::STATUS);
        let status =
            VrfStatus::from_u8(status_val.as_limbs()[0] as u8).unwrap_or(VrfStatus::Pending);

        let rv_val = state.get_storage(DEALER_ADDRESS, base + slots::RANDOM_VALUE);
        let random_value = B256::from(rv_val.to_be_bytes());

        let block_val = state.get_storage(DEALER_ADDRESS, base + slots::BLOCK_NUMBER);
        let block_number = block_val.as_limbs()[0];

        let prepaid_gas = state.get_storage(DEALER_ADDRESS, base + slots::PREPAID_GAS);

        Ok(VrfRequest { requester, seed, status, random_value, block_number, prepaid_gas })
    }

    /// Checks whether a request key exists in state.
    ///
    /// A request exists if any of its fields are non-zero.
    /// We check the requester field (first slot).
    fn exists(state: &impl PachiState, key: &B256) -> bool {
        let base = slots::request_base(key);
        let requester = state.get_storage(DEALER_ADDRESS, base + slots::REQUESTER);
        !requester.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pachi_oracle_precompile::MockState;
    use pachi_vrf_core::generate_keypair;

    fn setup() -> ([u8; 32], [u8; 33], MockState) {
        let (sk, pk) = generate_keypair();
        let state = MockState::new();
        (sk, pk, state)
    }

    #[test]
    fn request_and_fulfill_lifecycle() {
        let (sk, pk, mut state) = setup();
        let requester = Address::from([0xAAu8; 20]);
        let seed = B256::from([0xBBu8; 32]);
        let prepaid_gas = U256::from(100_000u64);

        // Request
        let key = Dealer::request_vrf(&mut state, requester, seed, prepaid_gas, 42).unwrap();

        // Check request stored
        let request = Dealer::get_request(&state, &key).unwrap();
        assert_eq!(request.requester, requester);
        assert_eq!(request.seed, seed);
        assert_eq!(request.status, VrfStatus::Pending);
        assert_eq!(request.block_number, 42);
        assert_eq!(request.prepaid_gas, prepaid_gas);
        assert!(!Dealer::is_fulfilled(&state, &key));

        // Compute VRF (using key as the final seed)
        let (random_value, proof) = pachi_vrf_core::vrf_compute(&sk, &key).unwrap();

        // Fulfill
        Dealer::fulfill(&mut state, &key, random_value, &proof, &pk).unwrap();

        // Check fulfilled
        assert!(Dealer::is_fulfilled(&state, &key));
        assert_eq!(Dealer::get_result(&state, &key), Some(random_value));

        let request = Dealer::get_request(&state, &key).unwrap();
        assert_eq!(request.status, VrfStatus::Fulfilled);
        assert_eq!(request.random_value, random_value);
    }

    #[test]
    fn duplicate_request_rejected() {
        let (_, _, mut state) = setup();
        let requester = Address::from([0xCCu8; 20]);
        let seed = B256::from([0xDDu8; 32]);

        Dealer::request_vrf(&mut state, requester, seed, U256::ZERO, 1).unwrap();

        let err = Dealer::request_vrf(&mut state, requester, seed, U256::ZERO, 2).unwrap_err();
        assert!(matches!(err, VrfPrecompileError::DuplicateRequest { .. }));
    }

    #[test]
    fn different_requesters_same_seed_allowed() {
        let (_, _, mut state) = setup();
        let seed = B256::from([0xEEu8; 32]);

        let key1 =
            Dealer::request_vrf(&mut state, Address::from([0x01u8; 20]), seed, U256::ZERO, 1)
                .unwrap();
        let key2 =
            Dealer::request_vrf(&mut state, Address::from([0x02u8; 20]), seed, U256::ZERO, 1)
                .unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn fulfill_nonexistent_request() {
        let (sk, pk, mut state) = setup();
        let fake_key = B256::from([0xFFu8; 32]);
        let (random_value, proof) = pachi_vrf_core::vrf_compute(&sk, &fake_key).unwrap();

        let err = Dealer::fulfill(&mut state, &fake_key, random_value, &proof, &pk).unwrap_err();
        assert!(matches!(err, VrfPrecompileError::RequestNotFound { .. }));
    }

    #[test]
    fn double_fulfill_rejected() {
        let (sk, pk, mut state) = setup();
        let requester = Address::from([0xAAu8; 20]);
        let seed = B256::from([0xBBu8; 32]);
        let key = Dealer::request_vrf(&mut state, requester, seed, U256::ZERO, 1).unwrap();

        let (random_value, proof) = pachi_vrf_core::vrf_compute(&sk, &key).unwrap();
        Dealer::fulfill(&mut state, &key, random_value, &proof, &pk).unwrap();

        let err = Dealer::fulfill(&mut state, &key, random_value, &proof, &pk).unwrap_err();
        assert!(matches!(err, VrfPrecompileError::AlreadyFulfilled { .. }));
    }

    #[test]
    fn fulfill_with_wrong_proof_rejected() {
        let (sk, pk, mut state) = setup();
        let requester = Address::from([0xAAu8; 20]);
        let seed = B256::from([0xBBu8; 32]);
        let key = Dealer::request_vrf(&mut state, requester, seed, U256::ZERO, 1).unwrap();

        // Compute VRF for a different seed
        let wrong_seed = B256::from([0xFFu8; 32]);
        let (random_value, proof) = pachi_vrf_core::vrf_compute(&sk, &wrong_seed).unwrap();

        let err = Dealer::fulfill(&mut state, &key, random_value, &proof, &pk).unwrap_err();
        assert!(matches!(err, VrfPrecompileError::ProofVerificationFailed));
    }

    #[test]
    fn get_result_pending_returns_none() {
        let (_, _, mut state) = setup();
        let requester = Address::from([0xAAu8; 20]);
        let seed = B256::from([0xBBu8; 32]);
        let key = Dealer::request_vrf(&mut state, requester, seed, U256::ZERO, 1).unwrap();

        assert_eq!(Dealer::get_result(&state, &key), None);
    }

    #[test]
    fn get_result_nonexistent_returns_none() {
        let state = MockState::new();
        let key = B256::from([0xAAu8; 32]);
        assert_eq!(Dealer::get_result(&state, &key), None);
    }

    #[test]
    fn is_fulfilled_nonexistent_returns_false() {
        let state = MockState::new();
        let key = B256::from([0xAAu8; 32]);
        assert!(!Dealer::is_fulfilled(&state, &key));
    }

    #[test]
    fn compute_key_deterministic() {
        let requester = Address::from([0xAAu8; 20]);
        let seed = B256::from([0xBBu8; 32]);

        let key1 = Dealer::compute_key(requester, seed);
        let key2 = Dealer::compute_key(requester, seed);
        assert_eq!(key1, key2);
    }

    #[test]
    fn compute_key_different_for_different_inputs() {
        let addr1 = Address::from([0x01u8; 20]);
        let addr2 = Address::from([0x02u8; 20]);
        let seed = B256::from([0xAAu8; 32]);

        assert_ne!(Dealer::compute_key(addr1, seed), Dealer::compute_key(addr2, seed));

        let seed2 = B256::from([0xBBu8; 32]);
        assert_ne!(Dealer::compute_key(addr1, seed), Dealer::compute_key(addr1, seed2));
    }
}
