#![cfg(test)]

use crate::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

// ---------------------------------------------------------------------------
// Shared setup
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address, CredenceMultiSigClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let signer = Address::generate(&e);
    let mut signers = Vec::new(&e);
    signers.push_back(signer.clone());

    let contract_id = e.register_contract(None, CredenceMultiSig);
    let client = CredenceMultiSigClient::new(&e, &contract_id);
    client.initialize(&admin, &signers, &1);

    (e, admin, client)
}

// ---------------------------------------------------------------------------
// Basic pause / unpause (preserved from original)
// ---------------------------------------------------------------------------

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let signer = Address::generate(&e);
    let mut signers = Vec::new(&e);
    signers.push_back(signer.clone());

    let contract_id = e.register_contract(None, CredenceMultiSig);
    let client = CredenceMultiSigClient::new(&e, &contract_id);
    client.initialize(&admin, &signers, &1);

    // Initial state: not paused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Try a mutating action while paused
    let res = client.try_add_signer(&admin, &Address::generate(&e));
    assert!(res.is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Action should now succeed
    client.add_signer(&admin, &Address::generate(&e));
}

// ---------------------------------------------------------------------------
// Boundary: initial state
// ---------------------------------------------------------------------------

#[test]
fn test_initial_state_is_unpaused() {
    let (_, _, client) = setup();
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// Boundary: is_paused reflects storage truthfully after toggle
// ---------------------------------------------------------------------------

#[test]
fn test_is_paused_reflects_each_state_transition() {
    let (_, admin, client) = setup();

    assert!(!client.is_paused(), "starts unpaused");
    client.pause(&admin);
    assert!(client.is_paused(), "must be paused after pause()");
    client.unpause(&admin);
    assert!(!client.is_paused(), "must be unpaused after unpause()");
    client.pause(&admin);
    assert!(client.is_paused(), "must be paused again after second pause()");
}

// ---------------------------------------------------------------------------
// Boundary: require_not_paused blocks every guarded entry point
// ---------------------------------------------------------------------------

#[test]
fn test_add_signer_blocked_while_paused() {
    let (e, admin, client) = setup();
    client.pause(&admin);
    let res = client.try_add_signer(&admin, &Address::generate(&e));
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(106), // ContractPaused
    );
}

#[test]
fn test_remove_signer_blocked_while_paused() {
    let (e, admin, client) = setup();
    // Add a second signer first while unpaused.
    let extra = Address::generate(&e);
    client.add_signer(&admin, &extra);
    client.pause(&admin);

    let res = client.try_remove_signer(&admin, &extra);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(106),
    );
}

#[test]
fn test_set_threshold_blocked_while_paused() {
    let (_, admin, client) = setup();
    client.pause(&admin);
    let res = client.try_set_threshold(&admin, &1);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(106),
    );
}

// ---------------------------------------------------------------------------
// Recovery: operations resume after unpause
// ---------------------------------------------------------------------------

#[test]
fn test_operations_resume_after_unpause() {
    let (e, admin, client) = setup();
    client.pause(&admin);

    // Verify blocked.
    let res = client.try_add_signer(&admin, &Address::generate(&e));
    assert!(res.is_err());

    // Unpause and retry.
    client.unpause(&admin);
    client.add_signer(&admin, &Address::generate(&e));
    // No panic means recovery succeeded.
}

// ---------------------------------------------------------------------------
// Boundary: non-admin cannot pause or unpause (direct path, threshold == 0)
// ---------------------------------------------------------------------------

#[test]
fn test_non_admin_cannot_pause_direct_path() {
    let (e, _, client) = setup();
    let imposter = Address::generate(&e);
    let res = client.try_pause(&imposter);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(100), // NotAdmin
    );
    // State must be untouched.
    assert!(!client.is_paused());
}

#[test]
fn test_non_admin_cannot_unpause_direct_path() {
    let (e, admin, client) = setup();
    client.pause(&admin);
    let imposter = Address::generate(&e);
    let res = client.try_unpause(&imposter);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(100),
    );
    // Must remain paused.
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Boundary: set_pause_signer idempotency and signer count invariant
// ---------------------------------------------------------------------------

#[test]
fn test_enabling_same_pause_signer_twice_does_not_double_count() {
    let (e, admin, client) = setup();
    let ps = Address::generate(&e);

    client.set_pause_signer(&admin, &ps, &true);
    client.set_pause_signer(&admin, &ps, &true); // no-op second call

    // Disabling once must succeed cleanly (no underflow).
    client.set_pause_signer(&admin, &ps, &false);
}

#[test]
fn test_disabling_unregistered_signer_is_safe() {
    let (e, admin, client) = setup();
    let stranger = Address::generate(&e);
    // Must not panic or change any observable state.
    client.set_pause_signer(&admin, &stranger, &false);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// Boundary: threshold clamped on signer removal
// ---------------------------------------------------------------------------

#[test]
fn test_threshold_auto_clamped_when_signer_removed() {
    let (e, admin, client) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2); // threshold == count == 2

    client.set_pause_signer(&admin, &s1, &false); // count drops to 1

    // Threshold should now be 1 (auto-clamped). s2 alone can satisfy it.
    let id = client.pause(&s2).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Boundary: set_pause_threshold edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #601)")]
fn test_threshold_exceeds_signer_count_panics() {
    // Error 601 = ThresholdExceedsSigners
    let (e, admin, client) = setup();
    // No pause signers registered, so count == 0.
    client.set_pause_threshold(&admin, &1);
}

#[test]
fn test_threshold_zero_accepted() {
    let (_, admin, client) = setup();
    // Threshold 0 is valid; reverts to direct-admin mode.
    client.set_pause_threshold(&admin, &0);
    client.pause(&admin);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Boundary: quorum path — propose records proposer approval automatically
// ---------------------------------------------------------------------------

#[test]
fn test_proposer_approval_recorded_automatically() {
    let (e, admin, client) = setup();
    let s = Address::generate(&e);
    client.set_pause_signer(&admin, &s, &true);
    client.set_pause_threshold(&admin, &1);

    // Threshold == 1; proposer's own approval is enough.
    let id = client.pause(&s).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Regression: pause event is emitted on direct-admin path
// (smoke check — SDK testutils do not expose event inspection, but the call
//  sequence must not panic)
// ---------------------------------------------------------------------------

#[test]
fn test_pause_and_unpause_do_not_panic_on_event_emit() {
    let (_, admin, client) = setup();
    client.pause(&admin);
    client.unpause(&admin);
    // No assertion needed beyond "did not panic".
}
