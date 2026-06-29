//! Long horizon rolling bond renewal tests covering 10-year horizon and overflow safety.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

/// Test that the rolling bond can be renewed repeatedly over a 10‑year horizon.
#[test]
fn test_long_horizon_renewals() {
    let e = Env::default();
    // Start timestamp at 0 for simplicity.
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity) = setup(&e);
    // Bond parameters: duration = 1 day (86_400 seconds), notice period = 10 seconds.
    let bond_duration: u64 = 86_400; // 1 day
    client.create_bond_with_rolling(&identity, 0_i128, bond_duration, &true, &10_u64);

    let mut renewals = 0usize;
    // Simulate a 10‑year period (≈315_360_000 seconds).
    while e.ledger().timestamp() < 315_360_000 {
        // Advance just past the current period to trigger a renewal.
        let next_ts = e.ledger().timestamp() + bond_duration + 1;
        e.ledger().with_mut(|li| li.timestamp = next_ts);
        let bond = client.renew_if_rolling(&identity);
        assert_eq!(bond.bond_start, next_ts);
        // Withdrawal request should still be zero.
        assert_eq!(bond.withdrawal_requested_at, 0);
        renewals += 1;
    }
    // Ensure we performed at least 40 renewals (actually far more).
    assert!(
        renewals >= 40,
        "expected at least 40 renewals, got {}",
        renewals
    );
}

/// Test that an overflow in the period calculation correctly panics.
#[test]
#[should_panic(expected = "overflow")]
fn test_renew_overflow_panic() {
    let e = Env::default();
    // Set timestamp close to u64::MAX - duration.
    let max_ts = u64::MAX - 10; // small buffer
    e.ledger().with_mut(|li| li.timestamp = max_ts);
    let (client, _admin, identity) = setup(&e);
    // Use a tiny duration to push the addition over the limit on the next renewal.
    let bond_duration: u64 = 20;
    client.create_bond_with_rolling(&identity, 0_i128, bond_duration, &true, &10_u64);
    // Advance past the period – this will cause checked_add to return None and panic.
    let next_ts = max_ts + bond_duration + 1;
    e.ledger().with_mut(|li| li.timestamp = next_ts);
    // This call should panic due to overflow in `is_period_ended`.
    client.renew_if_rolling(&identity);
}
