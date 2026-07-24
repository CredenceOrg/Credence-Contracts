#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn setup_with_contract_id() -> (Env, Address, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, contract_id, client)
}

#[test]
fn test_pause_blocks_state_changes_but_allows_reads() {
    let (env, admin, client) = setup();

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    // Read should still work
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // State changes should fail
    assert!(client
        .try_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &86400_u64,
            &0_u64
        )
        .is_err());

    assert!(client
        .try_revoke_attestation(&owner, &delegate, &0_u64)
        .is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    // State change works again
    let _ = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
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
fn test_pause_proposal_id_uniqueness_and_scoped_approval_lifecycle() {
    let (env, admin, contract_id, client) = setup_with_contract_id();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_signer(&admin, &s3, &true);
    client.set_pause_threshold(&admin, &2u32);

    let proposal_a = client.pause(&s1).unwrap();
    let proposal_b = client.unpause(&s2).unwrap();

    // IDs are derived from (action, epoch), so Pause and Unpause must differ.
    assert_ne!(proposal_a, proposal_b);

    client.approve_pause_proposal(&s2, &proposal_a);
    assert!(client.try_execute_pause_proposal(&proposal_b).is_err());

    client.approve_pause_proposal(&s3, &proposal_a);
    client.execute_pause_proposal(&proposal_a);
    assert!(client.is_paused());
    assert!(env.as_contract(&client.address, || {
        !env.storage()
            .instance()
            .has(&DataKey::PauseProposal(proposal_a))
    }));
    assert!(env.as_contract(&client.address, || {
        !env.storage()
            .instance()
            .has(&DataKey::PauseApprovalCount(proposal_a))
    }));

    client.approve_pause_proposal(&s1, &proposal_b);
    // proposal_b keeps its proposer approval while proposal_a is executed and
    // cleaned up, so one additional approval satisfies the 2-of-3 threshold.
    client.execute_pause_proposal(&proposal_b);
    assert!(!client.is_paused());
    assert!(env.as_contract(&client.address, || {
        !env.storage()
            .instance()
            .has(&DataKey::PauseProposal(proposal_b))
    }));
    assert!(env.as_contract(&client.address, || {
        !env.storage()
            .instance()
            .has(&DataKey::PauseApprovalCount(proposal_b))
    }));

    assert!(client.try_execute_pause_proposal(&proposal_a).is_err());
}

#[test]
fn test_execute_requires_threshold() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

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
fn test_delegate_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.pause(&admin);
    assert!(client
        .try_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &86400_u64,
            &0_u64
        )
        .is_err());
}

#[test]
fn test_revoke_delegation_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.pause(&admin);
    assert!(client
        .try_revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &0_u64)
        .is_err());
}

#[test]
fn test_revoke_attestation_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.pause(&admin);
    assert!(client
        .try_revoke_attestation(&owner, &delegate, &0_u64)
        .is_err());
}

#[test]
fn test_execute_delegated_delegate_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.pause(&admin);
    let payload = DelegatedActionPayload {
        nonce: 0,
        contract_id: client.address.clone(),
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        scheme: 0,
        ledger_number: 0,
    };
    assert!(client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &86400_u64,
            &payload
        )
        .is_err());
}

#[test]
fn test_execute_delegated_revoke_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.pause(&admin);
    let payload = DelegatedActionPayload {
        nonce: 0,
        contract_id: client.address.clone(),
        domain: DomainTag::RevokeDelegation,
        owner: owner.clone(),
        target: delegate.clone(),
        scheme: 0,
        ledger_number: 0,
    };
    assert!(client
        .try_execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload)
        .is_err());
}

#[test]
fn test_execute_delegated_revoke_attest_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.pause(&admin);
    let payload = DelegatedActionPayload {
        nonce: 0,
        contract_id: client.address.clone(),
        domain: DomainTag::RevokeAttestation,
        owner: owner.clone(),
        target: delegate.clone(),
        scheme: 0,
        ledger_number: 0,
    };
    assert!(client
        .try_execute_delegated_revoke_attest(&owner, &delegate, &payload)
        .is_err());
}

#[test]
fn test_invalidate_nonce_range_paused() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    client.pause(&admin);
    assert!(client.try_invalidate_nonce_range(&owner, &100_u64).is_err());
}

#[test]
fn test_admin_can_always_unpause() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    // threshold auto-adjusts to 1

    let pid = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    // Even though there are signers and threshold > 0, admin can bypass and unpause directly
    let res = client.unpause(&admin);
    assert!(res.is_none());
    assert!(!client.is_paused());
}

#[test]
fn test_threshold_invariants() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    // Initial threshold is 0

    client.set_pause_signer(&admin, &s1, &true);
    // Threshold should automatically be 1

    // Setting threshold to 0 when signers exist should fail
    let res = client.try_set_pause_threshold(&admin, &0);
    assert!(res.is_err());

    client.set_pause_signer(&admin, &s2, &true);

    client.set_pause_threshold(&admin, &2);

    // Removing signers lowers threshold
    client.set_pause_signer(&admin, &s2, &false);
    // threshold should now be 1

    client.set_pause_signer(&admin, &s1, &false);
    // threshold should now be 0, as there are no signers, which makes count 0
    // Actually the code does not auto-lower to 0 unless threshold > new_count.
    // If threshold was 1, new_count is 0, so threshold becomes 0.

    // We can verify this by checking if admin can pause directly without proposal
    let res = client.pause(&admin);
    assert!(res.is_none());
    assert!(client.is_paused());
}
