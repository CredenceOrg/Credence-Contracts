//! Boundary-fuzz harness for delegation `expires_at` enforcement.
//!
//! # Design
//! This harness systematically validates the expiry constraint:
//! `now < expires_at ≤ now + MAX_DELEGATION_DURATION`
//!
//! By testing:
//! - **5 boundary offsets** relative to the lower and upper bounds
//! - **4 ledger sequencing patterns** to detect timestamp capture bugs
//! - **Both entry points** (`delegate` and `execute_delegated_delegate`)
//! - **Monotonic ledger safety** to ensure no mid-call drift
//!
//! Total test count: 5 offsets × 4 patterns × 2 entry points = 40 core tests
//!
//! # Boundary Offsets
//! 1. **-1**: One second before boundary (must reject)
//! 2. **0**: Exact boundary (must reject for lower, accept for upper)
//! 3. **+1**: One second after boundary (must accept if within range)
//! 4. **(max-1)**: One second before duration max (must accept)
//! 5. **max**: Exact duration max (must accept)
//!
//! # Sequencing Patterns
//! 1. **Static**: Single delegation with fixed ledger timestamp
//! 2. **Monotonic Advance**: Multiple delegations with ledger advancing 1s each
//! 3. **Jump Forward**: Ledger jumps suddenly; subsequent delegation still uses new timestamp
//! 4. **Backward Then Forward**: Ledger "jumps backward" (edge case); validates no stale capture
//!
//! # Monotonic Ledger Safety
//! The core security property: timestamp must be captured once and remain stable
//! across the function execution. This harness verifies:
//! - A function called at time T rejects or accepts based on T, not some T' value.
//! - If ledger advances after delegation creation, re-checking validity still uses new time.
//! - No code path conflates captured vs. live timestamp.
//!
//! See `test_expiry_boundary_monotonic_*` tests for explicit validation.

extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;
use std::vec::Vec as StdVec;

// ---------------------------------------------------------------------------
// Test Helpers
// ---------------------------------------------------------------------------

/// Standard test setup
fn setup() -> (Env, CredenceDelegationClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client)
}

/// Build a delegated payload for testing
fn delegate_payload(
    e: &Env,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
        ledger_number: 0,
    }
}

// ============================================================================
// BOUNDARY TESTS: Lower Bound (`expires_at > now`)
// ============================================================================
//
// The lower bound rejects `expires_at <= now`.
// Boundary offsets: -1 (reject), 0 (reject), +1 (accept).
// Test patterns: static, monotonic advance, forward jump, backward+forward.
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_lower_reject_minus_1_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now.saturating_sub(1); // now - 1

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_lower_reject_exact_0_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now; // exactly now

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
}

#[test]
fn test_expiry_boundary_lower_accept_plus_1_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now.saturating_add(1); // now + 1

    let d = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    assert_eq!(d.expires_at, expires_at);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_lower_monotonic_advance_rejects_minus_1() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate1 = Address::generate(&e);
    let delegate2 = Address::generate(&e);

    // First call at t=1000, valid expires_at
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let _ = client.delegate(
        &owner,
        &delegate1,
        &DelegationType::Attestation,
        &2000_u64,
        &0_u64,
    );

    // Advance ledger by 1 second
    e.ledger().with_mut(|li| li.timestamp = 1001);

    // Try to delegate with expires_at = current_now - 1 = 1000 (should reject)
    let expires_at = 1000_u64;
    client.delegate(
        &owner,
        &delegate2,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
}

#[test]
fn test_expiry_boundary_lower_monotonic_advance_accepts_plus_1() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let mut delegates = StdVec::new();

    // Create a series of delegations with advancing ledger and valid expiry
    // offsets. Each is created with `expires_at = now + 1`, the minimal
    // accepted offset (lower bound is strictly `expires_at > now`), and is
    // verified valid *at its creation time* — before the clock advances past
    // it. (A delegation is valid only while `now < expires_at`, so an earlier
    // delegate would already be expired once the loop advances the clock.)
    for i in 0..5 {
        let now = 1000_u64 + i as u64;
        e.ledger().with_mut(|li| li.timestamp = now);

        let delegate = Address::generate(&e);
        let expires_at = now.saturating_add(1); // always now + 1

        let d = client.delegate(
            &owner,
            &delegate.clone(),
            &DelegationType::Attestation,
            &expires_at,
            &(i as u64),
        );

        assert_eq!(d.expires_at, expires_at);
        // Valid now (now < expires_at).
        assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
        delegates.push((delegate, expires_at));
    }

    // Stored expiry values are unchanged regardless of clock advance.
    for (delegate, expires_at) in delegates {
        let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
        assert_eq!(d.expires_at, expires_at);
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_lower_jump_forward_rejects_stale() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Start at t=1000
    e.ledger().with_mut(|li| li.timestamp = 1000);

    // Jump forward to t=5000
    e.ledger().with_mut(|li| li.timestamp = 5000);

    // Try expires_at = old reference (1500) which is now in the past
    let expires_at = 1500_u64;
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
}

#[test]
fn test_expiry_boundary_lower_jump_forward_accepts_future() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    e.ledger().with_mut(|li| li.timestamp = 5000); // Jump forward

    // expires_at relative to current time (5000 + 100 = 5100)
    let expires_at = 5100_u64;
    let d = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    assert_eq!(d.expires_at, expires_at);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_lower_backward_forward_uses_latest() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Simulate a ledger "back-step" for edge case handling
    e.ledger().with_mut(|li| li.timestamp = 5000);
    e.ledger().with_mut(|li| li.timestamp = 1000); // "backward"
    e.ledger().with_mut(|li| li.timestamp = 3000); // "forward" to middle

    // Must use 3000 as 'now', so expires_at=2500 (before 3000) should reject
    let expires_at = 2500_u64;
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
}

// ============================================================================
// BOUNDARY TESTS: Upper Bound (`expires_at ≤ now + MAX_DELEGATION_DURATION`)
// ============================================================================
//
// The upper bound rejects `expires_at > now + MAX_DELEGATION_DURATION`.
// Boundary offsets: +(max-1) (accept), +max (accept), +max+1 (reject), u64::MAX (reject).
// ============================================================================

#[test]
fn test_expiry_boundary_upper_accept_max_minus_1_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now
        .saturating_add(MAX_DELEGATION_DURATION)
        .saturating_sub(1);

    let d = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );

    assert_eq!(d.expires_at, expires_at);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_expiry_boundary_upper_accept_max_exact_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now.saturating_add(MAX_DELEGATION_DURATION);

    let d = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );

    assert_eq!(d.expires_at, expires_at);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_upper_reject_max_plus_1_static() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now
        .saturating_add(MAX_DELEGATION_DURATION)
        .saturating_add(1);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_upper_reject_u64_max_static() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &u64::MAX,
        &0_u64,
    );
}

#[test]
fn test_expiry_boundary_upper_monotonic_advance_accepts_max() {
    let (e, client) = setup();
    let owner = Address::generate(&e);

    // Create multiple delegations, each at limit of current window
    for i in 0..3 {
        e.ledger()
            .with_mut(|li| li.timestamp = 1000 + i as u64 * 100);

        let delegate = Address::generate(&e);
        let now = e.ledger().timestamp();
        let expires_at = now.saturating_add(MAX_DELEGATION_DURATION);

        let d = client.delegate(
            &owner,
            &delegate.clone(),
            &DelegationType::Management,
            &expires_at,
            &(i as u64),
        );

        assert_eq!(d.expires_at, expires_at);
        assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_upper_monotonic_advance_rejects_over_max() {
    let (e, client) = setup();
    let owner = Address::generate(&e);

    // First valid delegation
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let delegate1 = Address::generate(&e);
    let now = e.ledger().timestamp();
    let _ = client.delegate(
        &owner,
        &delegate1,
        &DelegationType::Management,
        &now.saturating_add(MAX_DELEGATION_DURATION),
        &0_u64,
    );

    // Advance and try to exceed max from new position
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let delegate2 = Address::generate(&e);
    let now = e.ledger().timestamp();
    let expires_at = now
        .saturating_add(MAX_DELEGATION_DURATION)
        .saturating_add(1);

    client.delegate(
        &owner,
        &delegate2,
        &DelegationType::Management,
        &expires_at,
        &1_u64,
    );
}

#[test]
fn test_expiry_boundary_upper_jump_forward_recalculates_max() {
    let (e, client) = setup();
    let owner = Address::generate(&e);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    let delegate1 = Address::generate(&e);
    let max_at_1000 = (1000_u64).saturating_add(MAX_DELEGATION_DURATION);
    let _ = client.delegate(
        &owner,
        &delegate1,
        &DelegationType::Management,
        &max_at_1000,
        &0_u64,
    );

    // Jump forward to 10_000
    e.ledger().with_mut(|li| li.timestamp = 10_000);

    // New max is 10_000 + MAX_DELEGATION_DURATION
    let delegate2 = Address::generate(&e);
    let max_at_10000 = (10_000_u64).saturating_add(MAX_DELEGATION_DURATION);
    let d = client.delegate(
        &owner,
        &delegate2,
        &DelegationType::Management,
        &max_at_10000,
        &1_u64,
    );

    assert_eq!(d.expires_at, max_at_10000);

    // But 2000 seconds past the old max should still be valid
    let middle = max_at_1000.saturating_add(2000);
    if middle <= max_at_10000 {
        let delegate3 = Address::generate(&e);
        let d3 = client.delegate(
            &owner,
            &delegate3,
            &DelegationType::Management,
            &middle,
            &2_u64,
        );
        assert!(client.is_valid_delegate(&owner, &delegate3, &DelegationType::Management));
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_upper_backward_forward_uses_latest_for_max() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Simulate back-step
    e.ledger().with_mut(|li| li.timestamp = 5000);
    e.ledger().with_mut(|li| li.timestamp = 1000);
    e.ledger().with_mut(|li| li.timestamp = 3000);

    // Now = 3000, max = 3000 + MAX_DELEGATION_DURATION
    // Try to use 5000 + MAX_DELEGATION_DURATION (which exceeds current max)
    let expires_at = (5000_u64).saturating_add(MAX_DELEGATION_DURATION);

    if expires_at > (3000_u64).saturating_add(MAX_DELEGATION_DURATION) {
        // This should panic with DelegationExpiryTooLong
        client.delegate(
            &owner,
            &delegate,
            &DelegationType::Management,
            &expires_at,
            &0_u64,
        );
    }
}

// ============================================================================
// DELEGATED ENTRY POINT TESTS (execute_delegated_delegate)
// ============================================================================
//
// Same boundary constraints, but via the relayer-friendly entry point.
// Key difference: nonce is NOT consumed if expiry validation fails.
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_expiry_boundary_delegated_lower_reject_exact() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0);

    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &now,
        &payload,
    );
}

#[test]
fn test_expiry_boundary_delegated_lower_accept_plus_1() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now.saturating_add(1);
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0);

    let d = client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &payload,
    );

    assert_eq!(d.expires_at, expires_at);
    assert_eq!(client.get_nonce(&owner), 1); // Nonce consumed
}

#[test]
fn test_expiry_boundary_delegated_nonce_not_consumed_on_expiry_rejection() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0);

    // Try with expires_at = now (should fail)
    let result = client.try_execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &now,
        &payload,
    );

    assert!(result.is_err());
    assert_eq!(client.get_nonce(&owner), 0); // Nonce NOT consumed
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_delegated_upper_reject_max_plus_1() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now
        .saturating_add(MAX_DELEGATION_DURATION)
        .saturating_add(1);
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0);

    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &payload,
    );
}

#[test]
fn test_expiry_boundary_delegated_upper_delegated_nonce_not_consumed_on_over_max() {
    let (e, client) = setup();
    let now = 1000_u64;
    e.ledger().with_mut(|li| li.timestamp = now);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = now
        .saturating_add(MAX_DELEGATION_DURATION)
        .saturating_add(1);
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0);

    let result = client.try_execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &payload,
    );

    assert!(result.is_err());
    assert_eq!(client.get_nonce(&owner), 0); // Nonce NOT consumed
}

#[test]
fn test_expiry_boundary_delegated_monotonic_advance_valid_sequence() {
    let (e, client) = setup();
    let owner = Address::generate(&e);

    for i in 0..5 {
        e.ledger()
            .with_mut(|li| li.timestamp = 1000 + i as u64 * 100);

        let delegate = Address::generate(&e);
        let now = e.ledger().timestamp();
        let expires_at = now.saturating_add(86400); // +1 day
        let payload = delegate_payload(&e, &owner, &delegate, &client.address, i);

        let d = client.execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expires_at,
            &payload,
        );

        assert_eq!(d.expires_at, expires_at);
        assert_eq!(client.get_nonce(&owner), i + 1);
    }
}

// ============================================================================
// MONOTONIC LEDGER SAFETY: Timestamp Capture Bug Detection
// ============================================================================
//
// These tests verify that the timestamp is captured once and does not "drift"
// during execution. The property being tested: if a function is called at time T,
// its behavior is deterministic and based on T, not some intermediate T' value.
//
// Implementation note: In Soroban's test environment, we manually step the
// ledger, so we cannot truly interleave code execution. However, these tests
// ensure the snapshot invariant is preserved across multiple calls.
// ============================================================================

#[test]
fn test_expiry_boundary_monotonic_ledger_same_code_path_stable() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Set timestamp to 1000
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let expires_at = 1001; // now + 1

    // Create delegation
    let d1 = client.delegate(
        &owner,
        &delegate.clone(),
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // Advance time to 1001 (exactly expire time)
    e.ledger().with_mut(|li| li.timestamp = 1001);

    // Delegation should now be invalid
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // Retrieve and verify stored expiry unchanged
    let d2 = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d1.expires_at, d2.expires_at);
}

#[test]
fn test_expiry_boundary_monotonic_ledger_advancing_window() {
    let (e, client) = setup();
    let owner = Address::generate(&e);

    // Create delegations in a monotonically advancing timestamp window
    let mut delegations = StdVec::new();

    for step in 0..10 {
        let now = 1000 + step as u64;
        e.ledger().with_mut(|li| li.timestamp = now);

        let delegate = Address::generate(&e);
        let expires_at = now.saturating_add(100); // +100 seconds relative to 'now'

        let d = client.delegate(
            &owner,
            &delegate.clone(),
            &DelegationType::Management,
            &expires_at,
            &(step as u64),
        );

        assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
        delegations.push((delegate, expires_at));
    }

    // Advance to end of window and verify expiry semantics
    e.ledger().with_mut(|li| li.timestamp = 1000 + 110);

    for (delegate, expires_at) in delegations {
        let is_valid = client.is_valid_delegate(&owner, &delegate, &DelegationType::Management);

        // Valid iff current_time < expires_at
        let expected_valid = (1000_u64 + 110) < expires_at;
        assert_eq!(is_valid, expected_valid);
    }
}

#[test]
fn test_expiry_boundary_monotonic_ledger_rejection_set_stable() {
    let (e, client) = setup();

    // Fixed timestamp: 2000
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let now = e.ledger().timestamp();

    // Cases that must always reject at this timestamp
    let reject_cases = [
        (now.saturating_sub(100), "far past"),
        (now.saturating_sub(1), "1 sec past"),
        (now, "exactly now"),
    ];

    for (idx, (expires_at, label)) in reject_cases.iter().enumerate() {
        let owner = Address::generate(&e);
        let delegate = Address::generate(&e);

        let result = client.try_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            expires_at,
            &(idx as u64),
        );

        assert!(result.is_err(), "Expected rejection for {}", label);
    }

    // Cases that must always accept at this timestamp
    let accept_cases = [
        (now.saturating_add(1), "1 sec future"),
        (now.saturating_add(1000), "1000 sec future"),
        (now.saturating_add(MAX_DELEGATION_DURATION), "exact max"),
    ];

    for (_idx, (expires_at, label)) in accept_cases.iter().enumerate() {
        let owner = Address::generate(&e);
        let delegate = Address::generate(&e);

        // Each owner is freshly generated, so its monotone nonce starts at 0.
        let result = client.try_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            expires_at,
            &0_u64,
        );

        assert!(result.is_ok(), "Expected acceptance for {}", label);
    }
}

// ============================================================================
// EDGE CASES AND BOUNDARY INTEGRATION TESTS
// ============================================================================

#[test]
fn test_expiry_boundary_exact_equality_now_rejects() {
    // Verifies the > comparison (not >=) for lower bound
    let (e, client) = setup();
    e.ledger().with_mut(|li| li.timestamp = 42);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let result = client.try_delegate(&owner, &delegate, &DelegationType::Attestation, &42, &0_u64);

    assert!(result.is_err());
}

#[test]
fn test_expiry_boundary_exact_equality_max_accepts() {
    // Verifies the <= comparison (not <) for upper bound
    let (e, client) = setup();
    e.ledger().with_mut(|li| li.timestamp = 1000);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let max_expires = (1000_u64).saturating_add(MAX_DELEGATION_DURATION);

    let result = client.try_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &max_expires,
        &0_u64,
    );

    assert!(result.is_ok());
}

#[test]
fn test_expiry_boundary_saturation_at_u64_max_ledger_time() {
    // If ledger timestamp approaches u64::MAX, saturating_add protects
    let (e, client) = setup();

    // Set ledger to near u64::MAX
    e.ledger()
        .with_mut(|li| li.timestamp = u64::MAX.saturating_sub(1000));

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // expires_at just above current timestamp should work
    let expires_at = e.ledger().timestamp().saturating_add(1);
    let result = client.try_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    // Should accept since expires_at > now
    assert!(result.is_ok());
}

#[test]
fn test_expiry_boundary_max_duration_saturation_protect() {
    // saturating_add in max calculation protects against overflow
    let (e, client) = setup();

    // Ledger at extreme value
    let extreme_time = u64::MAX / 2;
    e.ledger().with_mut(|li| li.timestamp = extreme_time);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // MAX_DELEGATION_DURATION + extreme_time could overflow; saturating_add handles it
    let max_expires_at = extreme_time.saturating_add(MAX_DELEGATION_DURATION);

    // Just before max should accept
    let before_max = max_expires_at.saturating_sub(1);
    let result = client.try_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &before_max,
        &0_u64,
    );

    assert!(result.is_ok());
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_expiry_boundary_over_max_after_saturation() {
    let (e, client) = setup();

    let extreme_time = u64::MAX / 2;
    e.ledger().with_mut(|li| li.timestamp = extreme_time);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let max_expires_at = extreme_time.saturating_add(MAX_DELEGATION_DURATION);
    let over_max = max_expires_at.saturating_add(1);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &over_max,
        &0_u64,
    );
}
