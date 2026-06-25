extern crate std;

use crate::{ActionType, CredenceMultiSig, CredenceMultiSigClient, ProposalStatus};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, String, Vec,
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
#[should_panic(expected = "Error(Contract, #601)")]
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
#[should_panic(expected = "Error(Contract, #601)")]
fn test_initialize_threshold_zero() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);

    client.initialize(&admin, &signers, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #601)")]
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
#[should_panic(expected = "Error(Contract, #405)")]
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
#[should_panic(expected = "Error(Contract, #104)")]
fn test_remove_nonexistent_signer() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let fake_signer = Address::generate(&e);
    client.remove_signer(&admin, &fake_signer);
}

#[test]
#[should_panic(expected = "Error(Contract, #107)")]
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

#[test]
#[should_panic(expected = "Error(Contract, #605)")]
fn test_remove_signer_invalidates_removed_signature_at_execute() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);
    let mut signers = Vec::new(&e);
    for _ in 0..4 {
        signers.push_back(Address::generate(&e));
    }

    client.initialize(&admin, &signers, &3);

    let proposal_id = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
        &BytesN::from_array(&e, &[100; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(2).unwrap(), &proposal_id);

    client.remove_signer(&admin, &signers.get(2).unwrap());

    assert_eq!(client.get_signer_count(), 3);
    assert_eq!(client.get_threshold(), 3);

    client.execute_proposal(&proposal_id);
}

#[test]
fn test_remove_signer_keeps_proposal_executable_when_threshold_still_met() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);
    let mut signers = Vec::new(&e);
    for _ in 0..4 {
        signers.push_back(Address::generate(&e));
    }

    client.initialize(&admin, &signers, &3);

    let proposal_id = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
        &BytesN::from_array(&e, &[101; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(2).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(3).unwrap(), &proposal_id);

    client.remove_signer(&admin, &signers.get(2).unwrap());

    client.execute_proposal(&proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_readd_signer_restores_prior_signature_at_execute() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);
    let mut signers = Vec::new(&e);
    for _ in 0..4 {
        signers.push_back(Address::generate(&e));
    }

    client.initialize(&admin, &signers, &3);

    let proposal_id = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Test proposal"),
        &0_u64,
        &None,
        &BytesN::from_array(&e, &[102; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(2).unwrap(), &proposal_id);

    client.remove_signer(&admin, &signers.get(2).unwrap());
    client.add_signer(&admin, &signers.get(2).unwrap());

    client.execute_proposal(&proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
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
#[should_panic(expected = "Error(Contract, #601)")]
fn test_set_threshold_zero() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.set_threshold(&admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #601)")]
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
        &BytesN::from_array(&e, &[1; 32]),
    );

    assert_eq!(proposal_id, 0);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.id, 0);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.description, description);
}

#[test]
#[should_panic(expected = "Error(Contract, #104)")]
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
        &BytesN::from_array(&e, &[2; 32]),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #107)")]
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
        &BytesN::from_array(&e, &[3; 32]),
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
        &BytesN::from_array(&e, &[4; 32]),
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
        &BytesN::from_array(&e, &[5; 32]),
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
        &BytesN::from_array(&e, &[6; 32]),
    );

    client.sign_proposal(&signer, &proposal_id);

    assert_eq!(client.get_signature_count(&proposal_id), 1);
    assert_eq!(client.has_signed(&proposal_id, &signer), true);
}

#[test]
#[should_panic(expected = "Error(Contract, #104)")]
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
        &BytesN::from_array(&e, &[7; 32]),
    );

    client.sign_proposal(&non_signer, &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #603)")]
fn test_sign_nonexistent_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let signer = signers.get(0).unwrap();
    client.sign_proposal(&signer, &999_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #405)")]
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
        &BytesN::from_array(&e, &[8; 32]),
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
        &BytesN::from_array(&e, &[9; 32]),
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
        &BytesN::from_array(&e, &[10; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #605)")]
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
        &BytesN::from_array(&e, &[11; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id); // only 1 signature, threshold is 2
}

#[test]
#[should_panic(expected = "Error(Contract, #603)")]
fn test_execute_nonexistent_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.execute_proposal(&999_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
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
        &BytesN::from_array(&e, &[12; 32]),
    );

    client.sign_proposal(&signers.get(0).unwrap(), &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);

    client.execute_proposal(&proposal_id);
    client.execute_proposal(&proposal_id); // execute again
}

#[test]
#[should_panic(expected = "operation already executed")]
fn test_execute_duplicate_operation() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let op_hash = BytesN::from_array(&e, &[99; 32]);

    let id1 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Prop 1"),
        &0_u64,
        &None,
        &op_hash,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &id1);
    client.sign_proposal(&signers.get(1).unwrap(), &id1);
    client.execute_proposal(&id1);

    let id2 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Prop 2 identical"),
        &0_u64,
        &None,
        &op_hash, // Identical operation hash
    );

    client.sign_proposal(&signers.get(0).unwrap(), &id2);
    client.sign_proposal(&signers.get(1).unwrap(), &id2);
    client.execute_proposal(&id2); // Should trigger duplicate execution panic
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
        &BytesN::from_array(&e, &[13; 32]),
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
        &BytesN::from_array(&e, &[14; 32]),
    );

    client.reject_proposal(&admin, &proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
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
        &BytesN::from_array(&e, &[15; 32]),
    );

    client.reject_proposal(&admin, &proposal_id);
    client.reject_proposal(&admin, &proposal_id); // reject again
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
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
        &BytesN::from_array(&e, &[16; 32]),
    );

    client.reject_proposal(&admin, &proposal_id);
    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
}

// ==================== Expiration Tests ====================

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
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
        &BytesN::from_array(&e, &[17; 32]),
    );

    e.ledger().with_mut(|li| {
        li.timestamp = 1600; // move past expiration
    });

    client.sign_proposal(&signers.get(1).unwrap(), &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
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
        &BytesN::from_array(&e, &[18; 32]),
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
        &BytesN::from_array(&e, &[19; 32]),
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
        &BytesN::from_array(&e, &[20; 32]),
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
        &BytesN::from_array(&e, &[21; 32]),
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
        &BytesN::from_array(&e, &[22; 32]),
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
        &BytesN::from_array(&e, &[23; 32]),
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
        &BytesN::from_array(&e, &[24; 32]),
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
        &BytesN::from_array(&e, &[25; 32]),
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

#[test]
fn test_prune_expired_proposals_basic() {
    use soroban_sdk::FromVal;
    let e = Env::default();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();
    let signer2 = signers.get(1).unwrap();

    // Proposal 0: Expired (expires_at = 1200, now = 1000) -> will be expired after timestamp advances
    let id0 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Expired proposal"),
        &1200_u64,
        &None,
        &BytesN::from_array(&e, &[30; 32]),
    );

    // Proposal 1: Pending (expires_at = 2000, now = 1000) -> not expired
    let id1 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Pending proposal"),
        &2000_u64,
        &None,
        &BytesN::from_array(&e, &[31; 32]),
    );

    // Proposal 2: Executed (expires_at = 1200) -> will not be pruned because status is Executed
    let id2 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Executed proposal"),
        &1200_u64,
        &None,
        &BytesN::from_array(&e, &[32; 32]),
    );

    client.sign_proposal(&proposer, &id2);
    client.sign_proposal(&signer2, &id2);
    client.execute_proposal(&id2);

    // Proposal 3: Rejected (expires_at = 1200) -> will be expired after timestamp advances -> pruned
    let id3 = client.submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Rejected proposal"),
        &1200_u64,
        &None,
        &BytesN::from_array(&e, &[33; 32]),
    );
    client.reject_proposal(&admin, &id3);

    // Sign Proposal 0
    client.sign_proposal(&proposer, &id0);

    // Move time past 1200 (to 1300)
    e.ledger().with_mut(|li| {
        li.timestamp = 1300;
    });

    // Check pre-conditions
    assert_eq!(client.get_proposal(&id0).status, ProposalStatus::Pending);
    assert_eq!(client.get_signature_count(&id0), 1);
    assert_eq!(client.has_signed(&id0, &proposer), true);

    // Now prune starting from ID 0
    let pruned = client.prune_expired_proposals(&0, &10);
    assert_eq!(pruned, 2); // id0 and id3 are pruned

    // Verify pruned proposals are gone
    assert!(client.try_get_proposal(&id0).is_err());
    assert!(client.try_get_proposal(&id3).is_err());

    // Verify signatures are removed
    assert_eq!(client.get_signature_count(&id0), 0);
    assert_eq!(client.has_signed(&id0, &proposer), false);

    // Verify non-expired proposal 1 is NOT pruned
    let prop1 = client.get_proposal(&id1);
    assert_eq!(prop1.status, ProposalStatus::Pending);

    // Verify executed proposal 2 is NOT pruned and its executed op_hash is preserved
    let prop2 = client.get_proposal(&id2);
    assert_eq!(prop2.status, ProposalStatus::Executed);
    assert_eq!(
        client.is_operation_executed(&BytesN::from_array(&e, &[32; 32])),
        true
    );

    // Verify event using snapshot-based diagnostics
    assert_eq!(pruned, 2, "expected 2 proposals pruned");
}

#[test]
fn test_prune_expired_proposals_max_iter() {
    let e = Env::default();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    let proposer = signers.get(0).unwrap();

    // Create 5 expired proposals (id 0 to 4)
    for i in 0..5 {
        client.submit_proposal(
            &proposer,
            &ActionType::ConfigChange,
            &None,
            &None,
            &None,
            &String::from_str(&e, "Expired"),
            &1200_u64,
            &None,
            &BytesN::from_array(&e, &[i; 32]),
        );
    }

    e.ledger().with_mut(|li| {
        li.timestamp = 1300;
    });

    // Prune with max_iter = 2 starting at 0
    let pruned1 = client.prune_expired_proposals(&0, &2);
    assert_eq!(pruned1, 2); // prunes 0 and 1

    // Verify 0 and 1 are gone, 2 is still there
    assert!(client.try_get_proposal(&0).is_err());
    assert!(client.try_get_proposal(&1).is_err());
    assert!(client.get_proposal(&2).id == 2);

    // Prune with max_iter = 10 starting at 2
    let pruned2 = client.prune_expired_proposals(&2, &10);
    assert_eq!(pruned2, 3); // prunes 2, 3, 4

    assert!(client.try_get_proposal(&2).is_err());
    assert!(client.try_get_proposal(&3).is_err());
    assert!(client.try_get_proposal(&4).is_err());
}

#[test]
fn test_prune_expired_proposals_nonexistent() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    // Prune on non-existent proposals should just return 0 without panicking
    let pruned = client.prune_expired_proposals(&100, &10);
    assert_eq!(pruned, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #106)")]
fn test_prune_expired_proposals_paused() {
    let e = Env::default();
    let (client, admin, signers) = setup(&e);
    client.initialize(&admin, &signers, &2);

    client.pause(&admin);
    assert_eq!(client.is_paused(), true);

    // Calling prune while paused should panic with ContractPaused (#106)
    client.prune_expired_proposals(&0, &10);
}
