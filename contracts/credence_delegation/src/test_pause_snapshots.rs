extern crate std;

use super::*;
use serde::Serialize;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use std::collections::BTreeMap;

#[derive(Serialize)]
struct ProposalSnapshot {
    action: Option<u32>,
    approval_count: Option<u32>,
    approvals: BTreeMap<std::string::String, bool>,
}

#[derive(Serialize)]
struct StorageSnapshot {
    initialized: bool,
    paused: bool,
    pause_signer_count: u32,
    pause_threshold: u32,
    signers: BTreeMap<std::string::String, bool>,
    proposals: BTreeMap<std::string::String, ProposalSnapshot>,
}

fn dump_pause_storage(
    e: &Env,
    contract_id: &Address,
    signers: &[Address],
    proposal_ids: &[u64],
) -> StorageSnapshot {
    e.as_contract(contract_id, || {
        let storage = e.storage().instance();
        let mut snapshot_signers = BTreeMap::new();
        for (index, signer) in signers.iter().enumerate() {
            let enabled = storage
                .get::<_, bool>(&DataKey::PauseSigner(signer.clone()))
                .unwrap_or(false);
            snapshot_signers.insert(std::format!("signer_{}", index + 1), enabled);
        }

        let mut proposals = BTreeMap::new();
        for (index, proposal_id) in proposal_ids.iter().enumerate() {
            let mut approvals = BTreeMap::new();
            for (signer_index, signer) in signers.iter().enumerate() {
                let approved = storage
                    .get::<_, bool>(&DataKey::PauseApproval(*proposal_id, signer.clone()))
                    .unwrap_or(false);
                approvals.insert(std::format!("signer_{}", signer_index + 1), approved);
            }
            proposals.insert(
                std::format!("proposal_{}", index + 1),
                ProposalSnapshot {
                    action: storage.get(&DataKey::PauseProposal(*proposal_id)),
                    approval_count: storage.get(&DataKey::PauseApprovalCount(*proposal_id)),
                    approvals,
                },
            );
        }

        StorageSnapshot {
            initialized: storage.has(&DataKey::Admin),
            paused: storage.get(&DataKey::Paused).unwrap_or(false),
            pause_signer_count: storage.get(&DataKey::PauseSignerCount).unwrap_or(0),
            pause_threshold: storage.get(&DataKey::PauseThreshold).unwrap_or(0),
            signers: snapshot_signers,
            proposals,
        }
    })
}

fn assert_pause_snapshot(
    name: &str,
    e: &Env,
    contract_id: &Address,
    signers: &[Address],
    proposal_ids: &[u64],
) {
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!(name, dump_pause_storage(e, contract_id, signers, proposal_ids));
    });
}

#[test]
fn test_pause_proposal_lifecycle_snapshots() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let signer1 = Address::generate(&e);
    let signer2 = Address::generate(&e);
    let signer3 = Address::generate(&e);
    let all_signers = std::vec![signer1.clone(), signer2.clone(), signer3.clone()];

    client.initialize(&admin);
    assert_pause_snapshot("01_initial_state", &e, &contract_id, &all_signers, &[]);

    client.set_pause_signer(&admin, &signer1, &true);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_signer(&admin, &signer3, &true);
    client.set_pause_threshold(&admin, &2);
    assert_pause_snapshot("02_signers_set", &e, &contract_id, &all_signers, &[]);

    let prop_id = client.pause(&signer1).unwrap();
    assert_pause_snapshot("03_pause_proposed", &e, &contract_id, &all_signers, &[prop_id]);

    client.approve_pause_proposal(&signer2, &prop_id);
    assert_pause_snapshot("04_pause_approved", &e, &contract_id, &all_signers, &[prop_id]);

    client.execute_pause_proposal(&prop_id);
    assert_pause_snapshot("05_pause_executed", &e, &contract_id, &all_signers, &[prop_id]);

    let unpause_id = client.unpause(&signer3).unwrap();
    assert_pause_snapshot(
        "06_unpause_proposed",
        &e,
        &contract_id,
        &all_signers,
        &[prop_id, unpause_id],
    );

    client.approve_pause_proposal(&signer1, &unpause_id);
    assert_pause_snapshot(
        "07_unpause_approved",
        &e,
        &contract_id,
        &all_signers,
        &[prop_id, unpause_id],
    );

    client.execute_pause_proposal(&unpause_id);
    assert_pause_snapshot(
        "08_unpause_executed",
        &e,
        &contract_id,
        &all_signers,
        &[prop_id, unpause_id],
    );

    client.set_pause_signer(&admin, &signer2, &false);
    assert_pause_snapshot(
        "09_signer_removed",
        &e,
        &contract_id,
        &all_signers,
        &[prop_id, unpause_id],
    );
}

#[test]
fn pause_proposal_remains_pending_when_approvals_are_below_threshold() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let signer1 = Address::generate(&e);
    let signer2 = Address::generate(&e);
    let signers = std::vec![signer1.clone(), signer2.clone()];

    client.initialize(&admin);
    client.set_pause_signer(&admin, &signer1, &true);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_threshold(&admin, &2);

    let proposal_id = client.pause(&signer1).unwrap();
    assert!(client.try_execute_pause_proposal(&proposal_id).is_err());
    assert!(!client.is_paused());

    assert_pause_snapshot(
        "10_pause_execution_rejected_below_threshold",
        &e,
        &contract_id,
        &signers,
        &[proposal_id],
    );
}
