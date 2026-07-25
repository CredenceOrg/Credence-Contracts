//! Tests for `require_role_at_ledger` / `check_role_at_ledger` (issue #762).
//!
//! # Threat being mitigated
//!
//! Without a historical role check an attacker can:
//!
//! 1. Produce a signed payload *before* being granted admin rights.
//! 2. Wait until they are later promoted.
//! 3. Replay the old payload as if they had been authorised all along.
//!
//! `require_role_at_ledger` closes this gap by asserting that the actor's
//! `assigned_at` timestamp is ≤ the ledger timestamp embedded in the signed
//! action. If the role was granted *after* the action's ledger, the call is
//! rejected with `ContractError::RoleNotHeldAtLedger` (code 114).
//!
//! # Test structure
//!
//! - `role_held_at_exact_assignment_ledger_passes`    — happy path: assigned_at == at_ledger
//! - `role_held_before_action_ledger_passes`          — happy path: assigned_at < at_ledger
//! - `role_not_yet_granted_at_action_ledger_rejects`  — **negative**: assigned_at > at_ledger → 114
//! - `unknown_actor_rejects_with_not_admin`           — **negative**: unregistered actor → 100
//! - `insufficient_role_level_rejects_with_not_admin` — **negative**: role too low → 100

use crate::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = Address::generate(&env);
    AdminContract::initialize(env.clone(), super_admin.clone(), 1, 10);
    (env, contract_address, super_admin)
}

fn set_ts(env: &Env, ts: u64) {
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: ts,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 3_000_000,
    });
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

/// Role assigned at T=100, action timestamp T=100.
/// assigned_at == at_ledger → must pass.
#[test]
fn role_held_at_exact_assignment_ledger_passes() {
    let (env, _cid, super_admin) = setup();
    set_ts(&env, 100);
    let actor = Address::generate(&env);
    AdminContract::add_admin(env.clone(), super_admin.clone(), actor.clone(), AdminRole::Operator);

    // at_ledger == assigned_at(100) → OK
    AdminContract::check_role_at_ledger(env.clone(), AdminRole::Operator, actor, 100);
}

/// Role assigned at T=50, action timestamp T=100.
/// assigned_at < at_ledger → must pass.
#[test]
fn role_held_before_action_ledger_passes() {
    let (env, _cid, super_admin) = setup();
    set_ts(&env, 50);
    let actor = Address::generate(&env);
    AdminContract::add_admin(env.clone(), super_admin.clone(), actor.clone(), AdminRole::Operator);

    // at_ledger(100) > assigned_at(50) → OK
    AdminContract::check_role_at_ledger(env.clone(), AdminRole::Operator, actor, 100);
}

// ---------------------------------------------------------------------------
// Negative tests
// ---------------------------------------------------------------------------

/// Role assigned at T=200, action timestamp T=100.
/// assigned_at(200) > at_ledger(100) → RoleNotHeldAtLedger (code 114).
#[test]
#[should_panic(expected = "Error(Contract, #114)")]
fn role_not_yet_granted_at_action_ledger_rejects() {
    let (env, _cid, super_admin) = setup();
    set_ts(&env, 200);
    let actor = Address::generate(&env);
    AdminContract::add_admin(env.clone(), super_admin.clone(), actor.clone(), AdminRole::Operator);

    // Action claims to be from T=100, but role wasn't granted until T=200.
    AdminContract::check_role_at_ledger(env.clone(), AdminRole::Operator, actor, 100);
}

/// Actor not registered → NotAdmin (code 100).
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn unknown_actor_rejects_with_not_admin() {
    let (env, _cid, _super_admin) = setup();
    let stranger = Address::generate(&env);
    AdminContract::check_role_at_ledger(env.clone(), AdminRole::Operator, stranger, 0);
}

/// Actor has Operator role, check requires Admin → NotAdmin (code 100).
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn insufficient_role_level_rejects_with_not_admin() {
    let (env, _cid, super_admin) = setup();
    set_ts(&env, 10);
    let actor = Address::generate(&env);
    AdminContract::add_admin(env.clone(), super_admin.clone(), actor.clone(), AdminRole::Operator);

    // Actor is Operator, but Admin is required.
    AdminContract::check_role_at_ledger(env.clone(), AdminRole::Admin, actor, 10);
}
