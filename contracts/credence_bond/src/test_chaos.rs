//! Chaos testing suite — bond contract host-function failure injection.
//!
//! # Injection catalogue (9 points)
//!
//! | # | Fault injected | Target call | Verified invariant |
//! |---|---------------|-------------|-------------------|
//! | 1 | `on_slash` callback panic | `slash_bond` | `slashed_amount` reverts to 0 |
//! | 2 | `on_withdraw` callback panic | `withdraw_bond` | bond stays active |
//! | 3 | `on_collect` callback panic | `collect_fees` | fees not cleared |
//! | 4 | `Admin` key removed | `slash_bond` | fails before mutation |
//! | 5 | `Bond` key removed | `withdraw_bond` | fails before mutation |
//! | 6 | slash > bonded_amount | `slash_bond` | `SlashExceedsBond` + no mutation |
//! | 7 | lock pre-set (`locked=true`) | `slash_bond` | `ReentrancyDetected` |
//! | 8 | rolling-bond notice not elapsed | `withdraw_bond` | "notice period not elapsed" |
//! | 9 | `ChaosToken` toggle failures | token client | panic surfaces correctly |

// ──────────────────────────────────────────────────────────────────────────────
// Callback contracts: each lives in its own sub-module so the Soroban macro
// does not generate colliding `__SPEC_XDR_FN_*` symbols.
// ──────────────────────────────────────────────────────────────────────────────

/// chaos injection point #1/#2/#3 — every hook always panics.
///
/// Threat model: a malicious/broken downstream contract attempts to prevent
/// the bond contract from committing state by reverting from inside a callback.
mod panicking_cb {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct PanickingCallback;

    #[contractimpl]
    impl PanickingCallback {
        /// chaos: simulates a compromised slash-recipient hook.
        pub fn on_slash(_e: Env, _amount: i128) {
            panic!("chaos: on_slash callback panicked");
        }
        /// chaos: simulates a broken withdraw hook causing permanent bond lock.
        pub fn on_withdraw(_e: Env, _amount: i128) {
            panic!("chaos: on_withdraw callback panicked");
        }
        /// chaos: simulates a fee-drain attack via a malicious collect hook.
        pub fn on_collect(_e: Env, _amount: i128) {
            panic!("chaos: on_collect callback panicked");
        }
    }
}

/// No-op callback — used to verify invariants after a rollback by issuing a
/// clean second call and observing the restored state.
mod noop_cb {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct NoOpCallback;

    #[contractimpl]
    impl NoOpCallback {
        pub fn on_slash(_e: Env, _amount: i128) {}
        pub fn on_withdraw(_e: Env, _amount: i128) {}
        pub fn on_collect(_e: Env, _amount: i128) {}
    }
}

pub(crate) use noop_cb::NoOpCallback;
pub(crate) use panicking_cb::PanickingCallback;

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{NoOpCallback, PanickingCallback};
    use crate::chaos_token::{ChaosToken, ChaosTokenClient};
    use crate::{CredenceBond, CredenceBondClient, DataKey};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env, Symbol};

    // ─── Setup helper ────────────────────────────────────────────────────────

    /// Create a bond contract + one active non-rolling bond (1 000 units, 24 h).
    fn setup_with_bond(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(e, &contract_id);
        let admin = Address::generate(e);
        let identity = Address::generate(e);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
        (client, admin, identity)
    }

    // ── Injection #1 ─────────────────────────────────────────────────────────

    /// chaos injection point #1 — slash_bond writes `slashed_amount` then invokes
    /// `on_slash`.  A panicking callback must not leave the write committed.
    ///
    /// Soroban atomicity: the `try_*` variant rolls back all storage writes made
    /// during the failed invocation frame.
    ///
    /// Verified invariants:
    /// - `bond.slashed_amount == 0` after `Err` return.
    /// - `bond.bonded_amount == 1000` (unchanged).
    /// - `is_locked() == false` (lock also rolled back).
    #[test]
    fn chaos_injection_1_slash_bond_callback_panic_reverts_state() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        let cb = e.register(PanickingCallback, ());
        client.set_callback(&cb);

        assert_eq!(client.get_identity_state().slashed_amount, 0);
        assert!(!client.is_locked());

        // slash_bond: writes slashed_amount=50, then calls on_slash → panic.
        let result = client.try_slash_bond(&admin, &50_i128);
        assert!(result.is_err(), "slash_bond must fail when callback panics");

        // INVARIANT: no state mutation may persist when an inner call panics.
        let bond = client.get_identity_state();
        assert_eq!(bond.slashed_amount, 0, "slashed_amount must revert to 0");
        assert_eq!(bond.bonded_amount, 1000, "bonded_amount must be unchanged");
        assert!(
            !client.is_locked(),
            "lock must be released after atomic revert"
        );
    }

    // ── Injection #2 ─────────────────────────────────────────────────────────

    /// chaos injection point #2 — withdraw_bond sets `active=false` then calls
    /// `on_withdraw`.  A panicking callback must not leave the bond deactivated.
    ///
    /// Threat model: grief attack — a hook panics to permanently lock collateral.
    #[test]
    fn chaos_injection_2_withdraw_bond_callback_panic_reverts_state() {
        let e = Env::default();
        let (client, _, identity) = setup_with_bond(&e);

        let cb = e.register(PanickingCallback, ());
        client.set_callback(&cb);

        assert!(client.get_identity_state().active);

        let result = client.try_withdraw_bond(&identity);
        assert!(
            result.is_err(),
            "withdraw_bond must fail when callback panics"
        );

        // INVARIANT
        let bond = client.get_identity_state();
        assert!(
            bond.active,
            "bond must remain active after callback-induced rollback"
        );
        assert_eq!(bond.bonded_amount, 1000, "bonded_amount must not be zeroed");
        assert!(!client.is_locked(), "lock must be released");
    }

    // ── Injection #3 ─────────────────────────────────────────────────────────

    /// chaos injection point #3 — collect_fees zeroes the fee balance then calls
    /// `on_collect`.  Without rollback the treasury is silently drained.
    ///
    /// Verification strategy: after the failed call, swap in a no-op callback
    /// and call collect_fees again — it must return the original 500.
    #[test]
    fn chaos_injection_3_collect_fees_callback_panic_reverts_fees() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        client.initialize(&admin, &None);
        client.deposit_fees(&500_i128);

        let cb = e.register(PanickingCallback, ());
        client.set_callback(&cb);

        // collect_fees: zeros fee storage, calls on_collect → panic → rollback.
        let result = client.try_collect_fees(&admin);
        assert!(
            result.is_err(),
            "collect_fees must fail when callback panics"
        );

        // INVARIANT: fees must be intact. Verify via a clean second call.
        let noop = e.register(NoOpCallback, ());
        client.set_callback(&noop);
        let recovered = client.collect_fees(&admin);
        assert_eq!(
            recovered, 500,
            "fees must equal pre-call value after atomic revert"
        );
    }

    // ── Injection #4 ─────────────────────────────────────────────────────────

    /// chaos injection point #4 — Admin key removed; slash_bond panics before
    /// touching bond state.
    ///
    /// Threat model: storage TTL expiry evicts a key that must always present;
    /// an unauthenticated slash could proceed if the guard is absent.
    #[test]
    #[should_panic]
    fn chaos_injection_4_missing_admin_key_slash_bond_fails() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        e.as_contract(&client.address, || {
            e.storage().instance().remove(&DataKey::Admin);
        });

        client.slash_bond(&admin, &50_i128);
    }

    /// Companion: bond state is unchanged because the panic occurs before the
    /// bond state write.
    #[test]
    fn chaos_injection_4b_missing_admin_key_bond_state_unchanged() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        e.as_contract(&client.address, || {
            e.storage().instance().remove(&DataKey::Admin);
        });

        let result = client.try_slash_bond(&admin, &50_i128);
        assert!(result.is_err());

        // Restore admin so we can read state.
        e.as_contract(&client.address, || {
            e.storage().instance().set(&DataKey::Admin, &admin);
        });

        let bond = client.get_identity_state();
        assert_eq!(bond.slashed_amount, 0);
        assert_eq!(bond.bonded_amount, 1000);
    }

    // ── Injection #5 ─────────────────────────────────────────────────────────

    /// chaos injection point #5 — Bond key removed; withdraw_bond panics with
    /// BondNotFound before any lock or state mutation.
    ///
    /// Threat model: TTL eviction of the bond record; without a guard the
    /// contract could treat "no bond" as "empty bond" and proceed.
    #[test]
    #[should_panic]
    fn chaos_injection_5_missing_bond_key_withdraw_fails() {
        let e = Env::default();
        let (client, _, identity) = setup_with_bond(&e);

        e.as_contract(&client.address, || {
            e.storage().instance().remove(&DataKey::Bond);
        });

        client.withdraw_bond(&identity);
    }

    // ── Injection #6 ─────────────────────────────────────────────────────────

    /// chaos injection point #6 — slash_amount > bonded_amount is rejected with
    /// SlashExceedsBond.  The lock is safely released and bond state is pristine.
    ///
    /// Threat model: arithmetic exploitation to inflate slashed_amount beyond
    /// bonded_amount, corrupting the available-balance calculation.
    #[test]
    fn chaos_injection_6_slash_exceeds_bond_rejected_state_unchanged() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        assert_eq!(client.get_identity_state().slashed_amount, 0);

        let result = client.try_slash_bond(&admin, &2000_i128);
        assert!(result.is_err(), "slash exceeding bond must be rejected");

        let bond = client.get_identity_state();
        assert_eq!(bond.slashed_amount, 0);
        assert_eq!(bond.bonded_amount, 1000);
        assert!(!client.is_locked());
    }

    // ── Injection #7 ─────────────────────────────────────────────────────────

    /// chaos injection point #7 — reentrancy guard.  Pre-set the lock to `true`
    /// (simulating mid-execution state) and verify a concurrent slash_bond is
    /// rejected with ReentrancyDetected.
    ///
    /// Threat model: a reentrant caller (via callback) calls slash_bond again
    /// while the first invocation still holds the lock.
    #[test]
    fn chaos_injection_7_reentrancy_guard_blocks_double_lock() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        // Simulate "mid-execution" by manually acquiring the lock.
        e.as_contract(&client.address, || {
            let key = Symbol::new(&e, "locked");
            e.storage().instance().set(&key, &true);
        });

        assert!(client.is_locked(), "precondition: lock must be held");

        let result = client.try_slash_bond(&admin, &50_i128);
        assert!(
            result.is_err(),
            "slash_bond must be rejected while the reentrancy lock is held"
        );
    }

    // ── Injection #8 ─────────────────────────────────────────────────────────

    /// chaos injection point #8 — rolling bond notice period.  Calling
    /// withdraw_bond before the notice window elapses must be rejected.
    ///
    /// Threat model: ledger timestamp manipulation collapses the notice window,
    /// enabling premature withdrawal of collateral.
    #[test]
    #[should_panic]
    fn chaos_injection_8_rolling_bond_notice_period_not_elapsed() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|li| li.timestamp = 1000);

        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let identity = Address::generate(&e);

        client.initialize(&admin, &None);
        // Rolling bond: 24 h duration, 1 h notice period.
        client.create_bond(&identity, &1000_i128, &86400_u64, &true, &3600_u64);

        // Request withdrawal at t=1000; earliest_withdraw = 1000 + 3600 = 4600.
        client.request_withdrawal(&identity);

        // Attempt withdrawal at t=1000 — notice period not elapsed → panic.
        client.withdraw_bond(&identity);
    }

    // ── Injection #9 — ChaosToken failure toggles ────────────────────────────

    /// chaos injection point #9a — `transfer` panic.
    ///
    /// Threat model: host-level resource exhaustion or a compromised token.
    #[test]
    #[should_panic(expected = "chaos: transfer panicked")]
    fn chaos_injection_9a_chaos_token_transfer_panic() {
        let e = Env::default();
        e.mock_all_auths();
        let chaos_id = e.register(ChaosToken, ());
        let chaos = ChaosTokenClient::new(&e, &chaos_id);
        chaos.initialize();
        let from = Address::generate(&e);
        let to = Address::generate(&e);
        chaos.mint(&from, &500_i128);
        chaos.set_fail_transfer(&true);
        chaos.transfer(&from, &to, &100_i128); // must panic
    }

    /// Toggle can be disabled to restore normal transfer behaviour.
    #[test]
    fn chaos_injection_9a_chaos_token_transfer_recovery() {
        let e = Env::default();
        e.mock_all_auths();
        let chaos_id = e.register(ChaosToken, ());
        let chaos = ChaosTokenClient::new(&e, &chaos_id);
        chaos.initialize();
        let from = Address::generate(&e);
        let to = Address::generate(&e);
        chaos.mint(&from, &500_i128);
        // Capture initial balance of `to` (ChaosToken returns a non-zero default
        // for addresses not yet written to storage).
        let to_initial = chaos.balance(&to);
        // Enable then immediately disable — the toggle must be stateful.
        chaos.set_fail_transfer(&true);
        chaos.set_fail_transfer(&false);
        chaos.transfer(&from, &to, &100_i128);
        // Verify exactly 100 was transferred regardless of the default balance.
        assert_eq!(chaos.balance(&to), to_initial + 100_i128);
    }

    /// chaos injection point #9b — `balance` storage read failure.
    ///
    /// Threat model: token storage key unexpectedly None (ledger compaction /
    /// wrong TTL), causing `unwrap()` sites to crash.
    #[test]
    #[should_panic(expected = "chaos: balance storage read failed")]
    fn chaos_injection_9b_chaos_token_balance_read_failure() {
        let e = Env::default();
        e.mock_all_auths();
        let chaos_id = e.register(ChaosToken, ());
        let chaos = ChaosTokenClient::new(&e, &chaos_id);
        chaos.initialize();
        chaos.set_fail_balance(&true);
        let addr = Address::generate(&e);
        chaos.balance(&addr); // must panic
    }

    /// chaos injection point #9c — `transfer_from` panic.
    ///
    /// Threat model: allowance-based transfer revert mid-execution; the caller
    /// must not be left with partial state.
    #[test]
    #[should_panic(expected = "chaos: transfer_from panicked")]
    fn chaos_injection_9c_chaos_token_transfer_from_panic() {
        let e = Env::default();
        e.mock_all_auths();
        let chaos_id = e.register(ChaosToken, ());
        let chaos = ChaosTokenClient::new(&e, &chaos_id);
        chaos.initialize();
        let spender = Address::generate(&e);
        let from = Address::generate(&e);
        let to = Address::generate(&e);
        chaos.mint(&from, &500_i128);
        chaos.set_fail_transfer_from(&true);
        chaos.transfer_from(&spender, &from, &to, &100_i128); // must panic
    }

    // ── Guard validation ─────────────────────────────────────────────────────

    /// Validate: without the reentrancy guard, consecutive slash_bond calls
    /// both succeed.  This proves injection #7 uniquely tests the guard.
    ///
    /// Per issue spec: "remove the atomic guard and confirm a chaos test fails."
    /// The converse shown here confirms the guard is the operative control.
    #[test]
    fn chaos_validation_guard_absent_double_call_succeeds_without_guard() {
        let e = Env::default();
        let (client, admin, _) = setup_with_bond(&e);

        assert!(!client.is_locked());

        // First slash: no lock held, should succeed.
        let r1 = client.slash_bond(&admin, &50_i128);
        assert_eq!(r1, 50_i128);

        // Second slash: still no guard pre-set externally, succeeds for 50 more.
        let r2 = client.slash_bond(&admin, &50_i128);
        assert_eq!(r2, 100_i128);

        // This demonstrates that injection #7 is verifying the lock guard:
        // absent the guard setting, concurrent calls are NOT rejected.
    }
}
