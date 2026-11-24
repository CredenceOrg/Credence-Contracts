//! Boundary and recovery test coverage for [`crate::domain`].
//!
//! This module locks in the *deterministic* behaviour of the delegated-action
//! payload guards in `domain.rs` for valid, invalid, duplicate and
//! boundary-case inputs, and — critically — proves that **a rejected payload
//! never mutates state**, so a caller can recover by retrying.
//!
//! # Invariants under test
//!
//! 1. **Inclusive staleness window.** `check_payload_age` accepts
//!    `current - ledger_number == MAX_PAYLOAD_AGE_LEDGERS` (200) and rejects
//!    one ledger beyond it with `PayloadTooOld` (510).
//! 2. **Forward-only window.** A payload whose `ledger_number` is strictly
//!    ahead of the current sequence is rejected with `TimestampInFuture`
//!    (118). Without this guard `saturating_sub` would yield `0`, making a
//!    future-dated payload appear perpetually fresh.
//! 3. **Deterministic failure precedence.** `verify_payload` evaluates
//!    `domain → owner → target → contract_id`. A payload that is wrong in
//!    several fields must always surface the *first* failure, so error codes
//!    are stable for observability and alerting.
//! 4. **No state change on rejection (recovery).** All payload guards run
//!    *before* `nonce::consume_nonce` / `store_delegation`. A rejected payload
//!    therefore leaves both the nonce counter and the delegation record
//!    untouched, and a corrected payload reusing the same nonce succeeds.
//! 5. **Duplicate / racing inputs cannot corrupt state.** Replaying a payload
//!    (or racing two payloads over the same nonce) fails with `InvalidNonce`
//!    (208) and leaves exactly one delegation recorded.
//! 6. **Wire stability of scheme decoding.** `decode_scheme_safe` maps the
//!    known tags 0/1/2 to Ed25519/Secp256r1/MLDSA44 and falls back to Ed25519
//!    for any unknown tag (legacy payload compatibility). `verify_scheme_supported`
//!    rejects unknown tags with `UnknownScheme` (504).
//!
//! Every test below is deterministic: it drives the in-process Soroban `Env`
//! and only advances the ledger sequence explicitly.

#![cfg(test)]

use crate::{
    domain::{
        decode_scheme_safe, verify_scheme_supported, DelegatedActionPayload, DomainTag,
        MAX_PAYLOAD_AGE_LEDGERS,
    },
    verifier::SchemeTag,
    AttestationStatus, CredenceDelegation, CredenceDelegationClient, DelegationType,
};
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, contract_id)
}

/// Advance the ledger sequence (and, proportionally, the timestamp) by `n`.
/// Tests only ever reason about `ledger().sequence()` for the staleness guard,
/// but keeping the timestamp consistent avoids surprising expiry behaviour.
fn advance_sequence(e: &Env, n: u32) {
    e.ledger().with_mut(|info| {
        info.sequence_number = info.sequence_number.saturating_add(n);
        info.timestamp = info.timestamp.saturating_add(u64::from(n) * 5);
    });
}

/// A strictly-future expiry that always satisfies
/// `now < expires_at <= now + MAX_DELEGATION_DURATION`.
fn expiry(e: &Env) -> u64 {
    e.ledger().timestamp() + 86_400
}

/// Build a payload. `scheme = 0` (Ed25519) is verified implicitly by
/// `owner.require_auth()` at the call site, so tests do not need a registered
/// verifier contract.
fn make_payload(
    e: &Env,
    domain: DomainTag,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
    ledger_number: u32,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
        ledger_number,
        signature_domain: String::from_str(e, crate::domain::SIGNATURE_DOMAIN),
    }
}

/// A payload for `execute_delegated_delegate` at the current ledger.
fn delegate_payload(
    e: &Env,
    owner: &Address,
    delegate: &Address,
    contract_id: &Address,
    nonce: u64,
    ledger_number: u32,
) -> DelegatedActionPayload {
    make_payload(
        e,
        DomainTag::Delegate,
        owner,
        delegate,
        contract_id,
        nonce,
        ledger_number,
    )
}

// ===========================================================================
// 1. Constants and pure decoders
// ===========================================================================

/// `MAX_PAYLOAD_AGE_LEDGERS` is wire-visible behaviour: changing it silently
/// would change which signatures are accepted. Pin it.
#[test]
fn max_payload_age_ledgers_is_pinned_to_200() {
    assert_eq!(MAX_PAYLOAD_AGE_LEDGERS, 200);
}

/// The signature domain is bound into off-chain signing and must not drift.
#[test]
fn signature_domain_constant_is_credence_delegation() {
    assert_eq!(crate::domain::SIGNATURE_DOMAIN, "CredenceDelegation");
}

/// Each `DomainTag` must be distinguishable from the others; otherwise a
/// payload signed for one entry point could satisfy another's domain check.
#[test]
fn domain_tags_are_pairwise_distinct() {
    assert_ne!(DomainTag::Delegate, DomainTag::RevokeDelegation);
    assert_ne!(DomainTag::Delegate, DomainTag::RevokeAttestation);
    assert_ne!(DomainTag::RevokeDelegation, DomainTag::RevokeAttestation);
}

/// Known scheme tags decode to their corresponding signature scheme.
#[test]
fn decode_scheme_safe_maps_known_tags() {
    let e = Env::default();
    let owner = Address::generate(&e);
    let target = Address::generate(&e);
    let contract_id = Address::generate(&e);

    for (tag, expected) in [
        (0u32, SchemeTag::Ed25519),
        (1u32, SchemeTag::Secp256r1),
        (2u32, SchemeTag::MLDSA44),
    ] {
        let mut payload =
            make_payload(&e, DomainTag::Delegate, &owner, &target, &contract_id, 0, 0);
        payload.scheme = tag;
        assert_eq!(
            decode_scheme_safe(&payload),
            expected,
            "scheme tag {tag} must decode to {expected:?}"
        );
    }
}

/// Unknown scheme tags are treated as legacy Ed25519 payloads. This is the
/// documented backwards-compatibility contract: an unrecognised tag must never
/// cause a panic in the decoder itself.
#[test]
fn decode_scheme_safe_falls_back_to_ed25519_for_unknown_tags() {
    let e = Env::default();
    let owner = Address::generate(&e);
    let target = Address::generate(&e);
    let contract_id = Address::generate(&e);

    for tag in [3u32, 4, 255, u32::MAX] {
        let mut payload =
            make_payload(&e, DomainTag::Delegate, &owner, &target, &contract_id, 0, 0);
        payload.scheme = tag;
        assert_eq!(
            decode_scheme_safe(&payload),
            SchemeTag::Ed25519,
            "unknown scheme tag {tag} must fall back to Ed25519"
        );
    }
}

/// `verify_scheme_supported` accepts every known tag. This is the strict
/// counterpart to `decode_scheme_safe` and must not reject 0/1/2.
#[test]
fn verify_scheme_supported_accepts_known_tags() {
    let e = Env::default();
    verify_scheme_supported(&e, 0);
    verify_scheme_supported(&e, 1);
    verify_scheme_supported(&e, 2);
}

/// An out-of-range scheme tag is rejected with the wire-stable
/// `UnknownScheme` code (504) rather than silently downgraded.
#[test]
#[should_panic(expected = "Error(Contract, #504)")]
fn verify_scheme_supported_rejects_first_unknown_tag() {
    let e = Env::default();
    verify_scheme_supported(&e, 3);
}

/// Same boundary, at the extreme end of the `u32` range.
#[test]
#[should_panic(expected = "Error(Contract, #504)")]
fn verify_scheme_supported_rejects_u32_max() {
    let e = Env::default();
    verify_scheme_supported(&e, u32::MAX);
}

// ===========================================================================
// 2. Staleness boundary (inclusive at MAX, exclusive beyond)
// ===========================================================================

/// Age 0 is the normal path and must be accepted.
#[test]
fn fresh_payload_at_age_zero_is_accepted() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let payload = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &payload,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

/// The staleness window is inclusive: exactly `MAX_PAYLOAD_AGE_LEDGERS` old is
/// still valid.
#[test]
fn payload_at_exact_max_age_is_accepted() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS);

    let payload = delegate_payload(&e, &owner, &delegate, &contract_id, 0, signed_at);
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &payload,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

/// One ledger past the window is rejected with `PayloadTooOld`, and the
/// rejection must not consume the nonce: a corrected payload reusing the same
/// nonce still succeeds.
#[test]
fn payload_one_ledger_over_max_age_is_rejected_then_retry_succeeds() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let stale = delegate_payload(&e, &owner, &delegate, &contract_id, 0, signed_at);
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &stale,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::PayloadTooOld.into());
    // Recovery invariant: a stale payload burns no nonce slot.
    assert_eq!(client.get_nonce(&owner), 0);

    // Corrected payload, same nonce, fresh ledger_number: must now succeed.
    let retry = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &retry,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

// ===========================================================================
// 3. Forward-only boundary (future-dated payloads)
// ===========================================================================

/// A payload exactly one ledger in the future is impossible and must be
/// rejected as `TimestampInFuture`, with no nonce consumed.
#[test]
fn payload_one_ledger_in_future_is_rejected_then_retry_succeeds() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let future = e.ledger().sequence() + 1;
    let payload = delegate_payload(&e, &owner, &delegate, &contract_id, 0, future);

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::TimestampInFuture.into());
    assert_eq!(client.get_nonce(&owner), 0);

    let retry = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &retry,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

/// The extreme `u32::MAX` ledger number must be rejected, never wrapped or
/// treated as "fresh".
#[test]
fn payload_with_u32_max_ledger_number_is_rejected_as_future() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let payload = delegate_payload(&e, &owner, &delegate, &contract_id, 0, u32::MAX);
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::TimestampInFuture.into());
    assert_eq!(client.get_nonce(&owner), 0);
}

// ===========================================================================
// 4. Deterministic failure precedence in `verify_payload`
// ===========================================================================

/// Domain is checked first: wrong domain, wrong owner, wrong target and wrong
/// contract id must still surface `DomainMismatch`.
#[test]
fn precedence_domain_mismatch_wins_over_other_field_mismatches() {
    let (e, client, _) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let attacker = Address::generate(&e);
    let other_target = Address::generate(&e);
    let other_contract = Address::generate(&e);

    let payload = make_payload(
        &e,
        DomainTag::RevokeDelegation, // wrong domain
        &attacker,                   // wrong owner
        &other_target,               // wrong target
        &other_contract,             // wrong contract
        0,
        e.ledger().sequence(),
    );

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::DomainMismatch.into());
}

/// With the domain correct, the owner check takes precedence over target and
/// contract-id mismatches.
#[test]
fn precedence_owner_mismatch_wins_over_target_and_contract_mismatches() {
    let (e, client, _) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let attacker = Address::generate(&e);
    let other_target = Address::generate(&e);
    let other_contract = Address::generate(&e);

    let payload = make_payload(
        &e,
        DomainTag::Delegate,
        &attacker,
        &other_target,
        &other_contract,
        0,
        e.ledger().sequence(),
    );

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::OwnerMismatch.into());
}

/// With domain and owner correct, the target check takes precedence over the
/// contract-id check.
#[test]
fn precedence_target_mismatch_wins_over_contract_mismatch() {
    let (e, client, _) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let other_target = Address::generate(&e);
    let other_contract = Address::generate(&e);

    let payload = make_payload(
        &e,
        DomainTag::Delegate,
        &owner,
        &other_target,
        &other_contract,
        0,
        e.ledger().sequence(),
    );

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::TargetMismatch.into());
}

/// A payload bound to a different deployment is rejected with
/// `ContractIdMismatch` and leaves the nonce untouched, so the same nonce can
/// be retried against the correct contract.
#[test]
fn contract_id_mismatch_is_rejected_without_consuming_nonce_then_retry_succeeds() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let other_contract = Address::generate(&e);

    let wrong = delegate_payload(
        &e,
        &owner,
        &delegate,
        &other_contract,
        0,
        e.ledger().sequence(),
    );
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &wrong,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);

    let correct = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &correct,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

/// A domain-mismatched payload is rejected before nonce consumption; a
/// correctly-domained payload reusing the same nonce still succeeds.
#[test]
fn domain_mismatch_preserves_nonce_for_retry() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let wrong = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &wrong,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::DomainMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);

    let correct = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &correct,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

// ===========================================================================
// 5. Duplicate / racing inputs
// ===========================================================================

/// Replaying the exact same payload fails with `InvalidNonce` and leaves the
/// recorded delegation and the nonce counter unchanged; the next nonce is then
/// usable.
#[test]
fn replayed_payload_is_rejected_without_state_change_then_next_nonce_succeeds() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let payload = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &payload,
    );
    assert_eq!(client.get_nonce(&owner), 1);
    assert!(
        !client
            .get_delegation(&owner, &delegate, &DelegationType::Attestation)
            .revoked
    );

    // Replay of the identical payload.
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::InvalidNonce.into());
    // No state change from the rejected replay.
    assert_eq!(client.get_nonce(&owner), 1);

    // The next nonce remains usable.
    let next = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        1,
        e.ledger().sequence(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &next,
    );
    assert_eq!(client.get_nonce(&owner), 2);
}

/// Two submissions racing over the same nonce: exactly one wins. The loser
/// fails with `InvalidNonce` and creates no partial state.
#[test]
fn concurrent_nonce_race_allows_exactly_one_submission() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let payload = delegate_payload(
        &e,
        &owner,
        &delegate,
        &contract_id,
        0,
        e.ledger().sequence(),
    );

    // Winner: Attestation delegation.
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &payload,
    );
    assert_eq!(client.get_nonce(&owner), 1);

    // Loser: same signed payload, different call argument, same nonce.
    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Management,
            &expiry(&e),
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::InvalidNonce.into());

    // Exactly one delegation exists; the losing Management entry was never
    // written and the nonce did not advance.
    assert_eq!(client.get_nonce(&owner), 1);
    assert!(client
        .try_get_delegation(&owner, &delegate, &DelegationType::Management)
        .is_err());
    assert!(
        !client
            .get_delegation(&owner, &delegate, &DelegationType::Attestation)
            .revoked
    );
}

// ===========================================================================
// 6. Revoke-path recovery (state must survive a rejected payload)
// ===========================================================================

/// A stale revoke payload is rejected *before* the state transition, so the
/// delegation stays active; a corrected payload at the same nonce then revokes
/// it successfully.
#[test]
fn stale_revoke_is_rejected_and_delegation_remains_active_then_valid_revoke_succeeds() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Direct path consumes nonce 0 and creates the delegation.
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &0_u64,
    );
    assert_eq!(client.get_nonce(&owner), 1);

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let stale = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &contract_id,
        1,
        signed_at,
    );
    let err = client
        .try_execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &stale)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::PayloadTooOld.into());

    // Recovery invariants: nonce not burned, delegation not revoked.
    assert_eq!(client.get_nonce(&owner), 1);
    assert!(
        !client
            .get_delegation(&owner, &delegate, &DelegationType::Attestation)
            .revoked
    );

    // Retry with a fresh ledger_number at the same nonce succeeds.
    let valid = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &contract_id,
        1,
        e.ledger().sequence(),
    );
    client.execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &valid);
    assert_eq!(client.get_nonce(&owner), 2);
    assert!(
        client
            .get_delegation(&owner, &delegate, &DelegationType::Attestation)
            .revoked
    );
}

/// Same recovery guarantee for the attestation-revocation entry point.
#[test]
fn stale_revoke_attest_is_rejected_and_attestation_remains_active_then_valid_revoke_succeeds() {
    let (e, client, contract_id) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &expiry(&e),
        &0_u64,
    );
    assert_eq!(client.get_nonce(&attester), 1);
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Active
    ));

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let stale = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &contract_id,
        1,
        signed_at,
    );
    let err = client
        .try_execute_delegated_revoke_attest(&attester, &subject, &stale)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::PayloadTooOld.into());

    assert_eq!(client.get_nonce(&attester), 1);
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Active
    ));

    let valid = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &contract_id,
        1,
        e.ledger().sequence(),
    );
    client.execute_delegated_revoke_attest(&attester, &subject, &valid);
    assert_eq!(client.get_nonce(&attester), 2);
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Revoked
    ));
}

/// A future-dated revoke payload is likewise rejected before the transition,
/// leaving the delegation active for a legitimate retry.
#[test]
fn future_dated_revoke_is_rejected_and_delegation_remains_active() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry(&e),
        &0_u64,
    );

    let future = e.ledger().sequence() + 1;
    let payload = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &contract_id,
        1,
        future,
    );
    let err = client
        .try_execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::TimestampInFuture.into());
    assert_eq!(client.get_nonce(&owner), 1);
    assert!(
        !client
            .get_delegation(&owner, &delegate, &DelegationType::Attestation)
            .revoked
    );
}
