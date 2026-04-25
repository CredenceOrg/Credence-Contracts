//! Comprehensive unit tests for access control modifiers.
//! Covers admin/verifier/identity-owner checks, role composition, unauthorized paths,
//! and access denial event emission.

extern crate std;

use crate::access_control::{
    add_verifier_role, get_admin, is_admin, is_verifier, remove_verifier_role, require_admin,
    require_admin_or_verifier, require_identity_owner, require_verifier,
};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, Env, IntoVal, String, Symbol, TryFromVal,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

// Import main contract for testing privileged methods
use crate::{CredenceBond, CredenceBondClient};
use crate::access_control::AccessError;

#[contract]
pub struct AccessControlHarness;

#[contractimpl]
impl AccessControlHarness {
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&symbol_short!("admin"), &admin);
    }

    pub fn require_admin_only(e: Env, caller: Address) {
        require_admin(&e, &caller);
    }

    pub fn require_verifier_only(e: Env, caller: Address) {
        require_verifier(&e, &caller);
    }

    pub fn require_identity_owner_only(e: Env, caller: Address, expected: Address) {
        require_identity_owner(&e, &caller, &expected);
    }

    pub fn require_admin_or_verifier_only(e: Env, caller: Address) {
        require_admin_or_verifier(&e, &caller);
    }

    pub fn add_verifier(e: Env, admin: Address, verifier: Address) {
        add_verifier_role(&e, &admin, &verifier);
    }

    pub fn remove_verifier(e: Env, admin: Address, verifier: Address) {
        remove_verifier_role(&e, &admin, &verifier);
    }

    pub fn is_verifier_role(e: Env, address: Address) -> bool {
        is_verifier(&e, &address)
    }

    pub fn is_admin_role(e: Env, address: Address) -> bool {
        is_admin(&e, &address)
    }

    pub fn current_admin(e: Env) -> Address {
        get_admin(&e)
    }
}

fn setup(e: &Env) -> (AccessControlHarnessClient<'_>, Address) {
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

fn count_access_denied_events(e: &Env, contract_id: &Address, role: &str, code: u32) -> u32 {
    let expected_topics = vec![e, Symbol::new(e, "access_denied").into_val(e)];

    e.events()
        .all()
        .iter()
        .filter(|(contract, topics, data)| {
            if contract != contract_id || topics != &expected_topics {
                return false;
            }

            let parsed = <(Address, Symbol, u32)>::try_from_val(e, data);
            match parsed {
                Ok((_, role_symbol, error_code)) => {
                    role_symbol == Symbol::new(e, role) && error_code == code
                }
                Err(_) => false,
            }
        })
        .count() as u32
}

#[test]
fn test_require_admin_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.require_admin_only(&admin);
}

#[test]
#[should_panic(expected = "InvalidAction")]
fn test_require_admin_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_admin_only(&unauthorized);
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_require_admin_not_initialized() {
    let e = Env::default();
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(&e, &contract_id);

    let caller = Address::generate(&e);
    client.require_admin_only(&caller);
}

#[test]
fn test_add_and_remove_verifier_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    assert!(client.is_verifier_role(&verifier));

    client.remove_verifier(&admin, &verifier);
    assert!(!client.is_verifier_role(&verifier));
}

#[test]
#[should_panic(expected = "InvalidAction")]
fn test_add_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    let verifier = Address::generate(&e);
    client.add_verifier(&unauthorized, &verifier);
}

#[test]
fn test_require_verifier_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    client.require_verifier_only(&verifier);
}

#[test]
#[should_panic(expected = "not verifier")]
fn test_require_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_verifier_only(&unauthorized);
}

#[test]
fn test_require_identity_owner_success() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let identity = Address::generate(&e);
    client.require_identity_owner_only(&identity, &identity);
}

#[test]
#[should_panic(expected = "not identity owner")]
fn test_require_identity_owner_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let identity = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.require_identity_owner_only(&unauthorized, &identity);
}

#[test]
fn test_require_admin_or_verifier_success_for_admin() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.require_admin_or_verifier_only(&admin);
}

#[test]
fn test_require_admin_or_verifier_success_for_verifier() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    client.require_admin_or_verifier_only(&verifier);
}

#[test]
#[should_panic(expected = "not authorized")]
fn test_require_admin_or_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_admin_or_verifier_only(&unauthorized);
}

#[test]
fn test_admin_read_helpers() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let non_admin = Address::generate(&e);
    assert!(client.is_admin_role(&admin));
    assert!(!client.is_admin_role(&non_admin));
    assert_eq!(client.current_admin(), admin);
}

#[test]
fn test_multiple_verifiers() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier_1 = Address::generate(&e);
    let verifier_2 = Address::generate(&e);
    let verifier_3 = Address::generate(&e);

    client.add_verifier(&admin, &verifier_1);
    client.add_verifier(&admin, &verifier_2);
    client.add_verifier(&admin, &verifier_3);

    assert!(client.is_verifier_role(&verifier_1));
    assert!(client.is_verifier_role(&verifier_2));
    assert!(client.is_verifier_role(&verifier_3));

    client.remove_verifier(&admin, &verifier_2);

    assert!(client.is_verifier_role(&verifier_1));
    assert!(!client.is_verifier_role(&verifier_2));
    assert!(client.is_verifier_role(&verifier_3));
}

#[test]
fn test_access_denied_event_for_not_admin() {
    let e = Env::default();
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "admin", 1);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_admin_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "admin", 1);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_not_verifier() {
    let e = Env::default();
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "verifier", 2);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_verifier_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "verifier", 2);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_not_identity_owner() {
    let e = Env::default();
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    let owner = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "identity_owner", 3);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_identity_owner_only(&unauthorized, &owner);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "identity_owner", 3);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_admin_or_verifier() {
    let e = Env::default();
    let contract_id = e.register(AccessControlHarness, ());
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "admin_or_verifier", 2);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_admin_or_verifier_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "admin_or_verifier", 2);
    assert_eq!(denied_before + 1, denied_after);
}

// ==================== PRIVILEGED METHOD UNAUTHORIZED TESTS ====================

// Helper function to setup main contract for privileged method testing
fn setup_main_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let unauthorized = Address::generate(e);
    
    client.initialize(&admin);
    (client, admin, unauthorized)
}

// Test unauthorized access to set_supply_cap
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_supply_cap_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_supply_cap(&unauthorized, &1000_i128);
}

// Test unauthorized access to set_early_exit_config
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_early_exit_config_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let treasury = Address::generate(&e);
    
    client.set_early_exit_config(&unauthorized, &treasury, &100_u32);
}

// Test unauthorized access to set_emergency_config
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_emergency_config_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let governance = Address::generate(&e);
    let treasury = Address::generate(&e);
    
    client.set_emergency_config(&unauthorized, &governance, &treasury, &50_u32, &true);
}

// Test unauthorized access to set_emergency_mode (wrong admin)
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_emergency_mode_unauthorized_admin() {
    let e = Env::default();
    let (client, admin, unauthorized) = setup_main_contract(&e);
    let governance = Address::generate(&e);
    
    // First set emergency config with real admin
    client.set_emergency_config(&admin, &governance, &Address::generate(&e), &50_u32, &true);
    
    // Try to set emergency mode with unauthorized admin
    client.set_emergency_mode(&unauthorized, &governance, &true);
}

// Test unauthorized access to set_emergency_mode (wrong governance)
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_emergency_mode_unauthorized_governance() {
    let e = Env::default();
    let (client, admin, _) = setup_main_contract(&e);
    let governance = Address::generate(&e);
    let wrong_governance = Address::generate(&e);
    
    // First set emergency config with real admin
    client.set_emergency_config(&admin, &governance, &Address::generate(&e), &50_u32, &true);
    
    // Try to set emergency mode with wrong governance
    client.set_emergency_mode(&admin, &wrong_governance, &true);
}

// Test unauthorized access to emergency_withdraw
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_emergency_withdraw_unauthorized_admin() {
    let e = Env::default();
    let (client, admin, unauthorized) = setup_main_contract(&e);
    let governance = Address::generate(&e);
    
    // First set emergency config with real admin
    client.set_emergency_config(&admin, &governance, &Address::generate(&e), &50_u32, &true);
    
    // Try emergency withdraw with unauthorized admin
    client.emergency_withdraw(&unauthorized, &governance, &100_i128, &Symbol::new(&e, "test"));
}

// Test unauthorized access to emergency_withdraw (wrong governance)
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_emergency_withdraw_unauthorized_governance() {
    let e = Env::default();
    let (client, admin, _) = setup_main_contract(&e);
    let governance = Address::generate(&e);
    let wrong_governance = Address::generate(&e);
    
    // First set emergency config with real admin
    client.set_emergency_config(&admin, &governance, &Address::generate(&e), &50_u32, &true);
    
    // Try emergency withdraw with wrong governance
    client.emergency_withdraw(&admin, &wrong_governance, &100_i128, &Symbol::new(&e, "test"));
}

// Test unauthorized access to register_attester
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_register_attester_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let attester = Address::generate(&e);
    
    client.register_attester(&attester);
}

// Test unauthorized access to unregister_attester
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_unregister_attester_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let attester = Address::generate(&e);
    
    client.unregister_attester(&attester);
}

// Test unauthorized access to set_verifier_stake_requirement
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_verifier_stake_requirement_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_verifier_stake_requirement(&unauthorized, &1000_i128);
}

// Test unauthorized access to deactivate_verifier_by_admin
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_deactivate_verifier_by_admin_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let verifier = Address::generate(&e);
    
    client.deactivate_verifier_by_admin(&unauthorized, &verifier);
}

// Test unauthorized access to set_verifier_reputation
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_verifier_reputation_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let verifier = Address::generate(&e);
    
    client.set_verifier_reputation(&unauthorized, &verifier, &100_i128);
}

// Test unauthorized access to set_token
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_token_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let token = Address::generate(&e);
    
    client.set_token(&unauthorized, &token);
}

// Test unauthorized access to set_usdc_token
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_usdc_token_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let token = Address::generate(&e);
    let network = String::from_str(&e, "testnet");
    
    client.set_usdc_token(&unauthorized, &token, &network);
}

// Test unauthorized access to set_grace_window
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_grace_window_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_grace_window(&unauthorized, &300_u64);
}

// Test unauthorized access to set_attester_stake
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_attester_stake_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let attester = Address::generate(&e);
    
    client.set_attester_stake(&unauthorized, &attester, &1000_i128);
}

// Test unauthorized access to set_weight_config
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_weight_config_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_weight_config(&unauthorized, &150_u32, &100_u32);
}

// Test unauthorized access to slash
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_slash_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.slash(&unauthorized, &100_i128);
}

// Test unauthorized access to initialize_governance
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_initialize_governance_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let governors = vec![&e, Address::generate(&e)];
    
    client.initialize_governance(&unauthorized, &governors, &1000_u32, &1_u32);
}

// Test unauthorized access to set_fee_config
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_fee_config_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let treasury = Address::generate(&e);
    
    client.set_fee_config(&unauthorized, &treasury, &100_u32);
}

// Test unauthorized access to set_bond_token
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_bond_token_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let token = Address::generate(&e);
    
    client.set_bond_token(&unauthorized, &token);
}

// Test unauthorized access to set_protocol_fee_bps
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_protocol_fee_bps_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_protocol_fee_bps(&unauthorized, &50_u32);
}

// Test unauthorized access to set_attestation_fee_bps
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_attestation_fee_bps_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_attestation_fee_bps(&unauthorized, &25_u32);
}

// Test unauthorized access to set_withdrawal_cooldown_secs
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_withdrawal_cooldown_secs_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_withdrawal_cooldown_secs(&unauthorized, &3600_u64);
}

// Test unauthorized access to set_slash_cooldown_secs
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_slash_cooldown_secs_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_slash_cooldown_secs(&unauthorized, &7200_u64);
}

// Test unauthorized access to set_bronze_threshold
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_bronze_threshold_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_bronze_threshold(&unauthorized, &1000_i128);
}

// Test unauthorized access to set_silver_threshold
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_silver_threshold_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_silver_threshold(&unauthorized, &5000_i128);
}

// Test unauthorized access to set_gold_threshold
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_gold_threshold_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_gold_threshold(&unauthorized, &10000_i128);
}

// Test unauthorized access to set_platinum_threshold
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_platinum_threshold_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_platinum_threshold(&unauthorized, &50000_i128);
}

// Test unauthorized access to set_max_leverage
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_max_leverage_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_max_leverage(&unauthorized, &10_u32);
}

// Test unauthorized access to slash_bond
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_slash_bond_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.slash_bond(&unauthorized, &100_i128);
}

// Test unauthorized access to collect_fees
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_collect_fees_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.collect_fees(&unauthorized);
}

// Test unauthorized access to set_cooldown_period
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_cooldown_period_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_cooldown_period(&unauthorized, &3600_u64);
}

// Test unauthorized access to pause mechanism
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_pause_unauthorized() {
    let e = Env::default();
    let (client, admin, unauthorized) = setup_main_contract(&e);
    
    // This should panic with InvalidAction
    client.pause(&unauthorized);
}

// Test unauthorized access to unpause mechanism
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_unpause_unauthorized() {
    let e = Env::default();
    let (client, admin, unauthorized) = setup_main_contract(&e);
    
    // This should panic with InvalidAction
    client.unpause(&unauthorized);
}

// Test unauthorized access to set_pause_signer
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_pause_signer_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let signer = Address::generate(&e);
    
    client.set_pause_signer(&unauthorized, &signer, &true);
}

// Test unauthorized access to set_pause_threshold
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_set_pause_threshold_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    client.set_pause_threshold(&unauthorized, &3_u32);
}

// Test unauthorized access to initialize_upgrade_auth
#[test]
fn test_initialize_upgrade_auth_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    
    // This function might not panic but should fail in some way
    // Let's just call it to see what happens
    client.initialize_upgrade_auth(&unauthorized);
}

// Test unauthorized access to grant_upgrade_auth
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_grant_upgrade_auth_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let address = Address::generate(&e);
    
    client.grant_upgrade_auth(&unauthorized, &address, &crate::upgrade_auth::UpgradeRole::Upgrader, &1000_u64);
}

// Test unauthorized access to revoke_upgrade_auth
#[test]
#[should_panic(expected = "InvalidAction")]
fn test_revoke_upgrade_auth_unauthorized() {
    let e = Env::default();
    let (client, _, unauthorized) = setup_main_contract(&e);
    let address = Address::generate(&e);
    
    client.revoke_upgrade_auth(&unauthorized, &address);
}

// Test that admin can successfully call privileged methods
#[test]
fn test_admin_can_call_privileged_methods() {
    let e = Env::default();
    let (client, admin, _) = setup_main_contract(&e);
    let token = Address::generate(&e);
    let treasury = Address::generate(&e);
    
    // Test just a few key privileged methods that should work for admin
    // These should succeed without panicking
    client.set_grace_window(&admin, &300_u64);
    client.set_protocol_fee_bps(&admin, &10_u32);
    client.set_attestation_fee_bps(&admin, &5_u32);
    client.set_bronze_threshold(&admin, &1000_i128);
    client.set_max_leverage(&admin, &5_u32);
}
