//! Audit helper to verify that an off-chain promise matches on-chain execution.
///
//! # Threat being mitigated
///
//! Without this check, an off-chain actor (e.g. a relayer, a frontend, or a
//! compromised signing key) could submit a delegated action payload that *claims*
//! to perform one action (e.g. `delegate` to address A) but actually executes a
//! different action on-chain (e.g. `delegate` to address B, or `revoke_delegation`).
///
//! By hashing the intended action off-chain and passing that hash to
//! `require_kept_promise`, an auditor (or an automated monitoring system) can
//! verify after the fact that the on-chain execution matches the off-chain
//! promise. This is a defence-in-depth layer: the primary protection is the
//! domain-separated payload verification in [`domain::verify_payload`], but
//! this helper allows external verification without needing to replay the
//! signature verification logic.
///
//! # Usage
///
//! ```ignore
//! // Off-chain: compute hash of the intended action
//! let promised_hash = hash_delegated_action(&DelegatedActionPayload { ... });
//!
//! // On-chain: after execution, call the audit helper
//! audit::require_kept_promise(&env, &promised_hash, &actual_hash)?;
//! ```
///
//! The `actual_hash` should be computed from the actual parameters used in the
//! on-chain call (e.g. the `owner`, `target`, `delegation_type`, `expires_at`,
//! `domain`, `nonce`, `scheme`, `ledger_number`, `signature_domain` fields
//! that were actually passed to the entry point).

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, xdr::ToXdr, Bytes, Env, IntoVal};

/// Assert that a promised action hash matches the actual execution hash.
///
/// Returns `Ok(())` when the hashes match (promise kept), or
/// `Err(ContractError::PromiseNotKept)` when they differ (promise broken).
///
/// This is a pure function with no storage side effects — it can be called by
/// any contract or off-chain tool that has access to both hashes.
///
///  Arguments
///
///
/// * `e` - The Soroban environment (used only for error reporting).
/// * `promised_hash` - The hash of the off-chain promised action (computed off-chain).
/// * `actual_hash` - The hash of the on-chain executed action (computed on-chain).
///
///  Errors
///
///
/// Returns `ContractError::PromiseNotKept` (code 512) if the hashes do not match.
///
///  Example
///
///
/// ```ignore
/// // In an entry point after executing a delegated action:
/// let actual = compute_action_hash(&owner, &delegate, &delegation_type, &expires_at, &domain, nonce, scheme, ledger_number);
/// audit::require_kept_promise(&e, &promised_hash, &actual)?;
/// ```
///
///  Invariants
///
/// * The function is pure: no storage reads, no storage writes, no events.
/// * Comparison is byte-wise equality over the full `Bytes` value. Two hashes
///   of different lengths are never equal, and an empty hash only matches
///   another empty hash. This is deliberate: the audit helper must not
///   silently accept a malformed or truncated hash.
/// * On mismatch the function panics with a deterministic contract error
///   so the failure is observable and cannot be swallowed by a caller.
/// * The function never panics on matching hashes, even for empty or
///   over-sized inputs, so boundary inputs are safe.
pub fn require_kept_promise(
    e: &Env,
    promised_hash: &Bytes,
    actual_hash: &Bytes,
) -> Result<(), ContractError> {
    if promised_hash == actual_hash {
        Ok(())
    } else {
        panic_with_error!(e, ContractError::PromiseNotKept);
    }
}

/// Compute a hash of a delegated action for promise verification.
///
/// This function computes a deterministic hash from the action parameters.
/// Both the off-chain promiser and the on-chain verifier must use the same
/// hashing algorithm.
///
/// The hash includes all fields that define the action's intent:
/// - `owner`: the delegator's address
/// - `target`: the delegate/subject address
/// - `delegation_type`: Attestation or Management
/// - `expires_at`: the delegation expiry timestamp
/// - `domain`: the DomainTag (Delegate, RevokeDelegation, RevokeAttestation)
/// - `nonce`: the replay prevention nonce
/// - `scheme`: the signature scheme tag
/// - `ledger_number`: the ledger sequence at signing time
/// - `signature_domain`: the contract's signature domain string
///
/// # Invariants
///
/// * The field order is fixed and must not change without a compatibility
///   plan: changing the order would invalidate all previously committed
///   promise hashes.
/// * The function is pure and deterministic for identical inputs.
/// * Every field is included in the payload, so mutating any single field
///   produces a different hash (collision resistance depends on SHA-256).
///
///  Arguments
///
///
/// * `e` - The Soroban environment.
/// * `owner` - The address of the delegator.
/// * `target` - The address of the delegate (or subject for attestations).
/// * `delegation_type` - The type of delegation.
/// * `expires_at` - The delegation expiry timestamp.
/// * `domain` - The domain tag of the action.
/// * `nonce` - The nonce for replay prevention.
/// * `scheme` - The signature scheme tag.
/// * `ledger_number` - The ledger sequence number at signing time.
/// * `signature_domain` - The signature domain string (e.g. "CredenceDelegation").
///
///  Returns
///
///
/// A 32-byte hash (Bytes) that uniquely identifies the action.
#[allow(clippy::too_many_arguments)]
pub fn hash_delegated_action(
    e: &Env,
    owner: &soroban_sdk::Address,
    target: &soroban_sdk::Address,
    delegation_type: &crate::DelegationType,
    expires_at: &u64,
    domain: &crate::domain::DomainTag,
    nonce: &u64,
    scheme: &u32,
    ledger_number: &u32,
    signature_domain: &soroban_sdk::String,
) -> soroban_sdk::Bytes {
    // Build a structured payload for hashing
    let mut payload: soroban_sdk::Vec<soroban_sdk::Val> = soroban_sdk::Vec::new(e);

    // Serialize each field in a deterministic order.
    // DelegationType and DomainTag are not #[repr(u32)], so they are mapped
    // explicitly to stable u32 discriminants rather than cast with `as u32`.
    payload.push_back(owner.clone().into_val(e));
    payload.push_back(target.clone().into_val(e));
    payload.push_back(delegation_type_as_u32(delegation_type.clone()).into_val(e));
    payload.push_back((*expires_at).into_val(e));
    payload.push_back(domain_tag_as_u32(domain.clone()).into_val(e));
    payload.push_back((*nonce).into_val(e));
    payload.push_back((*scheme).into_val(e));
    payload.push_back((*ledger_number).into_val(e));
    payload.push_back(signature_domain.clone().into_val(e));

    // Hash the serialized payload using SHA-256
    let hash = e.crypto().sha256(&payload.to_val().to_xdr(e));
    Bytes::from_slice(e, &hash.to_array())
}

/// Stable u32 discriminant for [`crate::DelegationType`].
///
/// Documented so the hash encoding is stable and reproducible off-chain.
fn delegation_type_as_u32(dt: DelegationType) -> u32 {
    match dt {
        DelegationType::Attestation => 0,
        DelegationType::Management => 1,
    }
}

/// Stable u32 discriminant for [`crate::domain::DomainTag`].
///
/// Documented so the hash encoding is stable and reproducible off-chain.
fn domain_tag_as_u32(d: DomainTag) -> u32 {
    match d {
        DomainTag::Delegate => 0,
        DomainTag::RevokeDelegation => 1,
        DomainTag::RevokeAttestation => 2,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{domain::DomainTag, DelegationType};
    use credence_math;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    fn setup() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    // --------------------------------------------------------------------------
    // require_kept_promise: success / rejection / boundary / regression
    // --------------------------------------------------------------------------

    // Success: identical hashes return Ok.
    #[test]
    fn kept_promise_accepts_identical_hash() {
        let e = setup();
        let promised = Bytes::from_slice(&e, &[1u8; 32]);
        let actual = Bytes::from_slice(&e, &[1u8; 32]);

        assert!(require_kept_promise(&e, &promised, &actual).is_ok());
    }

    // Rejection: differing hashes panic with the deterministic error code.
    #[test]
    #[should_panic(expected = "Error(Contract, #512)")]
    fn kept_promise_rejects_single_bit_flip() {
        let e = setup();
        let mut original = [0u8; 32];
        original[31] = 0xAA;
        let promised = Bytes::from_slice(&e, &original);

        let mut flipped = original;
        flipped[31] ^= 0x01;
        let actual = Bytes::from_slice(&e, &flipped);

        require_kept_promise(&e, &promised, &actual).unwrap();
    }

    // Boundary: empty hashes are equal to each other and must not panic.
    #[test]
    fn kept_promise_accepts_empty_hashes() {
        let e = setup();
        let empty = Bytes::new(&e);
        let other_empty = Bytes::new(&e);

        assert!(require_kept_promise(&e, &empty, &other_empty).is_ok());
    }

    // Boundary: empty vs. non-empty must be rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #512)")]
    fn kept_promise_rejects_empty_vs_nonempty() {
        let e = setup();
        let empty = Bytes::new(&e);
        let nonempty = Bytes::from_slice(&e, &[7u8]);

        require_kept_promise(&e, &empty, &nonempty).unwrap();
    }

    // Boundary: different lengths must be rejected even if one is a prefix
    // of the other. This guards against truncation attacks.
    #[test]
    #[should_panic(expected = "Error(Contract, #512)")]
    fn kept_promise_rejects_length_mismatch() {
        let e = setup();
        let short = Bytes::from_slice(&e, &[1u8, 2u8, 3u8]);
        let long = Bytes::from_slice(&e, &[1u8, 2u8, 3u8, 4u8]);

        require_kept_promise(&e, &short, &long).unwrap();
    }

    // Boundary: over-sized hashes are compared byte-wise and match when
    // identical. This documents that the helper does not assume 32 bytes.
    #[test]
    fn kept_promise_accepts_oversized_match() {
        let e = setup();
        let big = Bytes::from_slice(&e, &[9u8; 128]);
        let big_copy = Bytes::from_slice(&e, &[9u8; 128]);

        assert!(require_kept_promise(&e, &big, &big_copy).is_ok());
    }

    // Regression: a single bit flip in an otherwise identical hash must
    // be detected. This is the core security property of the audit helper.
    #[test]
    #[should_panic(expected = "Error(Contract, #512)")]
    fn kept_promise_detects_single_bit_flip() {
        let e = setup();
        let mut original = [0u8; 32];
        original[15] = 0xAA;
        let promised = Bytes::from_slice(&e, &original);

        let mut flipped = original;
        flipped[15] ^= 0x01;
        let actual = Bytes::from_slice(&e, &flipped);

        require_kept_promise(&e, &promised, &actual).unwrap();
    }

    // Regression: the function must be pure and must not mutate its inputs.
    #[test]
    fn kept_promise_is_pure() {
        let e = setup();
        let promised = Bytes::from_slice(&e, &[3u8; 32]);
        let actual = Bytes::from_slice(&e, &[3u8; 32]);

        assert!(require_kept_promise(&e, &promised, &actual).is_ok());
        assert_eq!(promised, Bytes::from_slice(&e, &[3u8; 32]));
        assert_eq!(actual, Bytes::from_slice(&e, &[3u8; 32]));
    }

    // --------------------------------------------------------------------------
    // hash_delegated_action: determinism / boundary / field-sensitivity
    // --------------------------------------------------------------------------

    // Helper that builds a canonical action and returns its hash. Used by
    // multiple tests to keep the call sites consistent.
    #[allow(clippy::too_many_arguments)]
    fn hash_with(
        e: &Env,
        owner: &Address,
        target: &Address,
        delegation_type: &DelegationType,
        expires_at: u64,
        domain: &DomainTag,
        nonce: u64,
        scheme: u32,
        ledger_number: u32,
        signature_domain: &soroban_sdk::String,
    ) -> Bytes {
        hash_delegated_action(
            e,
            owner,
            target,
            delegation_type,
            &expires_at,
            domain,
            &nonce,
            &scheme,
            &ledger_number,
            signature_domain,
        )
    }

    #[test]
    fn hash_is_deterministic() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_with(
            &e,
            &owner,
            &target,
            &delegation_type,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        let hash2 = hash_with(
            &e,
            &owner,
            &target,
            &delegation_type,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_changes_per_field() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let domain = DomainTag::Delegate;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let base = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            1000,
            &domain,
            7,
            2,
            500,
            &signature_domain,
        );

        // 9 fields, each changed one at a time, all must differ from base.
        let owner2 = Address::generate(&e);
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner2,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1001,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                8,
                2,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                3,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                501,
                &signature_domain,
            ),
        );

        let target2 = Address::generate(&e);
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target2,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Management,
                1000,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &DomainTag::RevokeDelegation,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        let other_domain = soroban_sdk::String::from_str(&e, "OtherDomain");
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                500,
                &other_domain,
            ),
        );
    }

    // Regression: the hash is sensitive to every single field.
    #[test]
    fn hash_delegated_action_differs_for_every_field() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let domain = DomainTag::Delegate;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let base = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            1000,
            &domain,
            7,
            2,
            500,
            &signature_domain,
        );

        // owner changed
        let owner2 = Address::generate(&e);
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner2,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        // expires_at changed
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1001,
                &domain,
                7,
                2,
                500,
                &signature_domain,
            ),
        );

        // nonce changed
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                8,
                2,
                500,
                &signature_domain,
            ),
        );

        // scheme changed
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                3,
                500,
                &signature_domain,
            ),
        );

        // ledger_number changed
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                501,
                &signature_domain,
            ),
        );

        // signature_domain changed
        let other_domain = soroban_sdk::String::from_str(&e, "OtherDomain");
        assert_ne!(
            base,
            hash_with(
                &e,
                &owner,
                &target,
                &DelegationType::Attestation,
                1000,
                &domain,
                7,
                2,
                500,
                &other_domain,
            ),
        );
    }

    #[test]
    fn hash_delegated_action_differs_for_different_params() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_with(
            &e,
            &owner,
            &target,
            &delegation_type,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        // Different target should produce different hash
        let target2 = Address::generate(&e);
        let hash2 = hash_with(
            &e,
            &owner,
            &target2,
            &delegation_type,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_delegated_action_differs_for_different_domain() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::SECONDS_PER_DAY;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_with(
            &e,
            &owner,
            &target,
            &delegation_type,
            expires_at,
            &DomainTag::Delegate,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        let hash2 = hash_with(
            &e,
            &owner,
            &target,
            &delegation_type,
            expires_at,
            &DomainTag::RevokeDelegation,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_delegated_action_differs_for_different_delegation_type() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let expires_at = credence_math::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        let hash2 = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Management,
            expires_at,
            &domain,
            nonce,
            scheme,
            ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }

    // Boundary: expires_at at the minimum and maximum values must hash
    // deterministically and differently.
    #[test]
    fn hash_delegated_action_expires_at_boundaries() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let domain = DomainTag::Delegate;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash_min = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            0,
            &domain,
            0,
            0,
            0,
            &signature_domain,
        );

        let hash_max = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            u64::MAX,
            &domain,
            u64::MAX,
            u32::MAX,
            u32::MAX,
            &signature_domain,
        );

        assert_ne!(hash_min, hash_max);

        // Repeating the max boundary must be deterministic.
        let hash_max_again = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            u64::MAX,
            &domain,
            u64::MAX,
            u32::MAX,
            u32::MAX,
            &signature_domain,
        );
        assert_eq!(hash_max, hash_max_again);
    }

    // Boundary: an empty signature domain must still produce a deterministic
    // hash and must differ from a non-empty domain.
    #[test]
    fn hash_delegated_action_empty_signature_domain() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let domain = DomainTag::Delegate;
        let empty = soroban_sdk::String::from_str(&e, "");
        let non_empty = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash_empty = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            1,
            &domain,
            1,
            1,
            1,
            &empty,
        );

        let hash_non_empty = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            1,
            &domain,
            1,
            1,
            1,
            &non_empty,
        );

        assert_ne!(hash_empty, hash_non_empty);
    }

    // Regression: the promise hash computed from an actual action must match
    // the promised hash for the same action, and must be rejected for any
    // mutated field. This is the end-to-end audit property.
    #[test]
    fn audit_end_to_end_match_and_rejection() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let domain = DomainTag::Delegate;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let promised = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            12345,
            &domain,
            42,
            1,
            999,
            &signature_domain,
        );

        // Matching actual hash is accepted.
        let actual = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            12345,
            &domain,
            42,
            1,
            999,
            &signature_domain,
        );
        assert!(require_kept_promise(&e, &promised, &actual).is_ok());

        // Mutated actual hash differs from promised, so require_kept_promise
        // would reject it with the deterministic contract error.
        let mutated = hash_with(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            12345,
            &domain,
            43, // nonce changed
            1,
            999,
            &signature_domain,
        );
        assert_ne!(promised, mutated);
    }
}
