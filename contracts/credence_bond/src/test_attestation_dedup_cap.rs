use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let attester = Address::generate(e);
    client.initialize(&admin, &None);
    client.register_attester(&attester);
    (client, attester, contract_id)
}

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

#[test]
fn single_path_rejects_duplicate() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "kyc");

    let deadline = e.ledger().timestamp() + 100_000;
    let result = client.try_add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "kyc"),
        &contract_id,
        &deadline,
        &1,
    );

    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            credence_errors::ContractError::DuplicateAttestation as u32
        )))
    );
    assert_eq!(client.get_subject_attestation_count(&subject), 1);
    assert_eq!(client.get_nonce(&attester), 1);
}

#[test]
fn batch_rejects_triple_added_via_single_path() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = Address::generate(&e);
    add(&client, &e, &contract_id, &attester, &subject, "kyc");

    let mut items = Vec::new(&e);
    items.push_back(AttestationBatchItem {
        attester: attester.clone(),
        attestation_data: String::from_str(&e, "kyc"),
        contract_id: contract_id.clone(),
        deadline: e.ledger().timestamp() + 100_000,
        nonce: client.get_nonce(&attester),
    });

    assert_eq!(
        client.try_add_attestation_batch(&subject, &items),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            credence_errors::ContractError::DuplicateAttestation as u32
        )))
    );
    assert_eq!(client.get_subject_attestation_count(&subject), 1);
    assert_eq!(client.get_nonce(&attester), 1);
}

#[test]
fn revoke_then_readd_allowed() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = Address::generate(&e);
    let attestation = add(&client, &e, &contract_id, &attester, &subject, "kyc");

    revoke(&client, &e, &contract_id, &attester, &attestation.id);
    let replacement = add(&client, &e, &contract_id, &attester, &subject, "kyc");

    assert_ne!(replacement.id, attestation.id);
    assert_eq!(client.get_subject_attestation_count(&subject), 1);
}

#[test]
fn cap_enforced_at_1000() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = Address::generate(&e);
    let mut first_id = 0;

    for i in 0..parameters::MAX_ATTESTATIONS {
        let data = std::format!("claim-{i}");
        let attestation = add(&client, &e, &contract_id, &attester, &subject, &data);
        if i == 0 {
            first_id = attestation.id;
        }
    }
    assert_eq!(
        client.get_subject_attestation_count(&subject),
        parameters::MAX_ATTESTATIONS
    );

    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let rejected = client.try_add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "claim-over-cap"),
        &contract_id,
        &deadline,
        &nonce,
    );
    assert_eq!(
        rejected,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            credence_errors::ContractError::StorageCapReached as u32
        )))
    );
    assert_eq!(client.get_nonce(&attester), nonce);

    let mut batch = Vec::new(&e);
    batch.push_back(AttestationBatchItem {
        attester: attester.clone(),
        attestation_data: String::from_str(&e, "batch-over-cap"),
        contract_id: contract_id.clone(),
        deadline,
        nonce,
    });
    assert_eq!(
        client.try_add_attestation_batch(&subject, &batch),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            credence_errors::ContractError::StorageCapReached as u32
        )))
    );
    assert_eq!(client.get_nonce(&attester), nonce);

    revoke(&client, &e, &contract_id, &attester, &first_id);
    add(&client, &e, &contract_id, &attester, &subject, "claim-after-revoke");
    assert_eq!(
        client.get_subject_attestation_count(&subject),
        parameters::MAX_ATTESTATIONS
    );
}

#[test]
fn different_data_not_deduplicated() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = Address::generate(&e);

    let kyc = add(&client, &e, &contract_id, &attester, &subject, "kyc");
    let aml = add(&client, &e, &contract_id, &attester, &subject, "aml");

    assert_ne!(kyc.id, aml.id);
    assert_eq!(client.get_subject_attestation_count(&subject), 2);
}