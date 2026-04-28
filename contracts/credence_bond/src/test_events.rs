use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use crate::events;

#[test]
fn test_emit_bond_created() {
    let e = Env::default();
    let identity = Address::generate(&e);

    events::emit_bond_created(&e, &identity, 1000i128, 86400u64, false);
}

#[test]
fn test_emit_bond_increased() {
    let e = Env::default();
    let identity = Address::generate(&e);

    events::emit_bond_increased(&e, &identity, 500i128, 1500i128);
}

#[test]
fn test_emit_bond_withdrawn() {
    let e = Env::default();
    let identity = Address::generate(&e);

    events::emit_bond_withdrawn(&e, &identity, 200i128, 800i128);
}

#[test]
fn test_emit_bond_slashed() {
    let e = Env::default();
    let identity = Address::generate(&e);

    events::emit_bond_slashed(&e, &identity, 100i128, 500i128);
}

#[test]
fn test_emit_claim_added() {
    let e = Env::default();
    let user = Address::generate(&e);

    let claim = crate::claims::PendingClaim {
        claim_id: 1,
        claim_type: crate::claims::ClaimType::VerifierReward,
        amount: 100,
        created_at: 1000,
        expires_at: 0,
        source_id: 1,
        metadata: Symbol::new(&e, ""),
        processed: false,
    };

    events::emit_claim_added(&e, &user, &claim);
}

#[test]
fn test_emit_claims_processed() {
    let e = Env::default();
    let user = Address::generate(&e);

    let result = crate::claims::ClaimResult {
        processed_count: 2,
        total_amount: 500,
        claim_types: soroban_sdk::Vec::new(&e),
    };
    let processed_claims = soroban_sdk::Vec::new(&e);

    events::emit_claims_processed(&e, &user, &result, &processed_claims);
}

#[test]
fn test_emit_claims_expired() {
    let e = Env::default();
    let user = Address::generate(&e);

    events::emit_claims_expired(&e, &user, 3u32, 300i128);
}

#[test]
fn test_emit_parameter_updated() {
    let e = Env::default();
    let admin = Address::generate(&e);

    events::emit_parameter_updated(
        &e,
        Symbol::new(&e, "test_param"),
        Symbol::new(&e, "risk"),
        &admin,
        100i128,
        200i128,
    );
}

#[test]
fn test_emit_admin_transfer_started() {
    let e = Env::default();
    let current = Address::generate(&e);
    let pending = Address::generate(&e);

    events::emit_admin_transfer_started(&e, &current, &pending);
}

#[test]
fn test_emit_admin_transfer_completed() {
    let e = Env::default();
    let old_admin = Address::generate(&e);
    let new_admin = Address::generate(&e);

    events::emit_admin_transfer_completed(&e, &old_admin, &new_admin);
}
