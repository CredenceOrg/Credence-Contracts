use crate::{ActionType, CredenceMultiSig, CredenceMultiSigClient, ProposalStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup(e: &Env) -> (CredenceMultiSigClient, Address, Vec<Address>) {
    let contract_id = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(e, &contract_id);

    let admin = Address::generate(e);
    let signer1 = Address::generate(e);
    let signer2 = Address::generate(e);
    let signer3 = Address::generate(e);

    let mut signers = Vec::new(e);
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());

    e.mock_all_auths();

    (client, admin, signers)
}

// ==================== Initialization Tests ====================

#[test]
fn test_initialize() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);

    client.initialize(&admin, &signers, &2);

    assert_eq!(client.get_signer_count(), 3);
    assert_eq!(client.get_threshold(), 2);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.is_signer(&signers.get(0).unwrap()), true);
    assert_eq!(client.is_signer(&signers.get(1).unwrap()), true);
    assert_eq!(client.is_signer(&signers.get(2).unwrap()), true);
}

#[test]
#[should_panic(expected = "signers list cannot be empty")]
fn test_initialize_empty_signers() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let signers = Vec::new(&e);

    client.initialize(&admin, &signers, &1);
}

#[test]
#[should_panic(expected = "invalid threshold")]
fn test_initialize_threshold_zero() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);

    client.initialize(&admin, &signers, &0);
}

#[test]
#[should_panic(expected = "invalid threshold")]
fn test_initialize_threshold_exceeds_signers() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);

    client.initialize(&admin, &signers, &4);
}

// ==================== Signer Management Tests ====================

#[test]
fn test_add_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let new_signer = Address::generate(&e);
    client.add_signer(&admin, &new_signer);

    assert_eq!(client.get_signer_count(), 4);
    assert_eq!(client.is_signer(&new_signer), true);
}

#[test]
#[should_panic(expected = "signer already exists")]
fn test_add_duplicate_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.add_signer(&admin, &signers.get(0).unwrap());
}

#[test]
fn test_remove_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let signer_to_remove = signers.get(2).unwrap();
    client.remove_signer(&admin, &signer_to_remove);

    assert_eq!(client.get_signer_count(), 2);
    assert_eq!(client.is_signer(&signer_to_remove), false);
}

#[test]
#[should_panic(expected = "signer does not exist")]
fn test_remove_nonexistent_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let fake_signer = Address::generate(&e);
    client.remove_signer(&admin, &fake_signer);
}

#[test]
#[should_panic(expected = "cannot remove last signer")]
fn test_remove_last_signer() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);

    let mut single_signer = Vec::new(&e);
    single_signer.push_back(Address::generate(&e));

    client.initialize(&admin, &single_signer, &1);
    client.remove_signer(&admin, &single_signer.get(0).unwrap());
}

#[test]
fn test_remove_signer_auto_adjusts_threshold() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &3); // threshold = 3

    client.remove_signer(&admin, &signers.get(2).unwrap());

    assert_eq!(client.get_signer_count(), 2);
    assert_eq!(client.get_threshold(), 2); // auto-adjusted from 3 to 2
}

// ==================== Threshold Tests ====================

#[test]
fn test_set_threshold() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.set_threshold(&admin, &3);
    assert_eq!(client.get_threshold(), 3);
}

#[test]
#[should_panic(expected = "invalid threshold")]
fn test_set_threshold_zero() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.set_threshold(&admin, &0);
}

#[test]
#[should_panic(expected = "invalid threshold")]
fn test_set_threshold_exceeds_signers() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.set_threshold(&admin, &4);
}

// ==================== Proposal Submission Tests ====================

#[test]
fn test_submit_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let description = String::from_str(&e, "Test proposal");

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &description,
        &0_u64,
        &None,
    );

    assert_eq!(proposal_id, 0);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.id, 0);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.description, description);
}

#[test]
#[should_panic(expected = "not a signer")]
fn test_submit_proposal_non_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let non_signer = Address::generate(&e);
    let description = String::from_str(&e, "Test proposal");

    client.submit_proposal(
        &non_signer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &description,
        &0_u64,
        &None,
    );
}

#[test]
#[should_panic(expected = "description cannot be empty")]
fn test_submit_proposal_empty_description() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let description = String::from_str(&e, "");

    client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &description,
        &0_u64,
        &None,
    );
}

#[test]
fn test_submit_multiple_proposals() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let id1 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal 1"),
        &0_u64,
        &None,
    );

    let id2 = client.submit_proposal(
        &proposer,
        &ActionType::Transfer,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal 2"),
        &0_u64,
        &None,
    );

    assert_eq!(id1, 0);
    assert_eq!(id2, 1);
}

// ==================== Signing Tests ====================

#[test]
fn test_sign_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let signer = signers.get(1).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signer, &proposal_id);

    assert_eq!(client.get_signature_count(&proposal_id), 1);
    assert_eq!(client.has_signed(&proposal_id, &signer), true);
}

#[test]
#[should_panic(expected = "not a signer")]
fn test_sign_proposal_non_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let non_signer = Address::generate(&e);

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&non_signer, &proposal_id);
}

#[test]
#[should_panic(expected = "proposal not found")]
fn test_sign_nonexistent_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let signer = signers.get(0).unwrap();
    client.sign_proposal(&signer, &999_u64);
}

#[test]
#[should_panic(expected = "already signed")]
fn test_double_sign() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let signer = signers.get(1).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signer, &proposal_id);
    client.sign_proposal(&signer, &proposal_id); // double sign
}

#[test]
fn test_multiple_signers_sign() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    assert_eq!(client.get_signature_count(&proposal_id), 2);
}

// ==================== Execution Tests ====================

#[test]
fn test_execute_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
#[should_panic(expected = "insufficient signatures to execute")]
fn test_execute_proposal_insufficient_signatures() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id); // only 1 signature, threshold is 2
}

#[test]
#[should_panic(expected = "proposal not found")]
fn test_execute_nonexistent_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.execute_proposal(&999_u64);
}

#[test]
#[should_panic(expected = "proposal is not pending")]
fn test_execute_already_executed() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);
    client.execute_proposal(&proposal_id); // execute again
}

#[test]
fn test_execute_with_exact_threshold() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &3); // threshold = 3

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(2).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

// ==================== Rejection Tests ====================

#[test]
fn test_reject_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.reject_proposal(&admin, &proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

#[test]
#[should_panic(expected = "proposal is not pending")]
fn test_reject_already_rejected() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.reject_proposal(&admin, &proposal_id);
    client.reject_proposal(&admin, &proposal_id); // reject again
}

#[test]
#[should_panic(expected = "proposal is not pending")]
fn test_sign_rejected_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
    );

    client.reject_proposal(&admin, &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
}

// ==================== Expiration Tests ====================

#[test]
#[should_panic(expected = "proposal has expired")]
fn test_sign_expired_proposal() {
    let e = Env::default();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &1500_u64, // expires at 1500
        &None,
    );

    e.ledger().with_mut(|li| {
        li.timestamp = 1600; // move past expiration
    });

    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
}

#[test]
#[should_panic(expected = "proposal has expired")]
fn test_execute_expired_proposal() {
    let e = Env::default();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &1500_u64, // expires at 1500
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    e.ledger().with_mut(|li| {
        li.timestamp = 1600; // move past expiration
    });

    client.execute_proposal(&proposal_id);
}

// ==================== Threshold Scenarios ====================

#[test]
fn test_threshold_1_of_1() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let signer = Address::generate(&e);

    let mut signers = Vec::new(&e);
    signers.push_back(signer.clone());

    client.initialize(&admin, &signers, &1);

    let proposal_id = client.submit_proposal(
        &signer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signer, &proposal_id);
    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_threshold_3_of_3() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &3);

    let proposer = signers.get(0).unwrap();

    let proposal_id = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(2).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_threshold_2_of_5() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let mut signers = Vec::new(&e);
    for _ in 0..5 {
        signers.push_back(Address::generate(&e));
    }

    client.initialize(&admin, &signers, &2);

    let proposal_id = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

// ==================== Query Tests ====================

#[test]
fn test_get_signers() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let retrieved_signers = client.get_signers();
    assert_eq!(retrieved_signers.len(), 3);
    assert_eq!(retrieved_signers.get(0).unwrap(), signers.get(0).unwrap());
    assert_eq!(retrieved_signers.get(1).unwrap(), signers.get(1).unwrap());
    assert_eq!(retrieved_signers.get(2).unwrap(), signers.get(2).unwrap());
}

#[test]
fn test_is_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let non_signer = Address::generate(&e);

    assert_eq!(client.is_signer(&signers.get(0).unwrap()), true);
    assert_eq!(client.is_signer(&non_signer), false);
}

// ==================== Complex Scenarios ====================

#[test]
fn test_complex_scenario_multiple_proposals() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    // Submit 3 proposals
    let id1 = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal 1"),
        &0_u64,
        &None,
    );

    let id2 = client.submit_proposal(
        &signers.get(1).unwrap(),
        &ActionType::Transfer,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal 2"),
        &0_u64,
        &None,
    );

    let id3 = client.submit_proposal(
        &signers.get(2).unwrap(),
        &ActionType::Custom,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal 3"),
        &0_u64,
        &None,
    );

    // Execute first proposal
    client.sign_proposal(&signers.get(0).unwrap(), &id1);
    client.sign_proposal(&signers.get(1).unwrap(), &id1);
    client.execute_proposal(&id1);

    // Reject second proposal
    client.reject_proposal(&admin, &id2);

    // Sign but don't execute third proposal
    client.sign_proposal(&signers.get(0).unwrap(), &id3);

    // Verify statuses
    assert_eq!(client.get_proposal(&id1).status, ProposalStatus::Executed);
    assert_eq!(client.get_proposal(&id2).status, ProposalStatus::Rejected);
    assert_eq!(client.get_proposal(&id3).status, ProposalStatus::Pending);
    assert_eq!(client.get_signature_count(&id3), 1);
}

#[test]
fn test_signer_management_workflow() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    // Add a new signer
    let new_signer = Address::generate(&e);
    client.add_signer(&admin, &new_signer);
    assert_eq!(client.get_signer_count(), 4);

    // Increase threshold
    client.set_threshold(&admin, &3);

    // Remove old signer
    client.remove_signer(&admin, &signers.get(2).unwrap());
    assert_eq!(client.get_signer_count(), 3);
    assert_eq!(client.get_threshold(), 3); // threshold remains valid

    // Submit and execute proposal with new configuration
    let proposal_id = client.submit_proposal(
        &new_signer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test"),
        &0_u64,
        &None,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&new_signer, &proposal_id);

    client.execute_proposal(&proposal_id);

    assert_eq!(
        client.get_proposal(&proposal_id).status,
        ProposalStatus::Executed
    );
}
