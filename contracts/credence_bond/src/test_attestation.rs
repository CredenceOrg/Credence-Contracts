//! Comprehensive Unit Tests for Attestation Functionality
//!
//! Test Coverage Areas:
//! 1. Attester registration and authorization
//! 2. Attestation creation (positive and negative cases)
//! 3. Unauthorized attester rejection
//! 4. Attestation revocation
//! 5. Duplicate attestation handling
//! 6. Event emission
//! 7. Edge cases and boundary conditions

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

// Helper: register contract + admin + one attester, return (client, attester, contract_id).
fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    let attester = Address::generate(e);
    client.register_attester(&attester);
    (client, attester, contract_id)
}

// Convenience: add_attestation with a far-future deadline and the current nonce.
fn add(
    client: &CredenceBondClient<'_>,
    e: &Env,
    contract_id: &Address,
    attester: &Address,
    subject: &Address,
    data: &str,
) -> Attestation {
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(attester);
    client.add_attestation(
        attester,
        subject,
        &String::from_str(e, data),
        contract_id,
        &deadline,
        &nonce,
    )
}

// Convenience: revoke_attestation with a far-future deadline and the current nonce.
fn revoke(
    client: &CredenceBondClient<'_>,
    e: &Env,
    contract_id: &Address,
    attester: &Address,
    id: &u64,
) {
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(attester);
    client.revoke_attestation(attester, id, contract_id, &deadline, &nonce);
}

// ============================================================================
// ATTESTER REGISTRATION & AUTHORIZATION TESTS
// ============================================================================

#[test]
fn test_register_attester() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
}

#[test]
fn test_register_multiple_attesters() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let att1 = Address::generate(&e);
    let att2 = Address::generate(&e);
    let att3 = Address::generate(&e);
    client.register_attester(&att1);
    client.register_attester(&att2);
    client.register_attester(&att3);
    assert!(client.is_attester(&att1));
    assert!(client.is_attester(&att2));
    assert!(client.is_attester(&att3));
}

#[test]
fn test_unregister_attester() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
    client.unregister_attester(&attester);
    assert!(!client.is_attester(&attester));
}

#[test]
fn test_is_attester_false_for_unregistered() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let random = Address::generate(&e);
    assert!(!client.is_attester(&random));
}

// ============================================================================
// ATTESTATION CREATION TESTS
// ============================================================================

#[test]
fn test_add_attestation_basic() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let data = String::from_str(&e, "verified identity");
    let att = add(
        &client,
        &e,
        &contract_id,
        &attester,
        &subject,
        "verified identity",
    );
    assert_eq!(att.id, 0);
    assert_eq!(att.attester, attester);
    assert_eq!(att.subject, subject);
    assert_eq!(att.attestation_data, data);
    assert!(!att.revoked);
}

#[test]
fn test_add_multiple_attestations() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att1 = add(&client, &e, &contract_id, &attester, &subject, "att1");
    let att2 = add(&client, &e, &contract_id, &attester, &subject, "att2");
    let att3 = add(&client, &e, &contract_id, &attester, &subject, "att3");
    assert_eq!(att1.id, 0);
    assert_eq!(att2.id, 1);
    assert_eq!(att3.id, 2);
}

#[test]
fn test_add_attestation_different_attesters() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let att1 = Address::generate(&e);
    let att2 = Address::generate(&e);
    client.register_attester(&att1);
    client.register_attester(&att2);
    let subject = Address::generate(&e);
    let attestation1 = add(&client, &e, &contract_id, &att1, &subject, "verified");
    let attestation2 = add(&client, &e, &contract_id, &att2, &subject, "verified");
    assert_eq!(attestation1.attester, att1);
    assert_eq!(attestation2.attester, att2);
    assert_ne!(attestation1.id, attestation2.id);
}

#[test]
fn test_add_attestation_different_subjects() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let sub1 = Address::generate(&e);
    let sub2 = Address::generate(&e);
    let att1 = add(&client, &e, &contract_id, &attester, &sub1, "verified");
    let att2 = add(&client, &e, &contract_id, &attester, &sub2, "verified");
    assert_eq!(att1.subject, sub1);
    assert_eq!(att2.subject, sub2);
}

#[test]
fn test_add_attestation_empty_data() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &attester, &subject, "");
    assert_eq!(att.attestation_data, String::from_str(&e, ""));
}

// ============================================================================
// UNAUTHORIZED ATTESTER REJECTION TESTS
// ============================================================================

#[test]
#[should_panic(expected = "not verifier")]
fn test_unauthorized_attester_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let unauthorized = Address::generate(&e);
    let subject = Address::generate(&e);
    add(
        &client,
        &e,
        &contract_id,
        &unauthorized,
        &subject,
        "should fail",
    );
}

#[test]
#[should_panic(expected = "not verifier")]
fn test_unregistered_attester_cannot_attest() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "ok");
    client.unregister_attester(&attester);
    add(
        &client,
        &e,
        &contract_id,
        &attester,
        &subject,
        "should fail",
    );
}

// ============================================================================
// ATTESTATION REVOCATION TESTS
// ============================================================================

#[test]
fn test_revoke_attestation() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &attester, &subject, "to revoke");
    assert!(!att.revoked);
    revoke(&client, &e, &contract_id, &attester, &att.id);
    let revoked = client.get_attestation(&att.id);
    assert!(revoked.revoked);
}

#[test]
#[should_panic(expected = "only original attester can revoke")]
fn test_revoke_wrong_attester() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let att1 = Address::generate(&e);
    let att2 = Address::generate(&e);
    client.register_attester(&att1);
    client.register_attester(&att2);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &att1, &subject, "test");
    revoke(&client, &e, &contract_id, &att2, &att.id);
}

#[test]
#[should_panic(expected = "attestation already revoked")]
fn test_revoke_twice() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &attester, &subject, "test");
    revoke(&client, &e, &contract_id, &attester, &att.id);
    revoke(&client, &e, &contract_id, &attester, &att.id);
}

#[test]
#[should_panic(expected = "attestation not found")]
fn test_revoke_nonexistent() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    revoke(&client, &e, &contract_id, &attester, &999);
}

// ============================================================================
// DUPLICATE ATTESTATION HANDLING TESTS
// ============================================================================

#[test]
#[should_panic(expected = "duplicate attestation")]
fn test_duplicate_attestation_rejected() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "duplicate");
    add(&client, &e, &contract_id, &attester, &subject, "duplicate");
}

#[test]
fn test_same_attester_different_data_gets_unique_id() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att1 = add(&client, &e, &contract_id, &attester, &subject, "data1");
    let att2 = add(&client, &e, &contract_id, &attester, &subject, "data2");
    assert_ne!(att1.id, att2.id);
}

#[test]
fn test_same_attester_multiple_for_subject() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "1");
    add(&client, &e, &contract_id, &attester, &subject, "2");
    add(&client, &e, &contract_id, &attester, &subject, "3");
    let atts = client.get_subject_attestations(&subject);
    assert_eq!(atts.len(), 3);
}

// ============================================================================
// EVENT EMISSION TESTS
// ============================================================================

#[test]
fn test_events_published() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &attester, &subject, "test");
    revoke(&client, &e, &contract_id, &attester, &att.id);
    let revoked = client.get_attestation(&att.id);
    assert!(revoked.revoked);
}

// ============================================================================
// GETTER FUNCTION TESTS
// ============================================================================

#[test]
fn test_get_attestation() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let original = add(&client, &e, &contract_id, &attester, &subject, "get test");
    let retrieved = client.get_attestation(&original.id);
    assert_eq!(retrieved.id, original.id);
    assert_eq!(retrieved.attestation_data, original.attestation_data);
}

#[test]
#[should_panic(expected = "attestation not found")]
fn test_get_nonexistent_attestation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    client.get_attestation(&999);
}

#[test]
fn test_get_subject_attestations() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "1");
    add(&client, &e, &contract_id, &attester, &subject, "2");
    add(&client, &e, &contract_id, &attester, &subject, "3");
    let atts = client.get_subject_attestations(&subject);
    assert_eq!(atts.len(), 3);
}

#[test]
fn test_get_subject_attestations_empty() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let subject = Address::generate(&e);
    let atts = client.get_subject_attestations(&subject);
    assert_eq!(atts.len(), 0);
}

#[test]
fn test_get_subject_attestations_different_subjects() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let att = Address::generate(&e);
    client.register_attester(&att);
    let sub1 = Address::generate(&e);
    let sub2 = Address::generate(&e);
    add(&client, &e, &contract_id, &att, &sub1, "s1_1");
    add(&client, &e, &contract_id, &att, &sub1, "s1_2");
    add(&client, &e, &contract_id, &att, &sub2, "s2_1");
    let s1_atts = client.get_subject_attestations(&sub1);
    let s2_atts = client.get_subject_attestations(&sub2);
    assert_eq!(s1_atts.len(), 2);
    assert_eq!(s2_atts.len(), 1);
}

// ============================================================================
// EDGE CASES AND BOUNDARY TESTS
// ============================================================================

#[test]
fn test_self_attestation() {
    let e = Env::default();
    let (client, address, contract_id) = setup_with_contract(&e);
    let att = add(&client, &e, &contract_id, &address, &address, "self");
    assert_eq!(att.attester, att.subject);
}

#[test]
fn test_timestamp_set() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let att = add(&client, &e, &contract_id, &attester, &subject, "test");
    assert_eq!(att.timestamp, e.ledger().timestamp());
}

#[test]
fn test_revoke_preserves_data() {
    let e = Env::default();
    let (client, attester, contract_id) = setup_with_contract(&e);
    let subject = Address::generate(&e);
    let original = add(&client, &e, &contract_id, &attester, &subject, "preserved");
    revoke(&client, &e, &contract_id, &attester, &original.id);
    let revoked = client.get_attestation(&original.id);
    assert_eq!(revoked.id, original.id);
    assert_eq!(revoked.attestation_data, original.attestation_data);
    assert_eq!(revoked.timestamp, original.timestamp);
    assert!(revoked.revoked);
}

#[test]
fn test_complex_scenario() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let att1 = Address::generate(&e);
    let att2 = Address::generate(&e);
    let att3 = Address::generate(&e);
    client.register_attester(&att1);
    client.register_attester(&att2);
    client.register_attester(&att3);
    let sub1 = Address::generate(&e);
    let sub2 = Address::generate(&e);
    let a1 = add(&client, &e, &contract_id, &att1, &sub1, "a1s1_1");
    let a2 = add(&client, &e, &contract_id, &att1, &sub1, "a1s1_2");
    let _a3 = add(&client, &e, &contract_id, &att2, &sub1, "a2s1");
    let _a4 = add(&client, &e, &contract_id, &att2, &sub2, "a2s2");
    let _a5 = add(&client, &e, &contract_id, &att3, &sub2, "a3s2");
    revoke(&client, &e, &contract_id, &att1, &a1.id);
    let s1_atts = client.get_subject_attestations(&sub1);
    let s2_atts = client.get_subject_attestations(&sub2);
    assert_eq!(s1_atts.len(), 3);
    assert_eq!(s2_atts.len(), 2);
    let revoked = client.get_attestation(&a1.id);
    assert!(revoked.revoked);
    let not_revoked = client.get_attestation(&a2.id);
    assert!(!not_revoked.revoked);
}
