//! Boundary and recovery coverage for `chaos_token::ChaosToken` (issue #1318).
//!
//! `ChaosToken` is the substrate every fault-injection suite in this crate
//! trusts. If it misbehaves — silently wrapping a balance, minting supply on a
//! self-transfer, or firing an armed hostile-token payload twice — the
//! *surrounding* suite reports a protocol bug that does not exist. This module
//! therefore pins the mock's own contract rather than the bond's reaction to
//! it, covering the four axes the acceptance criteria call for:
//!
//! * **success** — healthy paths move value exactly and preserve supply,
//! * **rejection** — each toggle, each validation guard and each unsupported
//!   injection target rejects with a deterministic, diagnosable message,
//! * **boundary** — zero, negative, exactly-the-balance, `i128::MAX` credits
//!   and the one-past-the-boundary cases,
//! * **recovery** — a rejected call leaves no partial state, a retry after the
//!   fault clears applies exactly once, and re-arming starts a clean scenario.
//!
//! ## Why most assertions are behavioural rather than flag reads
//!
//! The failure toggles are intentionally write-only: exposing a `get_fail_*`
//! view would let a test "assert" a flag it just set instead of proving the
//! behaviour. Every toggle test here therefore drives the public SEP-41
//! surface and asserts on the resulting panic or balance.
//!
//! ## Concurrency note
//!
//! Soroban has no intra-transaction threading, so the "concurrent execution"
//! criterion maps onto re-entrancy and call ordering. Both are covered: the
//! injection is asserted to fire at most once per arming (§6), and to be
//! unreachable when an earlier guard in `transfer` has already reverted (§7).

use crate::chaos_token::{ChaosToken, ChaosTokenClient, DEFAULT_BALANCE};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

/// A target that accepts every supported injection method.
///
/// `maybe_reenter` only forwards `withdraw` / `withdraw_early` / `top_up` /
/// `slash` / `collect_fees`, so a permissive target has to expose at least one
/// of those names. `collect_fees` takes a single address, which keeps the
/// payload simple.
#[contract]
pub struct PermissiveTarget;

#[contractimpl]
impl PermissiveTarget {
    pub fn collect_fees(_e: Env, _admin: Address) -> i128 {
        0
    }
}

/// Register a fresh, `initialize`d token and return its id.
fn token(e: &Env) -> (Address, ChaosTokenClient<'_>) {
    let id = e.register(ChaosToken, ());
    let client = ChaosTokenClient::new(e, &id);
    client.initialize();
    (id, client)
}

/// Advance far enough to prove a flag's persistence is storage-backed rather
/// than an artifact of a single invocation's call frame.
fn advance(e: &Env) {
    e.ledger().with_mut(|l| {
        l.sequence_number += 100;
        l.timestamp += 3_600;
    });
}

// ─────────────────── 1. Safe defaults / cold-instance recovery ───────────────────

/// A never-`initialize`d instance must already behave like a healthy token.
/// Every flag read falls back to `false`; if any of them fell back to a panic
/// instead, every existing chaos suite would need a bootstrap call first.
#[test]
fn uninitialised_instance_behaves_like_a_healthy_token() {
    let e = Env::default();
    e.mock_all_auths();
    let id = e.register(ChaosToken, ());
    let t = ChaosTokenClient::new(&e, &id);

    let a = Address::generate(&e);
    let b = Address::generate(&e);

    assert_eq!(t.decimals(), 7);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE);
    assert_eq!(t.allowance(&a, &b), i128::MAX);
    t.approve(&a, &b, &100, &1_000);
    assert!(!t.attack_attempted());
    assert!(!t.attack_rejected());

    t.mint(&a, &500);
    t.transfer(&a, &b, &100);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 400);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 100);
}

/// `initialize` is the documented "safe defaults" entrypoint: it must clear all
/// five failure toggles, not just the ones a scenario happened to arm.
#[test]
fn initialize_clears_every_failure_toggle() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.set_fail_transfer(&true);
    t.set_fail_transfer_from(&true);
    t.set_fail_balance(&true);
    t.set_fail_approve(&true);
    t.set_fail_allowance(&true);

    t.initialize();

    // Every toggle is back off: none of these may revert.
    assert_eq!(t.balance(&a), DEFAULT_BALANCE);
    assert_eq!(t.allowance(&a, &b), i128::MAX);
    t.approve(&a, &b, &1, &1);
    t.transfer(&a, &b, &1);
    t.transfer_from(&a, &a, &b, &1);
}

#[test]
fn initialize_is_idempotent() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.mint(&a, &1_000);
    t.initialize();
    t.initialize();
    t.initialize();

    // A repeated reset must not disturb balances that the scenario owns.
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 1_000);
    t.transfer(&a, &b, &10);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 990);
}

/// `initialize` is a full reset: it must also drop an armed payload, not merely
/// clear the `armed` bit. A scenario that re-initialises between phases must
/// never inherit the previous phase's target.
#[test]
fn initialize_discards_a_previously_armed_attack() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let target = Address::generate(&e);
    t.set_reentry_attack(&target, &Symbol::new(&e, "withdraw"), &a, &b, &100_i128);
    t.initialize();

    t.transfer(&a, &b, &10);

    assert!(
        !t.attack_attempted(),
        "a reset token must not replay the previous phase's injection"
    );
    assert!(!t.attack_rejected());
}

// ───────────────────────── 2. Toggle independence ─────────────────────────

#[test]
fn fail_transfer_rejects_transfer() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.set_fail_transfer(&true);
    assert!(t.try_transfer(&a, &b, &1).is_err());
}

/// The five toggles are independent *reads*, but `transfer_from` delegates to
/// `transfer`, so `fail_transfer` transitively breaks the allowance path too.
/// That coupling is deliberate and is what lets a suite model "the whole token
/// stopped moving value"; pinning it here stops it from being mistaken for a
/// `fail_transfer_from` leak.
#[test]
fn fail_transfer_transitively_breaks_transfer_from() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.set_fail_transfer(&true);
    assert!(t.try_transfer_from(&a, &a, &b, &1).is_err());
}

/// …and the reverse does not hold: the allowance path stays healthy while the
/// plain `transfer` path is faulted.
#[test]
fn fail_transfer_from_leaves_transfer_healthy() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    t.set_fail_transfer_from(&true);
    assert!(t.try_transfer_from(&a, &a, &b, &1).is_err());

    t.transfer(&a, &b, &100);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 900);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 100);
}

#[test]
fn fail_transfer_from_rejects_transfer_from() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.set_fail_transfer_from(&true);
    assert!(t.try_transfer_from(&a, &a, &b, &1).is_err());
}

#[test]
fn fail_balance_rejects_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    t.set_fail_balance(&true);
    assert!(t.try_balance(&a).is_err());
}

/// `transfer` and `mint` both resolve balances internally, so the storage-read
/// fault reaches them. This is the compound scenario documented in the module
/// table ("balance reads succeed but transfers fail" — and its converse).
#[test]
fn fail_balance_also_rejects_transfer_and_mint() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.set_fail_balance(&true);
    assert!(t.try_transfer(&a, &b, &1).is_err());
    assert!(t.try_mint(&a, &1).is_err());
}

/// …while `approve` / `allowance` are untouched: a scenario that only loses the
/// balance read must still be able to model a healthy allowance write.
#[test]
fn fail_balance_leaves_approve_and_allowance_healthy() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.set_fail_balance(&true);
    t.approve(&a, &b, &1, &1);
    assert_eq!(t.allowance(&a, &b), i128::MAX);
    assert!(t.try_balance(&a).is_err());
}

#[test]
fn fail_approve_rejects_only_approve() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.set_fail_approve(&true);
    assert!(t.try_approve(&a, &b, &1, &1).is_err());

    // Independent: reads and transfers still work.
    assert_eq!(t.allowance(&a, &b), i128::MAX);
    t.transfer(&a, &b, &1);
}

#[test]
fn fail_allowance_rejects_only_allowance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.set_fail_allowance(&true);
    assert!(t.try_allowance(&a, &b).is_err());

    t.approve(&a, &b, &1, &1);
    t.transfer(&a, &b, &1);
}

/// Toggles live in *instance* storage, so arming one token must never be
/// observable through another. Without this a multi-token scenario (a chaos
/// token racing a real mock token) would silently inherit fault flags.
#[test]
fn toggles_are_scoped_to_a_single_instance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_faulted_id, faulted) = token(&e);
    let (_healthy_id, healthy) = token(&e);

    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);

    faulted.set_fail_transfer(&true);
    faulted.set_fail_balance(&true);
    faulted.set_fail_allowance(&true);

    // The untouched instance keeps serving every entrypoint.
    assert!(faulted.try_transfer(&a, &b, &1).is_err());
    healthy.transfer(&a, &b, &1);
    healthy.transfer_from(&spender, &a, &b, &1);
    healthy.mint(&a, &1);
    // Two debits of 1 and one credit of 1 against the default balance.
    assert_eq!(healthy.balance(&a), DEFAULT_BALANCE - 1);
    assert_eq!(healthy.balance(&b), DEFAULT_BALANCE + 2);
    assert_eq!(healthy.allowance(&a, &spender), i128::MAX);
    assert!(!healthy.attack_attempted());
}

// ─────────────── 3. A rejected call must not lose or corrupt state ───────────────

/// The property that makes `ChaosToken` safe to build fault suites on: a
/// faulted call is atomic. Both legs must be exactly where they were, so the
/// retry in §4 can be proven to apply once — not twice.
#[test]
fn failed_transfer_leaves_both_balances_untouched() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let before_a = t.balance(&a);
    let before_b = t.balance(&b);

    t.set_fail_transfer(&true);
    assert!(t.try_transfer(&a, &b, &400).is_err());

    assert_eq!(t.balance(&a), before_a, "debit must not be applied");
    assert_eq!(t.balance(&b), before_b, "credit must not be applied");
}

#[test]
fn failed_transfer_from_leaves_both_balances_untouched() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);
    t.mint(&a, &1_000);

    let before_a = t.balance(&a);
    let before_b = t.balance(&b);

    t.set_fail_transfer_from(&true);
    assert!(t.try_transfer_from(&spender, &a, &b, &400).is_err());

    assert_eq!(t.balance(&a), before_a);
    assert_eq!(t.balance(&b), before_b);
}

#[test]
fn failed_mint_leaves_balance_untouched() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let before = t.balance(&a);

    t.set_fail_balance(&true);
    assert!(t.try_mint(&a, &500).is_err());

    t.set_fail_balance(&false);
    assert_eq!(t.balance(&a), before);
}

// ───────────────────────────── 4. Retry / recovery ─────────────────────────────

/// The core recovery guarantee: a faulted attempt followed by a retry applies
/// the debit exactly once. If the failed attempt had leaked a partial write,
/// this assertion would be off by the transferred amount.
#[test]
fn transfer_recovers_exactly_once_after_the_fault_clears() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let start_a = t.balance(&a);
    let start_b = t.balance(&b);

    t.set_fail_transfer(&true);
    assert!(t.try_transfer(&a, &b, &250).is_err());
    t.set_fail_transfer(&false);
    t.transfer(&a, &b, &250);

    assert_eq!(t.balance(&a), start_a - 250);
    assert_eq!(t.balance(&b), start_b + 250);
}

/// Repeated toggling must be idempotent — a retry loop that re-arms the fault
/// must not leave the token in a half-armed state.
#[test]
fn repeated_toggle_writes_are_idempotent() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    for _ in 0..3 {
        t.set_fail_transfer(&true);
    }
    assert!(t.try_transfer(&a, &b, &1).is_err());
    for _ in 0..3 {
        t.set_fail_transfer(&false);
    }

    t.transfer(&a, &b, &1);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 999);
}

/// Clearing a fault restores the *whole* path, including the parts that were
/// never explicitly re-armed.
#[test]
fn clearing_a_toggle_restores_every_downstream_entrypoint() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);
    t.mint(&a, &1_000);

    t.set_fail_transfer_from(&true);
    assert!(t.try_transfer_from(&spender, &a, &b, &1).is_err());
    t.set_fail_transfer_from(&false);
    t.transfer_from(&spender, &a, &b, &100);

    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 900);
}

/// A fault flag is storage-backed, so it must survive a ledger advance: a
/// scenario that spans ledgers cannot accidentally "heal" itself.
#[test]
fn fault_flags_survive_a_ledger_advance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.set_fail_transfer(&true);
    advance(&e);
    assert!(t.try_transfer(&a, &b, &1).is_err());

    t.set_fail_transfer(&false);
    advance(&e);
    t.transfer(&a, &b, &1);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 1);
}

// ────────────────────── 5. Amount / arithmetic boundaries ──────────────────────

#[test]
fn transfer_of_zero_is_a_noop() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    t.transfer(&a, &b, &0);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 1_000);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE);
}

/// A negative amount would debit the recipient and credit the sender, creating
/// supply out of nothing. It must be rejected rather than silently reversed.
#[test]
fn transfer_rejects_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);
    assert!(t.try_transfer(&a, &b, &-100).is_err());
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 1_000);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE);
}

/// `transfer` writes `from` then `to`; when both are the same key the debit is
/// overwritten by the credit and the account ends up with `+amount`. Rejecting
/// the self-transfer keeps the conservation invariant true for every call.
#[test]
fn transfer_rejects_self_transfer() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    t.mint(&a, &1_000);
    let before = t.balance(&a);

    assert!(t.try_transfer(&a, &a, &100).is_err());
    assert_eq!(t.balance(&a), before, "self-transfer must not mint supply");

    // Zero-amount self-transfer is rejected too: the guard is not amount-shaped.
    assert!(t.try_transfer(&a, &a, &0).is_err());
    assert_eq!(t.balance(&a), before);
}

/// Spending the entire balance is legal and must land exactly on zero — the
/// boundary immediately below the rejected case.
#[test]
fn transfer_allows_spending_the_exact_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);
    let available = t.balance(&a);

    t.transfer(&a, &b, &available);
    assert_eq!(t.balance(&a), 0);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + available);
}

#[test]
fn transfer_rejects_one_more_than_the_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);
    let available = t.balance(&a);

    assert!(t.try_transfer(&a, &b, &(available + 1)).is_err());
    assert_eq!(t.balance(&a), available, "an over-spend must not wrap");
    assert_eq!(t.balance(&b), DEFAULT_BALANCE);
}

/// Without checked arithmetic a recipient credit at `i128::MAX` would wrap to a
/// large negative balance and every later assertion would read garbage.
#[test]
fn transfer_rejects_credit_that_would_overflow_i128() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    // Push the recipient to the ceiling, then try to credit it again.
    t.mint(&b, &(i128::MAX - DEFAULT_BALANCE));
    assert_eq!(t.balance(&b), i128::MAX);

    assert!(t.try_transfer(&a, &b, &1).is_err());
    assert_eq!(t.balance(&b), i128::MAX, "overflow must not wrap negative");
}

#[test]
fn mint_of_zero_is_a_noop() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    t.mint(&a, &0);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE);
}

#[test]
fn mint_rejects_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let before = t.balance(&a);
    assert!(t.try_mint(&a, &-1).is_err());
    assert_eq!(t.balance(&a), before);
}

#[test]
fn mint_allows_credit_exactly_to_i128_max() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    t.mint(&a, &(i128::MAX - DEFAULT_BALANCE));
    assert_eq!(t.balance(&a), i128::MAX);
}

#[test]
fn mint_rejects_one_more_than_i128_max() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    t.mint(&a, &(i128::MAX - DEFAULT_BALANCE));
    assert!(t.try_mint(&a, &1).is_err());
    assert_eq!(t.balance(&a), i128::MAX);
}

/// Conservation is the property the whole fault-injection strategy rests on: if
/// value can appear or vanish during a *successful* call, every suite built on
/// this mock is measuring noise.
#[test]
fn repeated_transfers_conserve_total_supply() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let c = Address::generate(&e);
    t.mint(&a, &10_000);

    let total = t.balance(&a) + t.balance(&b) + t.balance(&c);

    let amounts = [1_i128, 0, 7, 999, 0, 3_333, 1, 250];
    for (i, amt) in amounts.iter().enumerate() {
        let leg = match i % 3 {
            0 => (&a, &b),
            1 => (&b, &c),
            _ => (&c, &a),
        };
        t.transfer(&leg.0, &leg.1, amt);
        assert_eq!(
            t.balance(&a) + t.balance(&b) + t.balance(&c),
            total,
            "supply changed after transfer #{i} of {amt}"
        );
    }
}

#[test]
fn transfer_from_conserves_total_supply() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);
    t.mint(&a, &5_000);

    let total = t.balance(&a) + t.balance(&b) + t.balance(&spender);
    t.transfer_from(&spender, &a, &b, &1_250);
    assert_eq!(t.balance(&a) + t.balance(&b) + t.balance(&spender), total);
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 3_750);
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 1_250);
}

/// The allowance is reported as unlimited and is never decremented, so a
/// pull-payment scenario can only fail on the *revert* branch — never because
/// the mock ran out of allowance halfway.
#[test]
fn allowance_is_unlimited_and_never_decremented() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);
    t.mint(&a, &1_000);

    for _ in 0..5 {
        t.transfer_from(&spender, &a, &b, &100);
        assert_eq!(t.allowance(&a, &spender), i128::MAX);
    }
    assert_eq!(t.balance(&a), DEFAULT_BALANCE + 500);
}

#[test]
fn approve_accepts_zero_and_max_and_stays_a_noop() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);

    t.approve(&a, &b, &0, &0);
    t.approve(&a, &b, &i128::MAX, &u32::MAX);
    t.approve(&a, &b, &-1, &0);

    // `approve` stores nothing; `allowance` remains the constant.
    assert_eq!(t.allowance(&a, &b), i128::MAX);
}

// ───────────────── 6. Hostile-token injection lifecycle ─────────────────

#[test]
fn nothing_is_injected_until_the_attack_is_armed() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    t.transfer(&a, &b, &100);
    t.transfer_from(&a, &a, &b, &100);

    assert!(!t.attack_attempted());
    assert!(!t.attack_rejected());
}

/// The single most important injection invariant: an armed payload fires at
/// most once. Every later transfer — and every later ledger — must find the
/// token disarmed, or a hostile token could re-enter repeatedly and a suite
/// would measure the *second* attempt rather than the guarded one.
#[test]
fn armed_attack_fires_exactly_once() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    // No contract lives at this address, so the injected call must be rejected.
    let missing = Address::generate(&e);
    t.set_reentry_attack(
        &missing,
        &Symbol::new(&e, "collect_fees"),
        &a,
        &b,
        &100_i128,
    );

    t.transfer(&a, &b, &100);
    assert!(
        t.attack_attempted(),
        "first transfer must fire the injection"
    );
    assert!(t.attack_rejected());

    // Subsequent transfers move value normally and must not re-inject.
    t.transfer(&a, &b, &100);
    t.transfer_from(&a, &a, &b, &100);
    advance(&e);
    t.transfer(&a, &b, &100);

    assert!(t.attack_attempted());
    assert_eq!(
        t.balance(&a),
        DEFAULT_BALANCE + 600,
        "all four transfers must have applied exactly once each"
    );
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 400);
}

/// A re-arm is the explicit way to start a new scenario; it must clear the
/// previous attempt's bookkeeping so the new outcome cannot be masked by stale
/// flags.
#[test]
fn rearming_clears_the_previous_attempt_flags() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let missing = Address::generate(&e);
    t.set_reentry_attack(&missing, &Symbol::new(&e, "collect_fees"), &a, &b, &100);
    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted() && t.attack_rejected());

    // Re-arm against a target that accepts the call.
    let ok = e.register(PermissiveTarget, ());
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);
    assert!(!t.attack_attempted(), "re-arm must reset the attempt flag");
    assert!(!t.attack_rejected(), "re-arm must reset the rejection flag");

    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
    assert!(
        !t.attack_rejected(),
        "an accepted re-entry must be recorded as not-rejected"
    );
}

/// Re-arming before the payload fires replaces the payload; only the latest one
/// may run.
#[test]
fn rearming_before_the_payload_fires_uses_the_latest_payload() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let rejected_target = Address::generate(&e);
    let ok = e.register(PermissiveTarget, ());

    t.set_reentry_attack(
        &rejected_target,
        &Symbol::new(&e, "collect_fees"),
        &a,
        &b,
        &100,
    );
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
    assert!(
        !t.attack_rejected(),
        "the overwritten payload must not be the one that ran"
    );
}

#[test]
fn transfer_from_also_triggers_the_injection() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let spender = Address::generate(&e);
    t.mint(&a, &1_000);

    let missing = Address::generate(&e);
    t.set_reentry_attack(&missing, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    t.transfer_from(&spender, &a, &b, &100);
    assert!(t.attack_attempted());
    assert!(t.attack_rejected());
}

/// An unsupported method name is a scenario-authoring error. It must abort the
/// whole transfer rather than silently running an empty payload, and it must
/// not leave a half-applied debit behind.
#[test]
fn unsupported_injection_method_rejects_the_transfer_intact() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);
    let before_a = t.balance(&a);
    let before_b = t.balance(&b);

    let missing = Address::generate(&e);
    t.set_reentry_attack(&missing, &Symbol::new(&e, "drain_everything"), &a, &b, &100);

    assert!(t.try_transfer(&a, &b, &100).is_err());
    assert_eq!(t.balance(&a), before_a);
    assert_eq!(t.balance(&b), before_b);
}

/// A payload aimed at an address with no contract is recorded as *rejected* —
/// the token survived the call and the bond was never entered.
#[test]
fn injection_into_an_address_without_a_contract_is_recorded_as_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let missing = Address::generate(&e);
    t.set_reentry_attack(&missing, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    // The outer transfer must still complete — the token observes the failed
    // re-entry instead of unwinding it.
    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
    assert!(t.attack_rejected());
    assert_eq!(t.balance(&b), DEFAULT_BALANCE + 100);
}

/// The accepted branch: when the target really does expose the method, the
/// re-entry succeeds and `attack_rejected` must be `false`. Without this the
/// suite could not distinguish "the bond blocked it" from "the call never
/// happened", which is the difference between a passing and a vacuous reentrancy
/// test.
#[test]
fn injection_into_a_permissive_target_is_recorded_as_accepted() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let ok = e.register(PermissiveTarget, ());
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
    assert!(!t.attack_rejected());
}

/// Guard ordering: the `fail_transfer` toggle is checked before the injection, so
/// a fully faulted token never reaches the target. This is what keeps "token is
/// down" and "token is hostile" two independent scenarios.
#[test]
fn injection_does_not_run_when_the_transfer_toggle_fires_first() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let ok = e.register(PermissiveTarget, ());
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);
    t.set_fail_transfer(&true);

    assert!(t.try_transfer(&a, &b, &100).is_err());
    assert!(
        !t.attack_attempted(),
        "a reverted transfer must not have fired the payload"
    );

    // …and the payload is still armed for the next, healthy transfer.
    t.set_fail_transfer(&false);
    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
}

/// Validation precedes injection for the same reason: a malformed call is the
/// scenario author's bug and must not be reported as a hostile-token event.
#[test]
fn injection_does_not_run_for_an_invalid_transfer() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let ok = e.register(PermissiveTarget, ());
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    assert!(t.try_transfer(&a, &a, &100).is_err());
    assert!(t.try_transfer(&a, &b, &-1).is_err());
    assert!(!t.attack_attempted());
}

/// Two armed tokens are independent attack surfaces; consuming one must not
/// disarm the other.
#[test]
fn injection_state_is_scoped_to_a_single_instance() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id_a, armed) = token(&e);
    let (_id_b, other) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    armed.mint(&a, &1_000);
    other.mint(&a, &1_000);

    let ok = e.register(PermissiveTarget, ());
    armed.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);
    other.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &100);

    armed.transfer(&a, &b, &100);
    assert!(armed.attack_attempted());
    assert!(
        !other.attack_attempted(),
        "one token's injection must not mark another"
    );

    other.transfer(&a, &b, &100);
    assert!(other.attack_attempted());
}

/// A zero re-entry amount is a legal payload (used by suites that only care
/// about *whether* re-entry was attempted).
#[test]
fn injection_accepts_a_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, t) = token(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    t.mint(&a, &1_000);

    let ok = e.register(PermissiveTarget, ());
    t.set_reentry_attack(&ok, &Symbol::new(&e, "collect_fees"), &a, &b, &0);

    t.transfer(&a, &b, &100);
    assert!(t.attack_attempted());
    assert!(!t.attack_rejected());
}
