//! Audit helper to verify that an off-chain promise matches on-chain execution.
//!
//! # Threat being mitigated
//!
//! Without this check, an off-chain actor (e.g. a relayer, a frontend, or a
//! compromised signing key) could submit a delegated action payload that *claims*
//! to perform one action (e.g. `delegate` to address A) but actually executes a
//! different action on-chain (e.g. `delegate` to address B, or `revoke_delegation`).
//!
//! By hashing the intended action off-chain and passing that hash to
//! `require_kept_promise`, an auditor (or an automated monitoring system) can
//! verify after the fact that the on-chain execution matches the off-chain
//! promise. This is a defence-in-depth layer: the primary protection is the
//! domain-separated payload verification in [`domain::verify_payload`], but
//! this helper allows external verification without needing to replay the
//! signature verification logic.
//!
//! # Usage
//!
//! ```ignore
//! // Off-chain: compute hash of the intended action
//! let promised_hash = hash_delegated_action(&DelegatedActionPayload { ... });
//!
//! // On-chain: after execution, call the audit helper
//! audit::require_kept_promise(&env, &promised_hash, &actual_hash)?;
//! ```
//!
//! The `actual_hash` should be computed from the actual parameters used in the
//! on-chain call (e.g. the `owner`, `target`, `delegation_type`, `expires_at`,
//! `domain`, `nonce`, `scheme`, `ledger_number`, `signature_domain` fields
//! that were actually passed to the entry point).

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Bytes, Env, IntoVal, xdr::ToXdr};

/// Assert that a promised action hash matches the actual execution hash.
///
/// Returns `Ok(())` when the hashes match (promise kept), or
/// `Err(ContractError::PromiseNotKept)` when they differ (promise broken).
///
/// This is a pure function with no storage side effects — it can be called by
/// any contract or off-chain tool that has access to both hashes.
///
/// # Arguments
///
/// * `e` - The Soroban environment (used only for error reporting).
/// * `promised_hash` - The hash of the off-chain promised action (computed off-chain).
/// * `actual_hash` - The hash of the on-chain executed action (computed on-chain).
///
/// # Errors
///
/// Returns `ContractError::PromiseNotKept` (code 512) if the hashes do not match.
///
/// # Example
///
/// ```ignore
/// // In an entry point after executing a delegated action:
/// let actual = compute_action_hash(&owner, &delegate, &delegation_type, &expires_at, &domain, nonce, scheme, ledger_number);
/// audit::require_kept_promise(&e, &promised_hash, &actual)?;
/// ```
pub fn require_kept_promise(e: &Env, promised_hash: &Bytes, actual_hash: &Bytes) -> Result<(), ContractError> {
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
/// # Arguments
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
/// # Returns
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

    // Serialize each field in a deterministic order
    payload.push_back(owner.clone().into_val(e));
    payload.push_back(target.clone().into_val(e));
    payload.push_back((delegation_type.clone() as u32).into_val(e));
    payload.push_back((*expires_at).into_val(e));
    payload.push_back((domain.clone() as u32).into_val(e));
    payload.push_back((*nonce).into_val(e));
    payload.push_back((*scheme).into_val(e));
    payload.push_back((*ledger_number).into_val(e));
    payload.push_back(signature_domain.clone().into_val(e));

    // Hash the serialized payload using SHA-256
    let hash = e.crypto().sha256(&payload.to_val().to_xdr(e));
    Bytes::from_slice(e, &hash.to_array())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{DelegationType, domain::DomainTag};
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    fn setup() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    #[test]
    fn test_require_kept_promise_returns_ok_when_hashes_match() {
        let e = setup();
        let promised = Bytes::from_slice(&e, &[1u8; 32]);
        let actual = Bytes::from_slice(&e, &[1u8; 32]);

        assert!(require_kept_promise(&e, &promised, &actual).is_ok());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #512)")]
    fn test_require_kept_promise_panics_when_hashes_differ() {
        let e = setup();
        let promised = Bytes::from_slice(&e, &[1u8; 32]);
        let actual = Bytes::from_slice(&e, &[2u8; 32]);

        require_kept_promise(&e, &promised, &actual).unwrap();
    }

    #[test]
    fn test_hash_delegated_action_is_deterministic() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::Timestamp::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &delegation_type,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        let hash2 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &delegation_type,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_delegated_action_differs_for_different_params() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::Timestamp::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &delegation_type,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        // Different target should produce different hash
        let target2 = Address::generate(&e);
        let hash2 = hash_delegated_action(
            &e,
            &owner,
            &target2,
            &delegation_type,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_delegated_action_differs_for_different_domain() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let delegation_type = DelegationType::Attestation;
        let expires_at = credence_math::Timestamp::SECONDS_PER_DAY;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &delegation_type,
            &expires_at,
            &DomainTag::Delegate,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        let hash2 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &delegation_type,
            &expires_at,
            &DomainTag::RevokeDelegation,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_delegated_action_differs_for_different_delegation_type() {
        let e = setup();
        let owner = Address::generate(&e);
        let target = Address::generate(&e);
        let expires_at = credence_math::Timestamp::SECONDS_PER_DAY;
        let domain = DomainTag::Delegate;
        let nonce = 0_u64;
        let scheme = 0_u32;
        let ledger_number = 100_u32;
        let signature_domain = soroban_sdk::String::from_str(&e, "CredenceDelegation");

        let hash1 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &DelegationType::Attestation,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        let hash2 = hash_delegated_action(
            &e,
            &owner,
            &target,
            &DelegationType::Management,
            &expires_at,
            &domain,
            &nonce,
            &scheme,
            &ledger_number,
            &signature_domain,
        );

        assert_ne!(hash1, hash2);
    }
}