#![cfg(test)]

use crate::{
    test_helpers::expect_contract_error_panic, CredenceBond, CredenceBondClient, DataKey,
    UpgradeKey,
};
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

#[test]
fn test_upgrade_admin_transfer_full_flow() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);

    // Initial upgrade admin is correct
    let stored: Address = e.as_contract(&contract_id, || {
        e.storage()
            .instance()
            .get(&DataKey::Upgrade(UpgradeKey::Admin))
            .unwrap()
    });
    assert_eq!(stored, admin);

    // Propose transfer
    client.transfer_upgrade_admin(&admin, &new_admin);

    // Pending is set
    assert_eq!(client.get_pending_upgrade_admin(), Some(new_admin.clone()));

    // Fast-forward ledger past the 24-hour timelock (86,400 seconds)
    e.ledger().with_mut(|l| {
        l.timestamp += 86_401;
    });

    // Accept transfer
    client.accept_upgrade_admin(&new_admin);

    // New admin is now the upgrade admin
    let stored: Address = e.as_contract(&contract_id, || {
        e.storage()
            .instance()
            .get(&DataKey::Upgrade(UpgradeKey::Admin))
            .unwrap()
    });
    assert_eq!(stored, new_admin);

    // Pending admin is cleared
    assert_eq!(client.get_pending_upgrade_admin(), None);
}

#[test]
#[should_panic(expected = "Error(Contract, #112)")] // TimelockNotReady
fn test_upgrade_admin_transfer_timelock_enforced() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);

    client.transfer_upgrade_admin(&admin, &new_admin);

    // Attempt to accept before timelock elapses
    e.ledger().with_mut(|l| {
        l.timestamp += 1000; // Only 1000 seconds passed
    });

    client.accept_upgrade_admin(&new_admin); // Should panic
}

#[test]
fn test_upgrade_admin_transfer_expiry_enforced() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);

    client.transfer_upgrade_admin(&admin, &new_admin);

    // Fast-forward past the 7-day expiry (604,800 seconds)
    e.ledger().with_mut(|l| {
        l.timestamp += 604_801;
    });

    expect_contract_error_panic(ContractError::ProposalExpired as u32, || {
        client.accept_upgrade_admin(&new_admin);
    });
}

#[test]
fn test_upgrade_admin_transfer_cancel() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);

    client.transfer_upgrade_admin(&admin, &new_admin);
    assert_eq!(client.get_pending_upgrade_admin(), Some(new_admin.clone()));

    // Admin cancels the transfer
    client.cancel_upgrade_admin_transfer(&admin);
    assert_eq!(client.get_pending_upgrade_admin(), None);
}

#[test]
fn test_upgrade_admin_transfer_wrong_acceptor() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let wrong_admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);
    client.transfer_upgrade_admin(&admin, &new_admin);

    e.ledger().with_mut(|l| {
        l.timestamp += 86_401;
    });

    // Wrong address tries to accept
    expect_contract_error_panic(ContractError::NotPendingUpgradeAdmin as u32, || {
        client.accept_upgrade_admin(&wrong_admin);
    });
}

#[test]
fn test_cannot_propose_same_admin() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    client.initialize(&admin, &None);
    expect_contract_error_panic(ContractError::AdminUnchanged as u32, || {
        client.transfer_upgrade_admin(&admin, &admin);
    });
}
