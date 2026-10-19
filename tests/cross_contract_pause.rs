//! Cross-contract pause propagation tests (issue #1052).
//!
//! Verifies that the paused flag is visible and respected when one contract
//! calls into another contract. When Contract A is paused, any mutating
//! entrypoint call to Contract A — whether initiated directly or via a
//! cross-contract invocation from Contract B — MUST be rejected.
//!
//! ## Test Matrix
//!
//! | Caller           | Target paused? | Expected |
//! |------------------|----------------|----------|
//! | Direct (user)    | Yes            | Reject   |
//! | Cross-contract   | Yes            | Reject   |
//! | Direct (user)    | No             | Accept   |
//! | Cross-contract   | No             | Accept   |

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};

use credence_delegation::{CredenceDelegation, CredenceDelegationClient, DelegationType};

// ── Minimal caller contract that invokes another contract ──────────────────

/// A thin test contract whose only job is to call into another contract.
/// This simulates a cross-contract scenario (e.g., bond → delegation).
#[contract]
pub struct CallerContract;

#[contractimpl]
impl CallerContract {
    /// Attempt to delegate through the delegation contract.
    /// Propagates the delegation contract's result (or panic).
    pub fn delegate_through(
        e: Env,
        target: Address,
        owner: Address,
        delegate: Address,
        duration: u64,
    ) -> bool {
        let args: Vec<Val> = soroban_sdk::vec![
            &e,
            owner.into_val(&e),
            delegate.into_val(&e),
            (DelegationType::Attestation as u32).into_val(&e),
            duration.into_val(&e),
            0_u64.into_val(&e),
        ];
        e.invoke_contract::<bool>(&target, &Symbol::new(&e, "delegate"), args)
    }

    /// Attempt to check pause state of the target.
    pub fn is_target_paused(e: Env, target: Address) -> bool {
        e.invoke_contract::<bool>(&target, &Symbol::new(&e, "is_paused"), Vec::new(&e))
    }

    /// Attempt to pause the target.
    pub fn pause_target(e: Env, target: Address, caller: Address) -> Option<u64> {
        let args: Vec<Val> = soroban_sdk::vec![&e, caller.into_val(&e)];
        e.invoke_contract::<Option<u64>>(&target, &Symbol::new(&e, "pause"), args)
    }

    /// Attempt to unpause the target.
    pub fn unpause_target(e: Env, target: Address, caller: Address) -> Option<u64> {
        let args: Vec<Val> = soroban_sdk::vec![&e, caller.into_val(&e)];
        e.invoke_contract::<Option<u64>>(&target, &Symbol::new(&e, "unpause"), args)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn setup() -> (
    Env,
    CredenceDelegationClient<'static>,
    Address,
    Address,
    Address,
) {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let delegation_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &delegation_id);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    (e, client, admin, owner, delegate)
}

// ── Direct pause tests (baseline) ──────────────────────────────────────────

#[test]
fn test_direct_delegate_blocked_when_paused() {
    let (e, client, admin, owner, delegate) = setup();

    // Baseline: unpaused delegation works
    let ok = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &3600,
        &0_u64,
    );
    assert!(ok);

    // Pause the delegation contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Direct call must be rejected
    let result = client.try_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &3600,
        &0_u64,
    );
    assert!(
        result.is_err(),
        "direct delegate must fail when delegation contract is paused"
    );

    // Cleanup
    client.unpause(&admin);
    assert!(!client.is_paused());
}

// ── Cross-contract pause tests ─────────────────────────────────────────────

#[test]
fn test_cross_contract_delegate_blocked_when_target_paused() {
    let (e, delegation_client, admin, owner, delegate) = setup();

    // Register the caller contract
    let caller_id = e.register(CallerContract, ());

    // Pause the delegation contract first
    delegation_client.pause(&admin);
    assert!(delegation_client.is_paused());

    // Cross-contract call should be rejected
    let result = e.try_invoke_contract::<bool, soroban_sdk::Error>(
        &caller_id,
        &Symbol::new(&e, "delegate_through"),
        soroban_sdk::vec![
            &e,
            delegation_client.address.into_val(&e),
            owner.into_val(&e),
            delegate.into_val(&e),
            3600_u64.into_val(&e),
        ],
    );
    assert!(
        result.is_err(),
        "cross-contract delegate must fail when target is paused"
    );
}

#[test]
fn test_cross_contract_delegate_succeeds_when_target_unpaused() {
    let (e, delegation_client, _admin, owner, delegate) = setup();

    let caller_id = e.register(CallerContract, ());

    // Ensure delegation is unpaused
    assert!(!delegation_client.is_paused());

    // Cross-contract call should succeed
    let result = e.try_invoke_contract::<bool, soroban_sdk::Error>(
        &caller_id,
        &Symbol::new(&e, "delegate_through"),
        soroban_sdk::vec![
            &e,
            delegation_client.address.into_val(&e),
            owner.into_val(&e),
            delegate.into_val(&e),
            3600_u64.into_val(&e),
        ],
    );
    assert!(
        result.is_ok(),
        "cross-contract delegate must succeed when target is unpaused"
    );
}

#[test]
fn test_cross_contract_is_paused_visible() {
    let (e, delegation_client, admin, owner, delegate) = setup();

    // Initial state: unpaused
    assert!(!delegation_client.is_paused());

    // Pause
    delegation_client.pause(&admin);
    assert!(delegation_client.is_paused());

    // Verify is_paused is visible through cross-contract call
    let caller_id = e.register(CallerContract, ());

    let is_paused: bool = e.invoke_contract(
        &caller_id,
        &Symbol::new(&e, "is_target_paused"),
        soroban_sdk::vec![&e, delegation_client.address.into_val(&e)],
    );
    assert!(is_paused, "is_paused must return true across contracts");

    // Unpause
    delegation_client.unpause(&admin);

    let is_paused: bool = e.invoke_contract(
        &caller_id,
        &Symbol::new(&e, "is_target_paused"),
        soroban_sdk::vec![&e, delegation_client.address.into_val(&e)],
    );
    assert!(
        !is_paused,
        "is_paused must return false after unpause across contracts"
    );
}

#[test]
fn test_pause_unpause_via_cross_contract() {
    let (e, delegation_client, admin, owner, delegate) = setup();

    let caller_id = e.register(CallerContract, ());

    // Pause via cross-contract (admin as caller)
    let pid: Option<u64> = e.invoke_contract(
        &caller_id,
        &Symbol::new(&e, "pause_target"),
        soroban_sdk::vec![
            &e,
            delegation_client.address.into_val(&e),
            admin.into_val(&e),
        ],
    );
    // Admin can pause directly (no multisig needed when threshold is 0)
    assert!(delegation_client.is_paused(), "target must be paused");

    // Verify cross-contract calls are blocked while paused
    let result = e.try_invoke_contract::<bool, soroban_sdk::Error>(
        &caller_id,
        &Symbol::new(&e, "delegate_through"),
        soroban_sdk::vec![
            &e,
            delegation_client.address.into_val(&e),
            owner.into_val(&e),
            delegate.into_val(&e),
            3600_u64.into_val(&e),
        ],
    );
    assert!(
        result.is_err(),
        "cross-contract calls must be blocked while paused"
    );

    // Unpause via cross-contract
    let _: Option<u64> = e.invoke_contract(
        &caller_id,
        &Symbol::new(&e, "unpause_target"),
        soroban_sdk::vec![
            &e,
            delegation_client.address.into_val(&e),
            admin.into_val(&e),
        ],
    );
    assert!(!delegation_client.is_paused(), "target must be unpaused");
}

// ── Multi-contract pause independence ──────────────────────────────────────

#[test]
fn test_pause_is_contract_scoped_not_global() {
    let (e, delegation_client, admin, _owner, _delegate) = setup();

    // Pause the delegation contract
    delegation_client.pause(&admin);
    assert!(delegation_client.is_paused());

    // Register a second independent delegation contract
    let admin2 = Address::generate(&e);
    let delegation2_id = e.register(CredenceDelegation, ());
    let client2 = CredenceDelegationClient::new(&e, &delegation2_id);
    client2.initialize(&admin2);

    // The second contract should NOT be paused (pause is per-contract)
    assert!(
        !client2.is_paused(),
        "pause must be per-contract, not global"
    );
}
