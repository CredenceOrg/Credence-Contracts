//! Tests for configurable post-deadline grace window on signed orders.
//! Covers exact boundary, grace end, grace disabled, and replay protection.

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Env, String};

fn setup(
    e: &Env,
) -> (
    CredenceBondClient<'_>,
    soroban_sdk::Address,
    soroban_sdk::Address,
    soroban_sdk::Address,
) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    (client, admin, attester, contract_id)
}

// ── Default: strict enforcement (grace = 0) ───────────────────────────────────

#[test]
fn strict_mode_accepts_at_exact_deadline() {
    let e = Env::default();
    let (client, _admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // now == deadline — must be accepted (boundary inclusive)
    let now = e.ledger().timestamp();
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "exact"),
        &contract_id,
        &now,
        &0u64,
    );
}

#[test]
#[should_panic(expected = "signature expired")]
fn strict_mode_rejects_one_second_past_deadline() {
    let e = Env::default();
    let (client, _admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1001); // now = deadline + 1

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "late"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

// ── Grace window enabled ───────────────────────────────────────────────────────

#[test]
fn grace_window_accepts_within_grace() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // Set 30-second grace window
    client.set_grace_window(&admin, &30u64);
    assert_eq!(client.get_grace_window(), 30u64);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1020); // now = deadline + 20, inside grace

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "within_grace"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

#[test]
fn grace_window_accepts_at_exact_grace_end() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    client.set_grace_window(&admin, &30u64);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1030); // now == deadline + grace exactly

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "grace_end"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

#[test]
#[should_panic(expected = "signature expired")]
fn grace_window_rejects_one_second_past_grace_end() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    client.set_grace_window(&admin, &30u64);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1031); // now = deadline + grace + 1

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "past_grace"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

// ── Grace does NOT weaken replay protection ───────────────────────────────────

#[test]
#[should_panic(expected = "invalid nonce")]
fn nonce_reuse_rejected_within_grace_window() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    client.set_grace_window(&admin, &30u64);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1010); // inside grace

    // First use — accepted
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "first"),
        &contract_id,
        &deadline,
        &0u64,
    );

    // Replay with same nonce during grace — must be rejected
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "replay"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

// ── Setting grace to 0 restores strict mode ───────────────────────────────────

#[test]
#[should_panic(expected = "signature expired")]
fn setting_grace_to_zero_restores_strict_enforcement() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // Enable then disable grace
    client.set_grace_window(&admin, &60u64);
    client.set_grace_window(&admin, &0u64);
    assert_eq!(client.get_grace_window(), 0u64);

    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 1001); // now = deadline + 1, no grace

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "strict"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

// ── Grace on revoke_attestation ───────────────────────────────────────────────

#[test]
fn grace_window_applies_to_revoke() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // Add attestation with a future deadline
    let add_deadline = 5000u64;
    e.ledger().with_mut(|l| l.timestamp = 0);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "rev"),
        &contract_id,
        &add_deadline,
        &0u64,
    );

    // Set grace and advance into grace window of the revoke deadline
    client.set_grace_window(&admin, &30u64);
    let revoke_deadline = 2000u64;
    e.ledger().with_mut(|l| l.timestamp = 2015); // inside grace

    client.revoke_attestation(&attester, &att.id, &contract_id, &revoke_deadline, &1u64);
}

// ── Unauthorized grace window change ─────────────────────────────────────────

#[test]
#[should_panic]
fn non_admin_cannot_set_grace_window() {
    let e = Env::default();
    let (client, _admin, attester, _contract_id) = setup(&e);
    // attester is not admin — must panic
    client.set_grace_window(&attester, &30u64);
}
