extern crate std;
use std::format;
use std::string::String;
use std::collections::BTreeMap;
use std::vec;
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Ord, PartialOrd, Eq, PartialEq)]
enum SnapshotKey {
    Admin,
    Paused,
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApprovalCount(u64),
    PauseSigner(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct StorageSnapshot {
    instance: BTreeMap<String, serde_json::Value>,
}

fn dump_pause_storage(e: &Env, contract_id: &Address, signers: &[Address]) -> StorageSnapshot {
    e.as_contract(contract_id, || {
        let mut instance = BTreeMap::new();
        
        // Helper to insert with stringified key
        let mut insert = |key: SnapshotKey, val: serde_json::Value| {
            instance.insert(format!("{:?}", key), val);
        };

        // 1. Fixed keys
        if let Some(admin) = e.storage().instance().get::<_, Address>(&DataKey::Admin) {
            insert(SnapshotKey::Admin, serde_json::to_value(format!("{:?}", admin)).unwrap());
        }
        if let Some(paused) = e.storage().instance().get::<_, bool>(&DataKey::Paused) {
            insert(SnapshotKey::Paused, serde_json::to_value(paused).unwrap());
        }
        if let Some(count) = e.storage().instance().get::<_, u32>(&DataKey::PauseSignerCount) {
            insert(SnapshotKey::PauseSignerCount, serde_json::to_value(count).unwrap());
        }
        if let Some(threshold) = e.storage().instance().get::<_, u32>(&DataKey::PauseThreshold) {
            insert(SnapshotKey::PauseThreshold, serde_json::to_value(threshold).unwrap());
        }
        if let Some(counter) = e.storage().instance().get::<_, u64>(&DataKey::PauseProposalCounter) {
            insert(SnapshotKey::PauseProposalCounter, serde_json::to_value(counter).unwrap());
        }

        // 2. Signer-specific keys
        for signer in signers {
            if let Some(enabled) = e.storage().instance().get::<_, bool>(&DataKey::PauseSigner(signer.clone())) {
                insert(SnapshotKey::PauseSigner(format!("{:?}", signer)), serde_json::to_value(enabled).unwrap());
            }
        }

        // 3. Proposal-specific keys
        let next_id: u64 = e.storage().instance().get(&DataKey::PauseProposalCounter).unwrap_or(0);
        for id in 0..next_id {
            if let Some(action) = e.storage().instance().get::<_, u32>(&DataKey::PauseProposal(id)) {
                insert(SnapshotKey::PauseProposal(id), serde_json::to_value(action).unwrap());
            }
            if let Some(count) = e.storage().instance().get::<_, u32>(&DataKey::PauseApprovalCount(id)) {
                insert(SnapshotKey::PauseApprovalCount(id), serde_json::to_value(count).unwrap());
            }
        }

        StorageSnapshot { instance }
    })
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
    let all_signers = vec![signer1.clone(), signer2.clone(), signer3.clone()];

    // 1. Initial State
    client.initialize(&admin);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("01_initial_state", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 2. Setup signers and threshold
    client.set_pause_signer(&admin, &signer1, &true);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_signer(&admin, &signer3, &true);
    client.set_pause_threshold(&admin, &2);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("02_signers_set", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 3. Propose Pause
    let prop_id = client.pause(&signer1).unwrap();
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("03_pause_proposed", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 4. First Approval (already done by proposer, so this is second approval)
    client.approve_pause_proposal(&signer2, &prop_id);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("04_pause_approved", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 5. Execute Pause
    client.execute_pause_proposal(&prop_id);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("05_pause_executed", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 6. Propose Unpause
    let unpause_id = client.unpause(&signer3).unwrap();
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("06_unpause_proposed", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 7. Approve Unpause
    client.approve_pause_proposal(&signer1, &unpause_id);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("07_unpause_approved", dump_pause_storage(&e, &contract_id, &all_signers));
    });

    // 8. Execute Unpause
    client.execute_pause_proposal(&unpause_id);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("08_unpause_executed", dump_pause_storage(&e, &contract_id, &all_signers));
    });
    
    // 9. Signer removal
    client.set_pause_signer(&admin, &signer2, &false);
    insta::with_settings!({snapshot_path => "../test_snapshots/test_pausable_state"}, {
        insta::assert_json_snapshot!("09_signer_removed", dump_pause_storage(&e, &contract_id, &all_signers));
    });
}
