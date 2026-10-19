#![cfg(test)]

//! Authentication boundary tests for CredenceArbitration.
//!
//! Verifies that every non-view #[contractimpl] function requires an
//! authenticated address and rejects unauthorised callers.  Each function
//! has a happy-path test (authorisation granted) and at least one sad-path
//! test (authorisation denied or wrong caller).
//!
//! The baseline tests use [`Env::mock_all_auths`] and therefore only exercise
//! the *role* checks (`NotAdmin` / `NotAuthorized` / `NotArbitrator`). The
//! adversarial section added below additionally drives the real
//! `Address::require_auth` gate with explicit [`MockAuth`] entries so that:
//!
//! * a missing signature is rejected by the host before any state is written,
//! * a signature from the wrong principal, or for the wrong entry point, never
//!   satisfies the gate,
//! * a rejected call leaves recorded state (weights, tallies, dispute status,
//!   admin) exactly as it was, and an authorised retry succeeds.

use super::*;
use interfaces::governable::GovernableClient;
use soroban_sdk::testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal, InvokeError, String};
use status::ArbitrationError;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    admin: Address,
    arb: Address,
    creator: Address,
    contract_id: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10_i128);
    Setup {
        env,
        admin,
        arb,
        creator,
        contract_id,
    }
}

fn open_dispute(env: &Env, contract_id: &Address, creator: &Address) -> u64 {
    let client = CredenceArbitrationClient::new(env, contract_id);
    let desc = String::from_str(env, "test dispute");
    client.create_dispute(creator, &desc, &3600_u64)
}

// ---------------------------------------------------------------------------
// register_arbitrator — stored admin must authorize
// ---------------------------------------------------------------------------

/// register_arbitrator fetches the stored admin and calls admin.require_auth().
/// Happy path: admin's auth is present (mock_all_auths), call succeeds.
#[test]
fn register_arbitrator_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    client.register_arbitrator(&new_arb, &5_i128);
    assert_eq!(client.get_arbitrator_weight(&new_arb), 5_u32);
}

/// Sad path: weight ≤ 0 is rejected before any state write, even when auth
/// is valid.  Guards the boundary that weight validation is enforced.
#[test]
fn register_arbitrator_rejected_when_weight_is_zero() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    let err = client
        .try_register_arbitrator(&new_arb, &0_i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

/// Sad path: negative weight is also rejected.
#[test]
fn register_arbitrator_rejected_when_weight_is_negative() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    let err = client
        .try_register_arbitrator(&new_arb, &-1_i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

// ---------------------------------------------------------------------------
// unregister_arbitrator — stored admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin removes a previously-registered arbitrator.
#[test]
fn unregister_arbitrator_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    client.unregister_arbitrator(&s.arb);
    // After removal the weight query should fail with NotArbitrator.
    let err = client
        .try_get_arbitrator_weight(&s.arb)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}

// ---------------------------------------------------------------------------
// create_dispute — creator must authorize
// ---------------------------------------------------------------------------

/// Happy path: creator's auth is present, dispute is opened in Voting status.
#[test]
fn create_dispute_succeeds_when_creator_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let desc = String::from_str(&s.env, "valid dispute");
    let id = client.create_dispute(&s.creator, &desc, &3600_u64);
    let d = client.get_dispute(&id);
    assert_eq!(d.creator, s.creator);
    assert_eq!(d.status, DisputeStatus::Voting);
}

// ---------------------------------------------------------------------------
// cancel_dispute — caller (creator or admin) must authorize
// ---------------------------------------------------------------------------

/// Happy path: creator cancels their own dispute.
#[test]
fn cancel_dispute_by_creator_succeeds() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.creator, &id, &None);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Cancelled);
}

/// Happy path: admin cancels any dispute.
#[test]
fn cancel_dispute_by_admin_succeeds() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.admin, &id, &None);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Cancelled);
}

/// Sad path: a stranger (neither creator nor admin) is rejected with NotAuthorized.
#[test]
fn cancel_dispute_rejected_when_stranger_calls() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_cancel_dispute(&stranger, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAuthorized);
}

// ---------------------------------------------------------------------------
// vote — voter must be a registered arbitrator and must authorize
// ---------------------------------------------------------------------------

/// Happy path: a registered arbitrator casts a vote and the tally increases.
#[test]
fn vote_succeeds_when_registered_arbitrator_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.vote(&s.arb, &id, &1_u32);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
}

/// Sad path: a stranger that was never registered as an arbitrator is rejected.
#[test]
fn vote_rejected_when_caller_is_not_registered_arbitrator() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_vote(&stranger, &id, &1_u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}

/// Sad path: outcome 0 is always invalid regardless of who calls.
#[test]
fn vote_rejected_when_outcome_is_zero() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let err = client.try_vote(&s.arb, &id, &0_u32).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidOutcome);
}

// ---------------------------------------------------------------------------
// set_quorum — stored admin must authorize and match
// ---------------------------------------------------------------------------

/// Happy path: admin configures a non-trivial quorum that is then readable.
#[test]
fn set_quorum_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    client.set_quorum(&s.admin, &50_i128, &2_u32);
    let (min_weight, min_voters) = client.get_quorum();
    assert_eq!(min_weight, 50_i128);
    assert_eq!(min_voters, 2_u32);
}

/// Sad path: a stranger (not the stored admin) is rejected with NotAdmin.
#[test]
fn set_quorum_rejected_when_non_admin_calls() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_set_quorum(&stranger, &50_i128, &2_u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAdmin);
}

// ---------------------------------------------------------------------------
// Adversarial authorization helpers
// ---------------------------------------------------------------------------

/// Assert a call was rejected by the host authorization layer *before* any
/// contract logic ran.
///
/// A rejection that surfaces as a contract error would mean `require_auth()`
/// executed after input validation or a state write, which is exactly the
/// regression this suite guards against. A successful value would mean the
/// gate was bypassed entirely.
fn assert_auth_rejected<T: core::fmt::Debug, E: core::fmt::Debug, E2: core::fmt::Debug>(
    res: Result<Result<T, E>, Result<E2, InvokeError>>,
    context: &str,
) {
    match res {
        Err(Err(_host_error)) => {}
        Err(Ok(contract_error)) => panic!(
            "{context}: expected host-level authorization rejection, got contract \
             error {contract_error:?}; require_auth() must run before contract logic"
        ),
        Ok(value) => panic!("{context}: expected rejection but call succeeded with {value:?}"),
    }
}

/// Authorise exactly one `(contract, fn_name, args)` invocation for `signer`,
/// replacing any previously mocked authorizations.
///
/// Unlike `mock_all_auths` this lets a test prove a call fails when the
/// required address has not signed. The `args` must equal the arguments of the
/// contract call whose `require_auth` is being satisfied — Soroban matches them
/// exactly.
fn authorize(
    env: &Env,
    contract_id: &Address,
    signer: &Address,
    fn_name: &str,
    args: soroban_sdk::Vec<soroban_sdk::Val>,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract: contract_id,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

fn advance(env: &Env, seconds: u64) {
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + seconds);
}

// ---------------------------------------------------------------------------
// A. Missing signature: rejected before any state write, retry recovers
// ---------------------------------------------------------------------------

/// register_arbitrator must be signed by the stored admin. Without that
/// signature the host rejects the call and the registry is untouched; an
/// authorised retry then records the weight exactly once.
#[test]
fn register_arbitrator_rejected_without_admin_auth_and_recovers_when_authorized() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_register_arbitrator(&new_arb, &5_i128),
        "register_arbitrator without admin auth",
    );
    assert_eq!(
        client
            .try_get_arbitrator_weight(&new_arb)
            .unwrap_err()
            .unwrap(),
        ArbitrationError::NotArbitrator,
        "rejected registration must not touch the registry"
    );

    s.env.mock_all_auths();
    client.register_arbitrator(&new_arb, &5_i128);
    assert_eq!(client.get_arbitrator_weight(&new_arb), 5_u32);
    let (page, next) = client.get_arbitrators_page(&0_u32, &10_u32);
    assert_eq!(
        page.len(),
        2_u32,
        "seeded arb plus the newly registered one"
    );
    assert!(next.is_none());
}

/// unregister_arbitrator must be signed by the stored admin. A rejected call
/// leaves the existing weight intact so a failed removal cannot silently strip
/// an arbitrator of their voting power.
#[test]
fn unregister_arbitrator_rejected_without_admin_auth_and_preserves_weight() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_unregister_arbitrator(&s.arb),
        "unregister_arbitrator without admin auth",
    );
    assert_eq!(client.get_arbitrator_weight(&s.arb), 10_u32);

    s.env.mock_all_auths();
    client.unregister_arbitrator(&s.arb);
    assert_eq!(
        client
            .try_get_arbitrator_weight(&s.arb)
            .unwrap_err()
            .unwrap(),
        ArbitrationError::NotArbitrator
    );
}

/// create_dispute must be signed by the creator. A rejected attempt must not
/// advance the id counter nor the one-active-dispute guard — otherwise a
/// spammer could burn ids or lock a victim out by replaying unsigned calls.
#[test]
fn create_dispute_rejected_without_creator_auth_and_does_not_consume_an_id() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let desc = String::from_str(&s.env, "unsigned attempt");

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_create_dispute(&s.creator, &desc, &3600_u64),
        "create_dispute without creator auth",
    );

    s.env.mock_all_auths();
    let id = client.create_dispute(&s.creator, &desc, &3600_u64);
    assert_eq!(id, 0_u64, "failed attempt must not consume the id counter");
    assert_eq!(client.get_dispute(&id).creator, s.creator);
}

/// cancel_dispute must be signed by the caller. A rejected cancellation leaves
/// the dispute in Voting so the stakeholder keeps their dispute data and can
/// still act once they sign.
#[test]
fn cancel_dispute_rejected_without_caller_auth_and_preserves_voting_status() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_cancel_dispute(&s.creator, &id, &None),
        "cancel_dispute without caller auth",
    );
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Voting);

    s.env.mock_all_auths();
    client.cancel_dispute(&s.creator, &id, &None);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Cancelled);
}

/// vote must be signed by the voter. A rejected vote must not mark the voter as
/// having voted nor move the tally, so a griefing unsigned call cannot consume
/// an arbitrator's single vote.
#[test]
fn vote_rejected_without_voter_auth_and_preserves_empty_tally() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_vote(&s.arb, &id, &1_u32),
        "vote without voter auth",
    );
    assert!(!client.has_voted(&id, &s.arb));
    assert_eq!(client.get_tally(&id, &1_u32), 0_i128);

    s.env.mock_all_auths();
    client.vote(&s.arb, &id, &1_u32);
    assert!(client.has_voted(&id, &s.arb));
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
}

/// set_quorum must be signed by the stored admin. A rejected update leaves the
/// previous quorum in force rather than partially applying new thresholds.
#[test]
fn set_quorum_rejected_without_admin_auth_and_preserves_defaults() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_set_quorum(&s.admin, &50_i128, &2_u32),
        "set_quorum without admin auth",
    );
    assert_eq!(client.get_quorum(), (0_i128, 0_u32));

    s.env.mock_all_auths();
    client.set_quorum(&s.admin, &50_i128, &2_u32);
    assert_eq!(client.get_quorum(), (50_i128, 2_u32));
}

/// archive_dispute must be signed by the stored admin. A rejected archive keeps
/// the dispute readable in its prior status.
#[test]
fn archive_dispute_rejected_without_admin_auth_and_preserves_dispute() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.creator, &id, &None);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_archive_dispute(&s.admin, &id),
        "archive_dispute without admin auth",
    );
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Cancelled);

    s.env.mock_all_auths();
    client.archive_dispute(&s.admin, &id);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Archived);
}

/// reopen_dispute must be signed by the stored admin. A rejected reopen leaves
/// the dispute archived instead of clearing its vote history.
#[test]
fn reopen_dispute_rejected_without_admin_auth_and_preserves_archived_status() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.creator, &id, &None);
    client.archive_dispute(&s.admin, &id);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_reopen_dispute(&s.admin, &id, &3600_u64),
        "reopen_dispute without admin auth",
    );
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Archived);

    s.env.mock_all_auths();
    client.reopen_dispute(&s.admin, &id, &3600_u64);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Voting);
}

/// transfer_admin must be signed by the current admin. A rejected transfer must
/// leave control fully with the existing admin (no half-applied transfer).
#[test]
fn transfer_admin_rejected_without_admin_auth_and_preserves_admin() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let gov = GovernableClient::new(&s.env, &s.contract_id);
    let replacement = Address::generate(&s.env);

    s.env.set_auths(&[]);
    // `transfer_admin` returns `()`, so the SDK reports a failed `require_auth`
    // as a contract error (`Error(Context, InvalidAction)`) rather than a host
    // `InvokeError`; either way the call must fail and leave the admin in place.
    assert!(
        client.try_transfer_admin(&replacement).is_err(),
        "transfer_admin without admin auth must be rejected"
    );
    assert_eq!(gov.get_admin(), s.admin);

    s.env.mock_all_auths();
    client.transfer_admin(&replacement);
    assert_eq!(gov.get_admin(), replacement);
}

// ---------------------------------------------------------------------------
// B. Wrong principal / privilege confinement
// ---------------------------------------------------------------------------

/// Signing as a third party must not satisfy an admin gate. This is stronger
/// than the role check: it proves the contract requires the *stored admin's*
/// signature, not merely a valid signature from anyone.
#[test]
fn register_arbitrator_rejected_when_stranger_signs_instead_of_admin() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let stranger = Address::generate(&s.env);
    let new_arb = Address::generate(&s.env);

    let args: soroban_sdk::Vec<soroban_sdk::Val> = (new_arb.clone(), 5_i128).into_val(&s.env);
    authorize(
        &s.env,
        &s.contract_id,
        &stranger,
        "register_arbitrator",
        args,
    );

    assert_auth_rejected(
        client.try_register_arbitrator(&new_arb, &5_i128),
        "stranger-signed register_arbitrator",
    );
    assert_eq!(
        client
            .try_get_arbitrator_weight(&new_arb)
            .unwrap_err()
            .unwrap(),
        ArbitrationError::NotArbitrator
    );
}

/// The admin holds no implicit voting power: signing `vote` as the admin must
/// not satisfy the arbitrator's `require_auth`.
#[test]
fn vote_rejected_when_admin_signs_instead_of_the_arbitrator() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    let args: soroban_sdk::Vec<soroban_sdk::Val> = (s.arb.clone(), id, 1_u32).into_val(&s.env);
    authorize(&s.env, &s.contract_id, &s.admin, "vote", args);

    assert_auth_rejected(
        client.try_vote(&s.arb, &id, &1_u32),
        "admin-signed vote on behalf of an arbitrator",
    );
    assert_eq!(client.get_tally(&id, &1_u32), 0_i128);
}

/// An authorization is scoped to a single entry point. Signing `set_quorum`
/// must not authorize `archive_dispute`, while the signed call still succeeds.
#[test]
fn authorization_for_one_entrypoint_does_not_authorize_another() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    let args: soroban_sdk::Vec<soroban_sdk::Val> =
        (s.admin.clone(), 50_i128, 2_u32).into_val(&s.env);
    authorize(&s.env, &s.contract_id, &s.admin, "set_quorum", args);

    assert_auth_rejected(
        client.try_archive_dispute(&s.admin, &id),
        "set_quorum authorization must not cover archive_dispute",
    );
    client.set_quorum(&s.admin, &50_i128, &2_u32);
    assert_eq!(client.get_quorum(), (50_i128, 2_u32));
}

// ---------------------------------------------------------------------------
// C. Duplicate / idempotent inputs must not double-count or drop data
// ---------------------------------------------------------------------------

/// A rejected vote followed by an authorised one must count the weight exactly
/// once, and a duplicate authorised vote must be rejected without re-counting.
#[test]
fn rejected_vote_then_retry_counts_weight_exactly_once() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    s.env.set_auths(&[]);
    assert_auth_rejected(
        client.try_vote(&s.arb, &id, &1_u32),
        "unsigned vote before retry",
    );

    s.env.mock_all_auths();
    client.vote(&s.arb, &id, &1_u32);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
    assert_eq!(
        client.try_vote(&s.arb, &id, &1_u32).unwrap_err().unwrap(),
        ArbitrationError::AlreadyVoted
    );
    assert_eq!(
        client.get_tally(&id, &1_u32),
        10_i128,
        "duplicate vote must not double-count"
    );
}

/// Re-registering an existing arbitrator updates the weight in place without
/// appending a second registry entry (which would corrupt pagination).
#[test]
fn re_registering_arbitrator_updates_weight_without_duplicating_registry_entry() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);

    client.register_arbitrator(&s.arb, &25_i128);
    assert_eq!(client.get_arbitrator_weight(&s.arb), 25_u32);

    let (page, next) = client.get_arbitrators_page(&0_u32, &10_u32);
    assert_eq!(page.len(), 1_u32, "update must not duplicate the registry");
    assert!(next.is_none());
    assert_eq!(page.get(0).unwrap(), s.arb);
}

/// Unregistering an address that was never registered is a harmless no-op and
/// must not disturb the weights of real arbitrators.
#[test]
fn unregistering_unknown_arbitrator_is_idempotent_and_preserves_others() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let unknown = Address::generate(&s.env);

    client.unregister_arbitrator(&unknown);

    assert_eq!(client.get_arbitrator_weight(&s.arb), 10_u32);
    let (page, _) = client.get_arbitrators_page(&0_u32, &10_u32);
    assert_eq!(page.len(), 1_u32);
}

/// A second initialize is rejected and must not clobber the original admin or
/// the arbitrator registry.
#[test]
fn re_initialize_is_rejected_and_preserves_existing_state() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let gov = GovernableClient::new(&s.env, &s.contract_id);
    let impostor_admin = Address::generate(&s.env);

    assert_eq!(
        client.try_initialize(&impostor_admin).unwrap_err().unwrap(),
        ArbitrationError::AlreadyInitialized
    );
    assert_eq!(gov.get_admin(), s.admin);
    assert_eq!(client.get_arbitrator_weight(&s.arb), 10_u32);
}

// ---------------------------------------------------------------------------
// D. Stale authority and stale state
// ---------------------------------------------------------------------------

/// After a transfer the previous admin is fully revoked: even presenting their
/// own signature, they cannot perform an admin-gated call. The new admin can.
#[test]
fn stale_admin_authorization_is_rejected_after_transfer() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_admin = Address::generate(&s.env);
    client.transfer_admin(&new_admin);

    let new_arb = Address::generate(&s.env);
    let args: soroban_sdk::Vec<soroban_sdk::Val> = (new_arb.clone(), 7_i128).into_val(&s.env);
    authorize(
        &s.env,
        &s.contract_id,
        &s.admin,
        "register_arbitrator",
        args,
    );

    assert_auth_rejected(
        client.try_register_arbitrator(&new_arb, &7_i128),
        "old-admin-signed register_arbitrator after transfer",
    );
    assert_eq!(
        client
            .try_get_arbitrator_weight(&new_arb)
            .unwrap_err()
            .unwrap(),
        ArbitrationError::NotArbitrator
    );

    s.env.mock_all_auths();
    client.register_arbitrator(&new_arb, &7_i128);
    assert_eq!(client.get_arbitrator_weight(&new_arb), 7_u32);
}

/// Unregistering an arbitrator removes their ability to cast *future* votes but
/// must not rewrite the tally their earlier vote already contributed.
#[test]
fn unregistered_arbitrator_cannot_vote_but_prior_tally_survives() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    client.vote(&s.arb, &id, &1_u32);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);

    client.unregister_arbitrator(&s.arb);
    assert_eq!(
        client.try_vote(&s.arb, &id, &2_u32).unwrap_err().unwrap(),
        ArbitrationError::NotArbitrator
    );
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
    assert_eq!(client.get_tally(&id, &2_u32), 0_i128);
}

// ---------------------------------------------------------------------------
// E. Boundary inputs
// ---------------------------------------------------------------------------

/// Outcomes are recorded independently across the valid range: distinct outcome
/// codes must never bleed into one another, and an unused outcome stays zero.
#[test]
fn vote_records_each_outcome_independently_across_boundaries() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let arb_low = Address::generate(&s.env);
    let arb_high = Address::generate(&s.env);
    client.register_arbitrator(&arb_low, &20_i128);
    client.register_arbitrator(&arb_high, &30_i128);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);

    client.vote(&s.arb, &id, &1_u32);
    client.vote(&arb_low, &id, &2_u32);
    client.vote(&arb_high, &id, &3_u32);

    assert_eq!(client.get_tally(&id, &0_u32), 0_i128);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
    assert_eq!(client.get_tally(&id, &2_u32), 20_i128);
    assert_eq!(client.get_tally(&id, &3_u32), 30_i128);
}

// ---------------------------------------------------------------------------
// F. Failure recovery: a rejected mutation leaves state consistent
// ---------------------------------------------------------------------------

/// An over-long cancellation reason is rejected, the dispute stays votable, and
/// a corrected retry succeeds — proving the failure left no partial write.
#[test]
fn overlong_cancel_reason_is_rejected_then_recovers_with_a_valid_retry() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let long_reason = "x".repeat(257);
    let reason = String::from_str(&s.env, &long_reason);

    assert_eq!(
        client
            .try_cancel_dispute(&s.creator, &id, &Some(reason))
            .unwrap_err()
            .unwrap(),
        ArbitrationError::ReasonTooLong
    );
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Voting);

    client.cancel_dispute(&s.creator, &id, &None);
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Cancelled);
}

/// A failed quorum resolution must preserve the votes and the creator's active
/// dispute slot: no tally is lost and the guard is not silently released.
#[test]
fn quorum_failure_keeps_votes_and_active_dispute_guard_intact() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    client.set_quorum(&s.admin, &100_i128, &1_u32);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.vote(&s.arb, &id, &1_u32);
    advance(&s.env, 3601);

    assert_eq!(
        client.try_resolve_dispute(&id).unwrap_err().unwrap(),
        ArbitrationError::QuorumNotMet
    );
    assert_eq!(client.get_dispute(&id).status, DisputeStatus::Voting);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);

    let other = String::from_str(&s.env, "second dispute");
    assert_eq!(
        client
            .try_create_dispute(&s.creator, &other, &3600_u64)
            .unwrap_err()
            .unwrap(),
        ArbitrationError::OngoingDispute
    );
}
