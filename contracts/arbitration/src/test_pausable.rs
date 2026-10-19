#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};

fn setup() -> (Env, Address, CredenceArbitrationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn advance(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

#[test]
fn test_pause_blocks_state_changes_but_allows_reads() {
    let (env, admin, client) = setup();

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    // Read should still work
    let _ = client.get_tally(&0u64, &1u32);

    // State change should fail
    let arbitrator = Address::generate(&env);
    assert!(client
        .try_register_arbitrator(&arbitrator, &10_i128)
        .is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    // State change works again
    client.register_arbitrator(&arbitrator, &10_i128);
}

#[test]
fn test_pause_multisig_flow() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

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
fn test_pause_does_not_block_existing_tests_flow_when_unpaused() {
    let (env, _admin, client) = setup();

    let arb = Address::generate(&env);
    client.register_arbitrator(&arb, &10_i128);

    let creator = Address::generate(&env);
    let description = String::from_str(&env, "Dispute");
    let dispute_id = client.create_dispute(&creator, &description, &3600u64);
    let _ = client.get_dispute(&dispute_id);
    let _ = advance; // suppress unused warning
}

#[test]
fn test_pause_boundary_unauthorized_signer() {
    let (env, admin, client) = setup();
    let not_admin = Address::generate(&env);
    let signer1 = Address::generate(&env);

    // Non-admin trying to set pause signer
    assert!(client.try_set_pause_signer(&not_admin, &signer1, &true).is_err());

    // Admin adds signer1
    client.set_pause_signer(&admin, &signer1, &true);
    
    // Setting pause threshold by non-admin
    assert!(client.try_set_pause_threshold(&not_admin, &1u32).is_err());

    // Admin sets threshold
    client.set_pause_threshold(&admin, &1u32);

    // Non-signer trying to propose pause
    assert!(client.try_pause(&not_admin).is_err());

    // Signer proposes
    let pid = client.pause(&signer1).unwrap();

    // Non-signer trying to approve
    assert!(client.try_approve_pause_proposal(&not_admin, &pid).is_err());
}

#[test]
fn test_pause_boundary_signer_management() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    
    // Duplicate addition
    client.set_pause_signer(&admin, &s1, &true);

    let s2 = Address::generate(&env);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    // Threshold exceeding signers should fail
    assert!(client.try_set_pause_threshold(&admin, &3u32).is_err());

    // Removing a signer when threshold is 2 should adjust threshold to 1
    client.set_pause_signer(&admin, &s2, &false);

    // We can test this by proposing and seeing if 1 approval is enough
    let pid = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}

#[test]
fn test_pause_recovery_and_failures() {
    let (env, admin, client) = setup();
    
    // threshold == 0, pause by non-admin should fail
    let not_admin = Address::generate(&env);
    assert!(client.try_pause(&not_admin).is_err());

    // Pause by admin should succeed
    client.pause(&admin);
    assert!(client.is_paused());

    // Pausing when already paused should fail
    assert!(client.try_pause(&admin).is_err());

    // Unpausing when paused by non-admin should fail
    assert!(client.try_unpause(&not_admin).is_err());

    // Unpausing by admin should succeed
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Unpausing when not paused should fail
    assert!(client.try_unpause(&admin).is_err());
}

#[test]
fn test_pause_multisig_boundary_duplicate_and_insufficient() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_signer(&admin, &s3, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    // S1 has already approved implicitly during proposal.
    // Duplicate approval by S1 shouldn't increase count.
    client.approve_pause_proposal(&s1, &pid);
    
    // Attempt to execute with insufficient approvals (only S1)
    assert!(client.try_execute_pause_proposal(&pid).is_err());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);

    assert!(client.is_paused());

    // Execute same proposal again should fail (proposal removed)
    assert!(client.try_execute_pause_proposal(&pid).is_err());
    
    // Approve non-existent proposal
    assert!(client.try_approve_pause_proposal(&s1, &999u64).is_err());
}
