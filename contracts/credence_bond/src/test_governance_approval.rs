//! Comprehensive tests for governance approval for slashing (#7).
//! Covers multi-sig verification, vote tracking, quorum, delegation, and events.

#![cfg(test)]

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    // Use helper that sets up token + bonded identity so governance tests can create bonds safely.
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

fn setup_with_bond_and_governance<'a>(
    e: &'a Env,
    governors: &[Address],
    quorum_bps: u32,
    min_governors: u32,
) -> (CredenceBondClient<'a>, Address, Address) {
    let (client, admin, identity) = setup(e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let mut gov_vec = Vec::new(e);
    for g in governors {
        gov_vec.push_back(g.clone());
    }
    client.initialize_governance(&admin, &gov_vec, &quorum_bps, &min_governors);
    (client, admin, identity)
}

#[test]
fn test_initialize_governance() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);
    let g1 = Address::generate(&e);
    let g2 = Address::generate(&e);
    let governors = Vec::from_array(&e, [g1.clone(), g2.clone()]);
    client.initialize_governance(&admin, &governors, &5100_u32, &1_u32);
    let govs = client.get_governors();
    assert_eq!(govs.len(), 2);
    let (q, min) = client.get_quorum_config();
    assert_eq!(q, 5100);
    assert_eq!(min, 1);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_initialize_governance_unauthorized() {
    let e = Env::default();
    let (client, admin, _) = setup(&e);
    let other = Address::generate(&e);
    let governors = Vec::from_array(&e, [other.clone()]);
    client.initialize_governance(&other, &governors, &5100_u32, &1_u32);
}

#[test]
fn test_propose_slash() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, identity) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    let id = client.propose_slash(&admin, &100_i128);
    assert_eq!(id, 0);
    let prop = client.get_slash_proposal(&id);
    let prop = prop.unwrap();
    assert_eq!(prop.amount, 100);
    assert_eq!(prop.proposed_by, admin);
    assert!(matches!(
        prop.status,
        crate::governance_approval::ProposalStatus::Open
    ));
}

#[test]
fn test_vote_approve_and_execute() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, identity) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    let _id = client.propose_slash(&admin, &100_i128);
    client.governance_vote(&g1, &0_u64, &true);
    let bond = client.execute_slash_with_governance(&admin, &0_u64);
    assert_eq!(bond.slashed_amount, 100);
}

#[test]
#[should_panic(expected = "proposal not approved")]
fn test_vote_reject_then_execute_fails() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, _identity) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    let _id = client.propose_slash(&admin, &100_i128);
    client.governance_vote(&g1, &0_u64, &false);
    client.execute_slash_with_governance(&admin, &0_u64);
}

#[test]
fn test_quorum_two_of_three() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let g2 = Address::generate(&e);
    let g3 = Address::generate(&e);
    let (client, admin, _) =
        setup_with_bond_and_governance(&e, &[g1.clone(), g2.clone(), g3.clone()], 6600, 2);
    let _id = client.propose_slash(&admin, &50_i128);
    client.governance_vote(&g1, &0_u64, &true);
    client.governance_vote(&g2, &0_u64, &true);
    let bond = client.execute_slash_with_governance(&admin, &0_u64);
    assert_eq!(bond.slashed_amount, 50);
}

#[test]
fn test_delegate_vote() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let g2 = Address::generate(&e);
    let delegate_to = Address::generate(&e);
    let (client, admin, _) = setup_with_bond_and_governance(&e, &[g1.clone(), g2.clone()], 5100, 1);
    client.governance_delegate(&g1, &delegate_to);
    let _id = client.propose_slash(&admin, &75_i128);
    client.governance_vote(&delegate_to, &0_u64, &true);
    client.governance_vote(&g2, &0_u64, &true);
    let bond = client.execute_slash_with_governance(&admin, &0_u64);
    assert_eq!(bond.slashed_amount, 75);
}

#[test]
fn test_get_governance_vote() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, _) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    client.propose_slash(&admin, &10_i128);
    assert!(client.get_governance_vote(&0_u64, &g1).is_none());
    client.governance_vote(&g1, &0_u64, &true);
    assert_eq!(client.get_governance_vote(&0_u64, &g1), Some(true));
}

#[test]
#[should_panic(expected = "already voted")]
fn test_double_vote_rejected() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, _) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    client.propose_slash(&admin, &10_i128);
    client.governance_vote(&g1, &0_u64, &true);
    client.governance_vote(&g1, &0_u64, &false);
}

#[test]
#[should_panic(expected = "not a governor or delegate")]
fn test_non_governor_cannot_vote() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let (client, admin, _) = setup_with_bond_and_governance(&e, &[g1.clone()], 5100, 1);
    client.propose_slash(&admin, &10_i128);
    let other = Address::generate(&e);
    client.governance_vote(&other, &0_u64, &true);
}

#[test]
#[should_panic(expected = "only proposer can execute")]
fn test_only_proposer_executes() {
    let e = Env::default();
    let g1 = Address::generate(&e);
    let g2 = Address::generate(&e);
    let (client, admin, _) = setup_with_bond_and_governance(&e, &[g1.clone(), g2.clone()], 5100, 1);
    client.propose_slash(&admin, &50_i128);
    client.governance_vote(&g1, &0_u64, &true);
    client.governance_vote(&g2, &0_u64, &true);
    client.execute_slash_with_governance(&g1, &0_u64);
}
