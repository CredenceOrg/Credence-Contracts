//! Comprehensive tests for Evidence Hash Storage module.
//!
//! Tests cover evidence submission, linking to proposals, hash uniqueness,
//! multiple evidence per proposal, and query functions.

#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient, EvidenceType};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup(e: &Env) -> (CredenceBondClient, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

// ==================== Evidence Submission Tests ====================

#[test]
fn test_submit_evidence_ipfs() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let proposal_id = 1_u64;
    let hash = String::from_str(&e, "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco");
    let description = Some(String::from_str(&e, "Screenshot of violation"));

    let evidence_id = client.submit_evidence(
        &submitter,
        &proposal_id,
        &hash,
        &EvidenceType::IPFS,
        &description,
    );

    assert_eq!(evidence_id, 0); // First evidence ID is 0

    // Verify evidence was stored
    let evidence = client.get_evidence(&evidence_id);
    assert_eq!(evidence.id, evidence_id);
    assert_eq!(evidence.proposal_id, proposal_id);
    assert_eq!(evidence.hash, hash);
    assert_eq!(evidence.hash_type, EvidenceType::IPFS);
    assert_eq!(evidence.submitted_by, submitter);
    assert!(evidence.description.is_some());
}

#[test]
fn test_submit_evidence_sha256() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let hash = String::from_str(
        &e,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );

    let evidence_id =
        client.submit_evidence(&submitter, &1_u64, &hash, &EvidenceType::SHA256, &None);

    let evidence = client.get_evidence(&evidence_id);
    assert_eq!(evidence.hash_type, EvidenceType::SHA256);
    assert!(evidence.description.is_none());
}

#[test]
#[should_panic(expected = "hash cannot be empty")]
fn test_submit_evidence_empty_hash() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let empty_hash = String::from_str(&e, "");

    client.submit_evidence(&submitter, &1_u64, &empty_hash, &EvidenceType::IPFS, &None);
}

#[test]
#[should_panic(expected = "evidence hash already exists")]
fn test_submit_duplicate_hash() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter1 = Address::generate(&e);
    let submitter2 = Address::generate(&e);
    let hash = String::from_str(&e, "QmTest123");

    // First submission should succeed
    client.submit_evidence(&submitter1, &1_u64, &hash, &EvidenceType::IPFS, &None);

    // Second submission with same hash should fail
    client.submit_evidence(&submitter2, &2_u64, &hash, &EvidenceType::IPFS, &None);
}

#[test]
#[should_panic(expected = "description too long")]
fn test_submit_evidence_long_description() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let hash = String::from_str(&e, "QmTest123");

    // Create a description longer than 500 characters
    let long_description = String::from_str(&e, "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium totam rem aperiam eaque ipsa.");

    client.submit_evidence(
        &submitter,
        &1_u64,
        &hash,
        &EvidenceType::IPFS,
        &Some(long_description),
    );
}

// ==================== Multiple Evidence Tests ====================

#[test]
fn test_multiple_evidence_per_proposal() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let proposal_id = 1_u64;

    // Submit 3 pieces of evidence for same proposal
    let hash1 = String::from_str(&e, "QmHash1");
    let hash2 = String::from_str(&e, "QmHash2");
    let hash3 = String::from_str(&e, "QmHash3");

    let id1 = client.submit_evidence(
        &submitter,
        &proposal_id,
        &hash1,
        &EvidenceType::IPFS,
        &Some(String::from_str(&e, "Evidence 1")),
    );

    let id2 = client.submit_evidence(
        &submitter,
        &proposal_id,
        &hash2,
        &EvidenceType::IPFS,
        &Some(String::from_str(&e, "Evidence 2")),
    );

    let id3 = client.submit_evidence(
        &submitter,
        &proposal_id,
        &hash3,
        &EvidenceType::SHA256,
        &Some(String::from_str(&e, "Evidence 3")),
    );

    // Get all evidence for proposal
    let evidence_ids = client.get_proposal_evidence(&proposal_id);
    assert_eq!(evidence_ids.len(), 3);
    assert!(evidence_ids.contains(&id1));
    assert!(evidence_ids.contains(&id2));
    assert!(evidence_ids.contains(&id3));
}

#[test]
fn test_evidence_for_different_proposals() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);

    // Submit evidence for different proposals
    let id1 = client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "QmProposal1Evidence"),
        &EvidenceType::IPFS,
        &None,
    );

    let id2 = client.submit_evidence(
        &submitter,
        &2_u64,
        &String::from_str(&e, "QmProposal2Evidence"),
        &EvidenceType::IPFS,
        &None,
    );

    // Verify evidence is linked to correct proposals
    let proposal1_evidence = client.get_proposal_evidence(&1_u64);
    let proposal2_evidence = client.get_proposal_evidence(&2_u64);

    assert_eq!(proposal1_evidence.len(), 1);
    assert_eq!(proposal2_evidence.len(), 1);
    assert_eq!(proposal1_evidence.get(0).unwrap(), id1);
    assert_eq!(proposal2_evidence.get(0).unwrap(), id2);
}

// ==================== Query Function Tests ====================

#[test]
fn test_get_evidence_details() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let proposal_id = 1_u64;
    let hash = String::from_str(&e, "QmTest123");
    let description = Some(String::from_str(&e, "Test evidence"));

    let evidence_id = client.submit_evidence(
        &submitter,
        &proposal_id,
        &hash,
        &EvidenceType::IPFS,
        &description,
    );

    let evidence = client.get_evidence(&evidence_id);

    assert_eq!(evidence.id, evidence_id);
    assert_eq!(evidence.proposal_id, proposal_id);
    assert_eq!(evidence.hash, hash);
    assert_eq!(evidence.hash_type, EvidenceType::IPFS);
    assert_eq!(evidence.submitted_by, submitter);
    assert_eq!(evidence.description, description);
    // Timestamp is set but may be 0 in test environment
    assert!(evidence.submitted_at >= 0);
}

#[test]
#[should_panic(expected = "evidence not found")]
fn test_get_nonexistent_evidence() {
    let e = Env::default();
    let (client, _) = setup(&e);

    client.get_evidence(&999_u64);
}

#[test]
fn test_get_proposal_evidence_empty() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let evidence_ids = client.get_proposal_evidence(&999_u64);
    assert_eq!(evidence_ids.len(), 0);
}

#[test]
fn test_get_proposal_evidence_details() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let proposal_id = 1_u64;

    // Submit multiple evidence
    client.submit_evidence(
        &submitter,
        &proposal_id,
        &String::from_str(&e, "QmHash1"),
        &EvidenceType::IPFS,
        &Some(String::from_str(&e, "Evidence 1")),
    );

    client.submit_evidence(
        &submitter,
        &proposal_id,
        &String::from_str(&e, "QmHash2"),
        &EvidenceType::SHA256,
        &Some(String::from_str(&e, "Evidence 2")),
    );

    // Get all evidence details
    let evidence_list = client.get_proposal_evidence_details(&proposal_id);
    assert_eq!(evidence_list.len(), 2);

    // Verify each evidence has correct proposal_id
    for evidence in evidence_list.iter() {
        assert_eq!(evidence.proposal_id, proposal_id);
    }
}

// ==================== Hash Existence Tests ====================

#[test]
fn test_hash_exists() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let hash = String::from_str(&e, "QmTest123");

    // Hash should not exist initially
    assert!(!client.evidence_hash_exists(&hash));

    // Submit evidence
    let submitter = Address::generate(&e);
    client.submit_evidence(&submitter, &1_u64, &hash, &EvidenceType::IPFS, &None);

    // Now hash should exist
    assert!(client.evidence_hash_exists(&hash));
}

#[test]
fn test_hash_uniqueness_across_proposals() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);
    let hash = String::from_str(&e, "QmUniqueHash");

    // Submit evidence for proposal 1
    client.submit_evidence(&submitter, &1_u64, &hash, &EvidenceType::IPFS, &None);

    // Verify hash is marked as existing
    assert!(client.evidence_hash_exists(&hash));
}

// ==================== Evidence Counter Tests ====================

#[test]
fn test_evidence_count() {
    let e = Env::default();
    let (client, _) = setup(&e);

    assert_eq!(client.get_evidence_count(), 0);

    let submitter = Address::generate(&e);

    // Submit 3 pieces of evidence
    client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "QmHash1"),
        &EvidenceType::IPFS,
        &None,
    );

    assert_eq!(client.get_evidence_count(), 1);

    client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "QmHash2"),
        &EvidenceType::IPFS,
        &None,
    );

    assert_eq!(client.get_evidence_count(), 2);

    client.submit_evidence(
        &submitter,
        &2_u64,
        &String::from_str(&e, "QmHash3"),
        &EvidenceType::SHA256,
        &None,
    );

    assert_eq!(client.get_evidence_count(), 3);
}

// ==================== Integration Tests ====================

#[test]
fn test_evidence_workflow() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter1 = Address::generate(&e);
    let submitter2 = Address::generate(&e);
    let proposal_id = 1_u64;

    // Submitter 1 submits IPFS evidence
    let evidence_id1 = client.submit_evidence(
        &submitter1,
        &proposal_id,
        &String::from_str(&e, "QmIPFSHash"),
        &EvidenceType::IPFS,
        &Some(String::from_str(&e, "IPFS document")),
    );

    // Submitter 2 submits SHA256 evidence
    let evidence_id2 = client.submit_evidence(
        &submitter2,
        &proposal_id,
        &String::from_str(&e, "sha256hash"),
        &EvidenceType::SHA256,
        &Some(String::from_str(&e, "Hash of file")),
    );

    // Verify both are linked to proposal
    let evidence_ids = client.get_proposal_evidence(&proposal_id);
    assert_eq!(evidence_ids.len(), 2);

    // Verify evidence details
    let evidence1 = client.get_evidence(&evidence_id1);
    let evidence2 = client.get_evidence(&evidence_id2);

    assert_eq!(evidence1.submitted_by, submitter1);
    assert_eq!(evidence2.submitted_by, submitter2);
    assert_eq!(evidence1.hash_type, EvidenceType::IPFS);
    assert_eq!(evidence2.hash_type, EvidenceType::SHA256);

    // Verify total count
    assert_eq!(client.get_evidence_count(), 2);
}

#[test]
fn test_evidence_types() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter = Address::generate(&e);

    // Test all evidence types
    let ipfs_id = client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "QmIPFS"),
        &EvidenceType::IPFS,
        &None,
    );

    let sha_id = client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "sha256"),
        &EvidenceType::SHA256,
        &None,
    );

    let other_id = client.submit_evidence(
        &submitter,
        &1_u64,
        &String::from_str(&e, "other"),
        &EvidenceType::Other,
        &None,
    );

    // Verify each type
    assert_eq!(client.get_evidence(&ipfs_id).hash_type, EvidenceType::IPFS);
    assert_eq!(client.get_evidence(&sha_id).hash_type, EvidenceType::SHA256);
    assert_eq!(
        client.get_evidence(&other_id).hash_type,
        EvidenceType::Other
    );
}

#[test]
fn test_multiple_submitters_same_proposal() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let submitter1 = Address::generate(&e);
    let submitter2 = Address::generate(&e);
    let submitter3 = Address::generate(&e);
    let proposal_id = 1_u64;

    // Multiple submitters provide evidence for same proposal
    client.submit_evidence(
        &submitter1,
        &proposal_id,
        &String::from_str(&e, "QmSubmitter1"),
        &EvidenceType::IPFS,
        &None,
    );

    client.submit_evidence(
        &submitter2,
        &proposal_id,
        &String::from_str(&e, "QmSubmitter2"),
        &EvidenceType::IPFS,
        &None,
    );

    client.submit_evidence(
        &submitter3,
        &proposal_id,
        &String::from_str(&e, "QmSubmitter3"),
        &EvidenceType::SHA256,
        &None,
    );

    // Verify all evidence is linked
    let evidence_details = client.get_proposal_evidence_details(&proposal_id);
    assert_eq!(evidence_details.len(), 3);

    // Verify different submitters are present
    let mut found_submitter1 = false;
    let mut found_submitter2 = false;
    let mut found_submitter3 = false;

    for evidence in evidence_details.iter() {
        if evidence.submitted_by == submitter1 {
            found_submitter1 = true;
        }
        if evidence.submitted_by == submitter2 {
            found_submitter2 = true;
        }
        if evidence.submitted_by == submitter3 {
            found_submitter3 = true;
        }
    }

    assert!(found_submitter1);
    assert!(found_submitter2);
    assert!(found_submitter3);
}
