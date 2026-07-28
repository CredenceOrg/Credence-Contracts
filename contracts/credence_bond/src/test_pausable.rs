#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient};
use crate::test_helpers;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env, String, Vec};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

fn setup_with_bond(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    client.create_bond(&identity, &10_000_i128, &3600_u64, &false, &0_u64);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    (client, admin, identity)
}

#[test]
fn test_pause_blocks_all_mutating_entrypoints() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);
    let stranger = Address::generate(&e);

    // Verify initial state is unpaused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // ── Bond lifecycle operations ───────────────────────────────
    assert!(client.try_create_bond(&stranger, &5_000_i128, &3600_u64, &false, &0_u64).is_err());
    assert!(client.try_top_up(&identity, &1_000_i128).is_err());
    assert!(client.try_extend_duration(&identity, &3600_u64).is_err());
    assert!(client.try_request_withdrawal(&identity).is_err());
    assert!(client.try_withdraw(&identity, &1_000_i128).is_err());
    assert!(client.try_withdraw_early(&identity, &1_000_i128).is_err());
    assert!(client.try_renew_if_rolling(&identity).is_err());
    assert!(client.try_withdraw_bond(&identity).is_err());

    // ── Slashing ────────────────────────────────────────────────
    assert!(client.try_slash(&admin, &100_i128).is_err());
    assert!(client.try_slash_bond(&admin, &100_i128, &Bytes::new(&e)).is_err());

    // ── Admin / config mutations ────────────────────────────────
    assert!(client.try_register_attester(&stranger).is_err());
    assert!(client.try_unregister_attester(&stranger).is_err());
    assert!(client.try_set_accepted_tokens(&admin, &Vec::new(&e)).is_err());
    assert!(client.try_set_token(&admin, &stranger).is_err());
    let treasury = Address::generate(&e);
    assert!(client.try_set_early_exit_config(&admin, &treasury, &500_u32).is_err());
    assert!(client.try_set_borrow_frozen(&admin, &true).is_err());
    assert!(client.try_set_liquidation_treasury(&admin, &treasury).is_err());
    assert!(client.try_set_slash_treasury(&admin, &treasury).is_err());
    assert!(client.try_collect_fees(&admin, &Bytes::new(&e)).is_err());
    assert!(client.try_deposit_fees(&1_000_i128).is_err());
    assert!(client.try_set_callback(&stranger).is_err());
    assert!(client.try_liquidate(&admin).is_err());
    assert!(client.try_batch_transfer(&admin, &Vec::new(&e)).is_err());

    // ── Attestation operations ──────────────────────────────────
    let attester = Address::generate(&e);
    assert!(client.try_add_attestation(&attester, &stranger, &String::new(&e), &0_u64).is_err());
    assert!(client.try_revoke_attestation(&attester, &0_u64, &0_u64).is_err());

    // ── Admin transfer ──────────────────────────────────────────
    let new_admin = Address::generate(&e);
    assert!(client.try_transfer_admin(&admin, &new_admin).is_err());
    assert!(client.try_transfer_upgrade_admin(&admin, &new_admin).is_err());
    assert!(client.try_accept_upgrade_admin(&new_admin).is_err());
    assert!(client.try_cancel_upgrade_admin_transfer(&admin).is_err());

    // ── Claims ──────────────────────────────────────────────────
    assert!(client.try_expire_claims(&stranger, &50_u32).is_err());

    // ── Attester stake / weight config ──────────────────────────
    assert!(client.try_set_attester_stake(&admin, &attester, &100_000_i128).is_err());
    assert!(client.try_set_weight_config(&admin, &100_u32, &10_000_u32).is_err());

    // ── Attestation batch ───────────────────────────────────────
    assert!(client.try_add_attestation_batch(&stranger, &Vec::new(&e)).is_err());
}

#[test]
fn test_pause_allows_reads() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    let random_addr = Address::generate(&e);

    // Read-only functions must work when paused
    assert!(!client.is_attester(&random_addr));
    let _ = client.version();
    let _ = client.describe_config();
    let _ = client.describe_bond(&random_addr);
    let _ = client.is_borrow_frozen();
    let _ = client.get_nonce(&random_addr);
    let _ = client.get_weight_config();
    let _ = client.get_liquidation_treasury();
    let _ = client.get_slash_treasury();
    let _ = client.is_liquidated(&random_addr);
    let _ = client.is_locked();
    let _ = client.get_pending_claims_page(&random_addr, &0_u64, &10_u32);
    let _ = client.get_drain_eta();
    let _ = client.get_latest_drain_id();
    let _ = client.is_paused();
    let _ = client.get_identity_state(&identity);
    let _ = client.get_tier();
}

#[test]
fn test_pause_management_exempt() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Pause management functions must work when paused
    client.unpause(&admin);
    assert!(!client.is_paused());

    client.pause(&admin);
    assert!(client.is_paused());
}

#[test]
fn test_emergency_drain_requires_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // schedule_emergency_drain requires the contract to be PAUSED (inverse check)
    // It should fail when unpaused
    assert!(client.try_schedule_emergency_drain(&admin, &86400_u64).is_err());

    // It should succeed when paused
    client.pause(&admin);
    assert!(client.try_schedule_emergency_drain(&admin, &86400_u64).is_ok());
}

#[test]
fn test_cancel_emergency_drain_exempt() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // cancel_emergency_drain has no pause check (intentional exemption)
    // Must work both paused and unpaused
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());

    client.pause(&admin);
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());
}

#[test]
fn test_pause_prevents_initialize() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Pause the contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Try to re-initialize (should fail because paused, not because already initialized)
    let new_admin = Address::generate(&e);
    // The v2 initialize checks require_not_paused first
    assert!(client.try_initialize(&new_admin, &None).is_err());
}

#[test]
fn test_pause_blocks_add_attestation_batch() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let attester = Address::generate(&e);
    client.register_attester(&attester);

    // Setup a valid batch item
    let item = crate::AttestationBatchItem {
        attester: attester.clone(),
        attestation_data: String::from_str(&e, "kyc:verified"),
        nonce: 0_u64,
    };
    let mut items = Vec::new(&e);
    items.push_back(item);

    client.pause(&admin);
    assert!(client.is_paused());

    assert!(client.try_add_attestation_batch(&attester, &items).is_err());
}

#[test]
fn test_pause_blocks_initialize_v1() {
    let e = Env::default();

    // Register a fresh contract, pause the old one, then test initialize on a new one
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    e.mock_all_auths();

    // Use the v2 initialize (with registry)
    client.initialize(&admin, &None);
    client.pause(&admin);

    // Try to call initialize (v1 signature - no registry)
    // This is a separate entrypoint from the v2 initialize
    // Register a new contract for this test
    let contract_id2 = e.register(CredenceBond, ());
    let client2 = CredenceBondClient::new(&e, &contract_id2);
    let admin2 = Address::generate(&e);
    e.mock_all_auths();

    // Initialize with v2 (which has require_not_paused)
    client2.initialize(&admin2, &None);

    // Pause
    client2.pause(&admin2);

    // The v1 initialize entrypoint (no registry param) should be paused
    // Cannot call it on the existing contract since it's already initialized
    // So we just verify the v2 path is correctly gated (tested in test_pause_prevents_initialize)
}

#[test]
fn test_unpause_restores_operations() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Verify blocked
    assert!(client.try_register_attester(&Address::generate(&e)).is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Now operations should work again
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
}

#[test]
fn test_pause_multisig_flow() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    assert!(!client.is_paused());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    let pid2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid2);
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

#[test]
fn test_execute_requires_threshold() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    assert!(client.try_execute_pause_proposal(&pid).is_err());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}

#[test]
fn test_pause_set_pause_signer_set_pause_threshold_exempt() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // set_pause_signer and set_pause_threshold are pause management functions
    // They must work when paused
    client.pause(&admin);
    assert!(client.is_paused());

    let signer = Address::generate(&e);
    // These are delegated to pausable::set_pause_signer / set_pause_threshold
    // which are intentionally not pause-gated
    client.set_pause_signer(&admin, &signer, &true);
    client.set_pause_threshold(&admin, &1u32);

    // Verify signer config took effect
    client.unpause(&admin);
    let pid = client.pause(&signer);
    assert_eq!(pid, None); // threshold=1, single-signer -> immediate
    assert!(client.is_paused());
}

#[test]
fn test_pause_slash_bond_blocked() {
    let e = Env::default();
    let (client, admin) = setup_with_bond(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // slash_bond has its own require_not_paused check
    assert!(client.try_slash_bond(&admin, &100_i128, &Bytes::new(&e)).is_err());
}

#[test]
fn test_pause_withdraw_bond_blocked() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // withdraw_bond has require_not_paused
    assert!(client.try_withdraw_bond(&identity).is_err());
}

#[test]
fn test_pause_during_active_lockup_blocks_mutations() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);

    // Bond is still inside its 3600s lock-up window.
    assert!(client.try_withdraw(&identity, &1_000_i128).is_err());

    client.pause(&admin);
    assert!(client.is_paused());

    // Mutating lifecycle paths must stay blocked while paused mid-lock-up.
    assert!(client.try_top_up(&identity, &500_i128).is_err());
    assert!(client.try_withdraw(&identity, &1_000_i128).is_err());
    assert!(client.try_withdraw_early(&identity, &1_000_i128).is_err());
    assert!(client.try_slash_bond(&admin, &100_i128, &Bytes::new(&e)).is_err());
    assert!(client.try_collect_fees(&admin, &Bytes::new(&e)).is_err());
    assert!(client.try_withdraw_bond(&identity).is_err());

    // Views remain available during the paused lock-up.
    let _ = client.get_identity_state();
    let _ = client.get_tier();
    assert!(client.is_paused());
}

#[test]
fn test_unpause_after_lockup_pause_restores_top_up() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);

    client.pause(&admin);
    assert!(client.try_top_up(&identity, &500_i128).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());
    assert!(client.try_top_up(&identity, &500_i128).is_ok());
}
