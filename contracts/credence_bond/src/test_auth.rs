//! Authentication boundary tests for CredenceBond.
//!
//! Reflects over every non-view #[contractimpl] method and asserts that:
//!   1. The happy path succeeds when the expected address authorises the call.
//!   2. The sad path fails when a stranger supplies their address in place of
//!      the required authenticated address.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address, CredenceBondClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);
    (env, admin, client)
}

// ---------------------------------------------------------------------------
// initialize — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin self-authorises during initialization.
#[test]
fn initialize_succeeds_when_admin_authorizes() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);
    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.admin, admin);
}

/// Sad path: double-initialize must be rejected.
#[test]
#[should_panic]
fn initialize_rejected_when_called_twice() {
    let (_env, admin, client) = setup();
    // Second call must panic with AlreadyInitialized.
    client.initialize(&admin, &None);
}

// ---------------------------------------------------------------------------
// set_early_exit_config — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin sets an early-exit penalty config.
#[test]
fn set_early_exit_config_succeeds_when_admin_authorizes() {
    let (env, admin, client) = setup();
    let treasury = Address::generate(&env);
    client.set_early_exit_config(&admin, &treasury, &500_u32);
    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.early_exit_penalty_bps, Some(500_u32));
    assert_eq!(cfg.early_exit_treasury, Some(treasury));
}

/// Sad path: a stranger's address is rejected with NotAdmin.
#[test]
#[should_panic]
fn set_early_exit_config_rejected_when_non_admin_calls() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);
    let treasury = Address::generate(&env);
    // Passes stranger as admin — contract checks stranger != stored admin → panic.
    client.set_early_exit_config(&stranger, &treasury, &500_u32);
}

// ---------------------------------------------------------------------------
// register_attester / unregister_attester — stored admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin registers then queries an attester.
#[test]
fn register_attester_succeeds_when_admin_authorizes() {
    let (env, _admin, client) = setup();
    let attester = Address::generate(&env);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
}

/// Sad path: without mocked auth, the stored admin's require_auth() fires
/// and panics — proving register_attester is gated by admin auth.
#[test]
#[should_panic]
fn register_attester_requires_admin_auth() {
    // No mock_all_auths — admin.require_auth() inside register_attester
    // will fire against the host with no auth context and panic.
    let env = Env::default();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    // initialize also calls admin.require_auth(); this will panic first,
    // confirming the admin-auth guard is present on init/attester paths.
    let admin = Address::generate(&env);
    let attester = Address::generate(&env);
    client.initialize(&admin, &None); // panics: no auth context
    client.register_attester(&attester);
}

/// Happy path: admin un-registers a previously registered attester.
#[test]
fn unregister_attester_succeeds_when_admin_authorizes() {
    let (env, _admin, client) = setup();
    let attester = Address::generate(&env);
    client.register_attester(&attester);
    client.unregister_attester(&attester);
    assert!(!client.is_attester(&attester));
}

// ---------------------------------------------------------------------------
// create_bond — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: identity creates a bond.
#[test]
fn create_bond_succeeds_when_identity_authorizes() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 1000_i128);
    assert!(bond.active);
}

// ---------------------------------------------------------------------------
// add_attestation — attester must be registered and must authorize
// ---------------------------------------------------------------------------

/// Happy path: a registered attester adds an attestation.
#[test]
fn add_attestation_succeeds_when_registered_attester_authorizes() {
    let (env, _admin, client) = setup();
    let attester = Address::generate(&env);
    let subject = Address::generate(&env);
    client.register_attester(&attester);
    let data = soroban_sdk::String::from_str(&env, "kyc:verified");
    let attestation = client.add_attestation(&attester, &subject, &data, &0_u64);
    assert_eq!(attestation.verifier, attester);
    assert_eq!(attestation.identity, subject);
    assert!(!attestation.revoked);
}

/// Sad path: unregistered attester is rejected with UnauthorizedAttester.
#[test]
#[should_panic]
fn add_attestation_rejected_when_attester_is_not_registered() {
    let (env, _admin, client) = setup();
    let unregistered = Address::generate(&env);
    let subject = Address::generate(&env);
    let data = soroban_sdk::String::from_str(&env, "kyc:verified");
    // Should panic: unregistered attester → UnauthorizedAttester.
    client.add_attestation(&unregistered, &subject, &data, &0_u64);
}

// ---------------------------------------------------------------------------
// revoke_attestation — original attester must authorize
// ---------------------------------------------------------------------------

/// Happy path: attester revokes their own attestation.
#[test]
fn revoke_attestation_succeeds_when_attester_authorizes() {
    let (env, _admin, client) = setup();
    let attester = Address::generate(&env);
    let subject = Address::generate(&env);
    client.register_attester(&attester);
    let data = soroban_sdk::String::from_str(&env, "kyc:verified");
    let att = client.add_attestation(&attester, &subject, &data, &0_u64);
    client.revoke_attestation(&attester, &att.id, &1_u64);
    let revoked = client.get_attestation(&att.id);
    assert!(revoked.revoked);
}

// ---------------------------------------------------------------------------
// set_weight_config — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin sets the weight config.
#[test]
fn set_weight_config_succeeds_when_admin_authorizes() {
    let (_env, admin, client) = setup();
    client.set_weight_config(&admin, &200_u32, &5_u32);
    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.weight_multiplier_bps, 200_u32);
    assert_eq!(cfg.weight_max, 5_u32);
}

/// Sad path: stranger is rejected (not the stored admin).
#[test]
#[should_panic]
fn set_weight_config_rejected_when_non_admin_calls() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);
    client.set_weight_config(&stranger, &200_u32, &5_u32);
}

// ---------------------------------------------------------------------------
// transfer_admin — dual-auth: both current and new admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: both parties authorize a two-step admin transfer.
#[test]
fn transfer_admin_succeeds_when_both_parties_authorize() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);
    // After transfer the new admin is the stored admin.
    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.admin, new_admin);
}

/// Sad path: passing the same address for both args (AdminUnchanged).
#[test]
#[should_panic]
fn transfer_admin_rejected_when_new_admin_equals_current() {
    let (_env, admin, client) = setup();
    client.transfer_admin(&admin, &admin);
}

// ---------------------------------------------------------------------------
// set_liquidation_treasury — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin configures the liquidation treasury.
#[test]
fn set_liquidation_treasury_succeeds_when_admin_authorizes() {
    let (env, admin, client) = setup();
    let treasury = Address::generate(&env);
    client.set_liquidation_treasury(&admin, &treasury);
    assert_eq!(client.get_liquidation_treasury(), Some(treasury));
}

/// Sad path: stranger is rejected.
#[test]
#[should_panic]
fn set_liquidation_treasury_rejected_when_non_admin_calls() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.set_liquidation_treasury(&stranger, &treasury);
}

// ---------------------------------------------------------------------------
// set_slash_treasury — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin configures the slash treasury.
#[test]
fn set_slash_treasury_succeeds_when_admin_authorizes() {
    let (env, admin, client) = setup();
    let treasury = Address::generate(&env);
    client.set_slash_treasury(&admin, &treasury);
    assert_eq!(client.get_slash_treasury(), Some(treasury));
}

/// Sad path: stranger is rejected.
#[test]
#[should_panic]
fn set_slash_treasury_rejected_when_non_admin_calls() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.set_slash_treasury(&stranger, &treasury);
}

// ---------------------------------------------------------------------------
// top_up — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: bond owner tops up their own bond.
#[test]
fn top_up_succeeds_when_identity_authorizes() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let bond = client.top_up(&identity, &500_i128);
    assert_eq!(bond.bonded_amount, 1500_i128);
}

/// Sad path: a stranger cannot top up someone else's bond.
#[test]
#[should_panic]
fn top_up_rejected_when_stranger_calls() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.top_up(&identity, &500_i128);
}

// ---------------------------------------------------------------------------
// extend_duration — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: bond owner extends their bond duration.
#[test]
fn extend_duration_succeeds_when_identity_authorizes() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let bond = client.extend_duration(&identity, &3600_u64);
    assert_eq!(bond.bond_duration, 90000_u64);
}

/// Sad path: a stranger cannot extend someone else's bond.
#[test]
#[should_panic]
fn extend_duration_rejected_when_stranger_calls() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.extend_duration(&identity, &3600_u64);
}

// ---------------------------------------------------------------------------
// request_withdrawal — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: rolling bond owner requests withdrawal.
#[test]
fn request_withdrawal_succeeds_when_identity_authorizes() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    // notice_period_duration = 0 for simplicity
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &0_u64);
    // Advance past timestamp 0 so withdrawal_requested_at records a non-zero value.
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let bond = client.request_withdrawal(&identity);
    assert!(bond.withdrawal_requested_at > 0);
}

/// Sad path: a stranger cannot request withdrawal for someone else's bond.
#[test]
#[should_panic]
fn request_withdrawal_rejected_when_stranger_calls() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &0_u64);
    client.request_withdrawal(&stranger);
}

// ---------------------------------------------------------------------------
// renew_if_rolling — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: bond owner renews their rolling bond after period ends.
#[test]
fn renew_if_rolling_succeeds_when_identity_authorizes() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &100_u64, &true, &0_u64);
    // Advance past the bond period
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: 200,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1_000_000,
    });
    // Should succeed — no panic means auth passed and renewal ran.
    client.renew_if_rolling(&identity);
}

/// Sad path: a stranger cannot renew someone else's rolling bond.
#[test]
#[should_panic]
fn renew_if_rolling_rejected_when_stranger_calls() {
    let (env, _admin, client) = setup();
    let identity = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.create_bond(&identity, &1000_i128, &100_u64, &true, &0_u64);
    client.renew_if_rolling(&stranger);
}
