//! Tests for the `get_pause_proposal_state` monitoring view.
//!
//! Two properties are asserted:
//! 1. The aggregated view always equals the underlying per-key storage reads.
//! 2. The view fields track the proposal through each state transition
//!    (propose → approve → execute), plus the never-allocated case.
//! A final test confirms the entrypoint performs no `require_auth`.

use super::*;
use crate::pausable::PauseAction;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Env, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup {
    e: Env,
    client: CredenceDelegationClient<'static>,
    contract_id: Address,
    signers: [Address; 2],
}

/// Initialize a contract with two pause signers and a 2-of-2 threshold.
fn setup() -> Setup {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2);

    Setup {
        e,
        client,
        contract_id,
        signers: [s1, s2],
    }
}

/// Direct per-key reads, executed inside the contract's storage context, so we
/// can prove the view matches the raw storage entries it aggregates.
fn raw_action(s: &Setup, id: u64) -> u32 {
    s.e.as_contract(&s.contract_id, || {
        s.e.storage()
            .instance()
            .get(&DataKey::PauseProposal(id))
            .unwrap_or(0)
    })
}

fn raw_approval_count(s: &Setup, id: u64) -> u32 {
    s.e.as_contract(&s.contract_id, || {
        s.e.storage()
            .instance()
            .get(&DataKey::PauseApprovalCount(id))
            .unwrap_or(0)
    })
}

fn raw_approved(s: &Setup, id: u64, signer: &Address) -> bool {
    s.e.as_contract(&s.contract_id, || {
        s.e.storage()
            .instance()
            .get(&DataKey::PauseApproval(id, signer.clone()))
            .unwrap_or(false)
    })
}

fn all_signers(s: &Setup) -> Vec<Address> {
    vec![&s.e, s.signers[0].clone(), s.signers[1].clone()]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The view must reproduce exactly what the four storage entries hold.
#[test]
fn view_matches_per_key_reads() {
    let s = setup();
    let [s1, s2] = s.signers.clone();

    let id = s.client.pause(&s1).unwrap(); // proposes Pause, auto-approves s1
    s.client.approve_pause_proposal(&s2, &id);

    let view = s.client.get_pause_proposal_state(&id, &all_signers(&s));

    assert_eq!(view.proposal_id, id);
    assert_eq!(view.action, raw_action(&s, id));
    assert_eq!(view.approvals, raw_approval_count(&s, id));

    // approved_by must be exactly the supplied signers whose flag is set.
    let mut expected = Vec::new(&s.e);
    for signer in all_signers(&s).iter() {
        if raw_approved(&s, id, &signer) {
            expected.push_back(signer);
        }
    }
    assert_eq!(view.approved_by, expected);
}

/// Walk a proposal through every transition and assert the view at each step.
#[test]
fn view_tracks_state_transitions() {
    let s = setup();
    let [s1, s2] = s.signers.clone();

    // (0) Never-allocated id: no payload, not executed.
    let before = s.client.get_pause_proposal_state(&0, &all_signers(&s));
    assert_eq!(before.action, 0);
    assert_eq!(before.approvals, 0);
    assert_eq!(before.approved_by.len(), 0);
    assert!(!before.executed);

    // (1) Proposed by s1 — Pause action, one self-approval.
    let id = s.client.pause(&s1).unwrap();
    let proposed = s.client.get_pause_proposal_state(&id, &all_signers(&s));
    assert_eq!(proposed.action, PauseAction::Pause as u32);
    assert_eq!(proposed.approvals, 1);
    assert_eq!(proposed.approved_by, vec![&s.e, s1.clone()]);
    assert!(!proposed.executed);

    // (2) Second approval by s2 — quorum reached, still in-flight.
    s.client.approve_pause_proposal(&s2, &id);
    let approved = s.client.get_pause_proposal_state(&id, &all_signers(&s));
    assert_eq!(approved.approvals, 2);
    assert_eq!(approved.approved_by, vec![&s.e, s1.clone(), s2.clone()]);
    assert!(!approved.executed);

    // (3) Executed — payload cleared, so action resets to 0 and executed flips.
    s.client.execute_pause_proposal(&id);
    assert!(s.client.is_paused());
    let executed = s.client.get_pause_proposal_state(&id, &all_signers(&s));
    assert_eq!(executed.action, 0);
    assert_eq!(executed.approvals, 0); // PauseApprovalCount removed on execute
    assert!(executed.executed);
    // Self-consistency with raw storage is preserved post-execution too.
    assert_eq!(executed.action, raw_action(&s, id));
    assert_eq!(executed.approvals, raw_approval_count(&s, id));
}

/// `approved_by` only reports signers in the supplied candidate set.
#[test]
fn approved_by_is_scoped_to_supplied_signers() {
    let s = setup();
    let [s1, s2] = s.signers.clone();
    let id = s.client.pause(&s1).unwrap();
    s.client.approve_pause_proposal(&s2, &id);

    // Ask about only s2: s1 approved too, but is not in the queried set.
    let view = s
        .client
        .get_pause_proposal_state(&id, &vec![&s.e, s2.clone()]);
    assert_eq!(view.approvals, 2); // count is global, independent of the set
    assert_eq!(view.approved_by, vec![&s.e, s2.clone()]);

    // Empty set yields empty approved_by but the other fields still resolve.
    let empty = s.client.get_pause_proposal_state(&id, &Vec::new(&s.e));
    assert_eq!(empty.approved_by.len(), 0);
    assert_eq!(empty.action, PauseAction::Pause as u32);
}

/// The view must be callable with no authorization in scope: it is a pure read
/// and must never invoke `require_auth`.
#[test]
fn view_requires_no_auth() {
    let s = setup();
    let [s1, _s2] = s.signers.clone();
    let id = s.client.pause(&s1).unwrap();

    // Switch the env to enforcing mode with zero pre-authorized entries; any
    // stray require_auth would now panic.
    s.e.set_auths(&[]);
    let view = s.client.get_pause_proposal_state(&id, &all_signers(&s));
    assert_eq!(view.proposal_id, id);
    assert_eq!(view.action, PauseAction::Pause as u32);
}
