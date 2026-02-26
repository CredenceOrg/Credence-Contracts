//! Integration tests for governance slash flow (#48).
//! Covers slash request submission, multi-sig approval, slash execution,
//! contested/rejected flow (dispute-style resolution), delegation, and state consistency.

#![cfg(test)]

use crate::governance_approval::ProposalStatus;
use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn setup(
    e: &Env,
) -> (
    CredenceBondClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    let g1 = Address::generate(e);
    let g2 = Address::generate(e);
    let g3 = Address::generate(e);

    client.create_bond(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);

    let governors = Vec::from_array(e, [g1.clone(), g2.clone(), g3.clone()]);
    client.initialize_governance(&admin, &governors, &6_600_u32, &2_u32);

    (client, admin, identity, g1, g2, g3)
}

/// Scenario: slash request submission is persisted with exact proposer, amount and status.
#[test]
fn test_governance_slash_request_submission() {
    let e = Env::default();
    let (client, admin, _identity, ..) = setup(&e);

    let proposal_id = client.propose_slash(&admin, &250_i128);
    assert_eq!(proposal_id, 0);

    let proposal = client
        .get_slash_proposal(&proposal_id)
        .expect("proposal should exist");

    assert_eq!(proposal.id, 0);
    assert_eq!(proposal.amount, 250);
    assert_eq!(proposal.proposed_by, admin);
    assert!(matches!(proposal.status, ProposalStatus::Open));
}

/// Scenario: multi-sig votes approve proposal, proposer executes slash, and bond state stays consistent.
#[test]
fn test_governance_multisig_approval_and_execution() {
    let e = Env::default();
    let (client, admin, identity, g1, g2, _g3) = setup(&e);

    let before = client.get_identity_state();
    assert_eq!(before.identity, identity);
    assert_eq!(before.bonded_amount, 1_000);
    assert_eq!(before.slashed_amount, 0);

    let proposal_id = client.propose_slash(&admin, &300_i128);

    client.governance_vote(&g1, &proposal_id, &true);
    client.governance_vote(&g2, &proposal_id, &true);

    assert_eq!(client.get_governance_vote(&proposal_id, &g1), Some(true));
    assert_eq!(client.get_governance_vote(&proposal_id, &g2), Some(true));

    let after_slash = client.execute_slash_with_governance(&admin, &proposal_id);

    assert_eq!(after_slash.bonded_amount, 1_000);
    assert_eq!(after_slash.slashed_amount, 300);
    assert!(after_slash.active);

    let proposal = client
        .get_slash_proposal(&proposal_id)
        .expect("proposal should still be readable");
    assert!(matches!(proposal.status, ProposalStatus::Executed));
}

/// Scenario: contested votes block slash execution (dispute-style resolution by rejection).
#[test]
#[should_panic(expected = "proposal not approved")]
fn test_governance_contested_flow_rejects_execution() {
    let e = Env::default();
    let (client, admin, _identity, g1, g2, _g3) = setup(&e);

    let disputed_id = client.propose_slash(&admin, &400_i128);
    client.governance_vote(&g1, &disputed_id, &true);
    client.governance_vote(&g2, &disputed_id, &false);
    client.execute_slash_with_governance(&admin, &disputed_id);
}

/// Scenario: after a contested request, governance can submit and approve a replacement slash request.
#[test]
fn test_governance_reproposal_after_contested_request() {
    let e = Env::default();
    let (client, admin, _identity, g1, g2, _g3) = setup(&e);

    let disputed_id = client.propose_slash(&admin, &400_i128);
    client.governance_vote(&g1, &disputed_id, &true);
    client.governance_vote(&g2, &disputed_id, &false);

    // Do not execute the disputed proposal; open a replacement proposal with updated amount.
    let accepted_id = client.propose_slash(&admin, &150_i128);
    client.governance_vote(&g1, &accepted_id, &true);
    client.governance_vote(&g2, &accepted_id, &true);

    let bond = client.execute_slash_with_governance(&admin, &accepted_id);
    assert_eq!(bond.slashed_amount, 150);

    let accepted = client
        .get_slash_proposal(&accepted_id)
        .expect("accepted proposal should exist");
    assert!(matches!(accepted.status, ProposalStatus::Executed));
}

/// Scenario: delegated voting counts toward quorum in a multi-actor governance approval flow.
#[test]
fn test_governance_multi_actor_delegation_flow() {
    let e = Env::default();
    let (client, admin, _identity, g1, g2, _g3) = setup(&e);
    let delegate = Address::generate(&e);

    client.governance_delegate(&g1, &delegate);
    assert_eq!(client.get_governance_delegate(&g1), Some(delegate.clone()));

    let proposal_id = client.propose_slash(&admin, &125_i128);
    client.governance_vote(&delegate, &proposal_id, &true);
    client.governance_vote(&g2, &proposal_id, &true);

    let bond = client.execute_slash_with_governance(&admin, &proposal_id);
    assert_eq!(bond.slashed_amount, 125);

    let final_state = client.get_identity_state();
    assert_eq!(final_state.bonded_amount, 1_000);
    assert_eq!(final_state.slashed_amount, 125);
}
