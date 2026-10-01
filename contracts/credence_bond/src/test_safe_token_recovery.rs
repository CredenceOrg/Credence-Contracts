//! Adversarial-token and failure-recovery coverage for `safe_token.rs`
//! (issue #1346).
//!
//! Boundary tests in `test_safe_token_boundary.rs` pin the module against a
//! well-behaved SEP-41 token, where "the amount moved" and "the amount was
//! requested" are the same statement. This file breaks that equivalence.
//!
//! ## The asymmetry being documented
//!
//! Both transfer helpers guard with a balance delta, but they measure
//! *different* parties:
//!
//! - `safe_transfer` measures the **sender** — which is always the bond
//!   contract itself. It therefore catches any token behaviour that debits the
//!   contract by more or less than `amount`, but it is structurally blind to
//!   anything that changes only the *recipient's* balance.
//! - `safe_transfer_from` measures the **recipient** — which is again always
//!   the bond contract. It therefore catches everything that changes the
//!   contract's incoming amount, and is structurally blind to the owner's side.
//!
//! For `safe_transfer_from` that is the correct invariant: the contract's
//! custody accounting is what must not be corrupted. For `safe_transfer` it is
//! narrower than the doc comment implies, because the recipient here is a bond
//! operator or withdrawer, not an anonymous third party. A token that
//! short-pays or debits that recipient is accepted silently.
//!
//! These tests assert the *current* behaviour precisely, so the asymmetry is
//! visible and reviewable, and so a future change to either side has to be a
//! deliberate edit rather than a silent drift. Nothing here asserts that the
//! current behaviour is correct in the abstract — see the notes on each test.
//!
//! ## Harness notes
//!
//! `catch_unwind` around `Env`-using closures is safe on soroban-sdk 22.0.11:
//! verified that the `Env` stays fully usable afterwards and that repeated
//! catches keep working. `catch_unwind` is required here rather than
//! `#[should_panic]` because most of these tests must also assert post-failure
//! state (balances, state update skipped), which `should_panic` cannot express.
//!
//! ### Harness caveat: a guard-level rejection is not rolled back
//!
//! There are two different kinds of rejection in this module, and only one of
//! them is atomic in the harness:
//!
//! 1. **The token rejects the transfer** (insufficient balance or allowance).
//!    The panic happens inside the *token's* contract frame, so that frame is
//!    discarded and no balance changes. Tests assert balances are untouched.
//! 2. **The guard rejects an otherwise-successful transfer** (the balance delta
//!    does not equal the requested amount). The token's `transfer` has already
//!    completed and returned `Ok`; `safe_token` then panics in the *caller's*
//!    frame. `catch_unwind` catches a bare Rust panic with no transaction
//!    boundary, so the token's writes persist here even though in production the
//!    panic would revert the whole transaction.
//!
//! Tests for case 2 therefore assert the *verdict* (rejected, every time) and
//! that the state update never ran — which is the property that actually
//! matters — while accounting for the movement the token already applied. They
//! deliberately do **not** assert "nothing moved", because that would encode a
//! harness artifact as if it were a contract guarantee.
//!
//! ### `checked_sub` is not a negativity check
//!
//! `safe_token` guards its balance deltas with `checked_sub(...).expect("balance
//! underflow")`. That is an i128 *arithmetic overflow* guard, not a sign check:
//! `1000.checked_sub(1100)` is `Some(-100)`, because `-100` is representable. A
//! token that moves the contract's balance the wrong way therefore produces a
//! negative delta that fails the *equality* check and surfaces as
//! `TRANSFER_AMOUNT_MISMATCH`. The `"balance underflow"` string is effectively
//! unreachable for realistic balances. Both paths reject the transfer, so the
//! security property holds, but the error string is misleading to anyone
//! debugging a rejected withdrawal.

#![allow(clippy::disallowed_macros)]
extern crate std;

use crate::safe_token::{
    atomic_transfer_and_update, errors, force_approve, safe_approve, safe_increase_allowance,
    safe_transfer, safe_transfer_from,
};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

// ---------------------------------------------------------------------------
// Adversarial token
// ---------------------------------------------------------------------------

/// How the token misbehaves during a transfer, expressed in terms of the
/// *requested* amount.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeeMode {
    /// Well behaved: debit and credit the requested amount.
    Clean,
    /// Debit the sender `amount + fee`, credit the recipient `amount`.
    SenderPaysMore(i128),
    /// Debit the sender `amount`, credit the recipient `amount - fee`.
    RecipientGetsLess(i128),
    /// Debit the sender `amount`, credit the recipient `amount + fee`.
    RecipientGetsMore(i128),
    /// Debit and credit both parties `amount - fee`.
    SenderGetsRefund(i128),
    /// Credit the *sender* `amount` (mint-on-transfer / rebase token).
    SenderIsCredited(i128),
    /// Debit the *recipient* instead of crediting it.
    RecipientIsDebited(i128),
}

#[contracttype]
#[derive(Clone)]
enum AdvKey {
    Balance(Address),
    Allowance(Address, Address),
    Mode,
    ApproveCalls,
}

#[contract]
pub struct AdversarialToken;

#[contractimpl]
impl AdversarialToken {
    /// Set the misbehaviour for subsequent transfers.
    pub fn set_mode(e: Env, mode: FeeMode) {
        e.storage().instance().set(&AdvKey::Mode, &mode);
    }

    /// How many times `approve` has been called — lets tests observe the
    /// reset-then-set sequence inside `force_approve`.
    pub fn approve_calls(e: Env) -> i128 {
        e.storage()
            .instance()
            .get(&AdvKey::ApproveCalls)
            .unwrap_or(0)
    }

    pub fn mint(e: Env, to: Address, amount: i128) {
        Self::_credit(&e, &to, amount);
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage()
            .instance()
            .get(&AdvKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        Self::_move(&e, &from, &to, amount);
    }

    pub fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128) {
        let allowed = Self::allowance(e.clone(), from.clone(), spender.clone());
        if allowed < amount {
            panic!("adversarial token: insufficient allowance");
        }
        let remaining = allowed - amount;
        e.storage()
            .instance()
            .set(&AdvKey::Allowance(from.clone(), spender), &remaining);
        Self::_move(&e, &from, &to, amount);
    }

    pub fn approve(e: Env, from: Address, spender: Address, amount: i128, _expiration: u32) {
        let calls = Self::approve_calls(e.clone()) + 1;
        e.storage().instance().set(&AdvKey::ApproveCalls, &calls);
        e.storage()
            .instance()
            .set(&AdvKey::Allowance(from, spender), &amount);
    }

    pub fn allowance(e: Env, from: Address, spender: Address) -> i128 {
        e.storage()
            .instance()
            .get(&AdvKey::Allowance(from, spender))
            .unwrap_or(0)
    }

    fn _move(e: &Env, from: &Address, to: &Address, amount: i128) {
        let mode = e
            .storage()
            .instance()
            .get(&AdvKey::Mode)
            .unwrap_or(FeeMode::Clean);

        match mode {
            FeeMode::Clean => {
                Self::_debit(e, from, amount);
                Self::_credit(e, to, amount);
            }
            FeeMode::SenderPaysMore(fee) => {
                Self::_debit(e, from, amount + fee);
                Self::_credit(e, to, amount);
            }
            FeeMode::RecipientGetsLess(fee) => {
                Self::_debit(e, from, amount);
                Self::_credit(e, to, amount - fee);
            }
            FeeMode::RecipientGetsMore(fee) => {
                Self::_debit(e, from, amount);
                Self::_credit(e, to, amount + fee);
            }
            FeeMode::SenderGetsRefund(fee) => {
                let net = amount - fee;
                Self::_debit(e, from, net);
                Self::_credit(e, to, net);
            }
            FeeMode::SenderIsCredited(credit) => {
                Self::_credit(e, from, amount);
                Self::_credit(e, to, amount - credit);
            }
            FeeMode::RecipientIsDebited(debit) => {
                Self::_debit(e, from, amount);
                Self::_debit(e, to, debit);
            }
        }
    }

    fn _debit(e: &Env, who: &Address, amount: i128) {
        let balance = Self::balance(e.clone(), who.clone());
        // An explicit business check, not `checked_sub`: a balance of 100 minus
        // 500 is -400, which is perfectly representable in i128, so `checked_sub`
        // happily returns `Some(-400)` and the token would happily go negative.
        if balance < amount {
            panic!("adversarial token: insufficient balance");
        }
        e.storage()
            .instance()
            .set(&AdvKey::Balance(who.clone()), &(balance - amount));
    }

    fn _credit(e: &Env, who: &Address, amount: i128) {
        let balance = Self::balance(e.clone(), who.clone());
        let next = balance
            .checked_add(amount)
            .expect("adversarial token: balance overflow");
        e.storage()
            .instance()
            .set(&AdvKey::Balance(who.clone()), &next);
    }
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    env: Env,
    contract: Address,
    token: Address,
}

impl Fixture {
    /// A bond contract bound to the adversarial token in `Clean` mode.
    fn new(mode: FeeMode) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(crate::CredenceBond, ());
        let token = env.register(AdversarialToken, ());

        let admin = AdversarialTokenClient::new(&env, &token);
        admin.set_mode(&mode);

        let token_for_storage = token.clone();
        env.as_contract(&contract, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::BondToken, &token_for_storage);
        });

        Fixture {
            env,
            contract,
            token,
        }
    }

    fn client(&self) -> AdversarialTokenClient<'_> {
        AdversarialTokenClient::new(&self.env, &self.token)
    }

    fn balance(&self, who: &Address) -> i128 {
        self.client().balance(who)
    }

    fn mint(&self, to: &Address, amount: i128) {
        self.client().mint(to, &amount);
    }

    fn set_mode(&self, mode: FeeMode) {
        self.client().set_mode(&mode);
    }

    fn approve(&self, owner: &Address, spender: &Address, amount: i128) {
        self.client()
            .approve(owner, spender, &amount, &self.env.ledger().sequence());
    }

    fn as_contract<R>(&self, f: impl FnOnce() -> R) -> R {
        self.env.as_contract(&self.contract, f)
    }
}

fn panic_message<F: FnOnce()>(f: F) -> String {
    let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a panic");
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        panic!("panic payload was neither String nor &str");
    }
}

#[track_caller]
fn expect_panic_with<F: FnOnce()>(expected: &str, f: F) {
    let message = panic_message(f);
    assert_eq!(message, expected);
}

#[track_caller]
fn expect_panic_containing<F: FnOnce()>(needle: &str, f: F) {
    let message = panic_message(f);
    assert!(
        message.contains(needle),
        "expected panic containing {needle:?}, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// safe_transfer — sender-side detection
// ---------------------------------------------------------------------------

/// A token that over-debits the sender is rejected. The contract is always the
/// sender here, so this is the case the balance-delta guard exists for: the
/// bond contract must not lose more than it intended.
#[test]
fn safe_transfer_rejects_a_token_that_over_debits_the_sender() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
    });
}

/// A token that refunds part of the sender's debit is rejected too, so the guard
/// is not merely checking for "more than expected".
#[test]
fn safe_transfer_rejects_a_token_that_under_debits_the_sender() {
    let f = Fixture::new(FeeMode::SenderGetsRefund(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
    });
}

/// A token that *credits* the sender (mint-on-transfer / rebase) makes the
/// measured delta negative.
///
/// Worth being precise about why this surfaces as
/// `TRANSFER_AMOUNT_MISMATCH` rather than the `"balance underflow"` message
/// that sits on the same line: `checked_sub` guards i128 *arithmetic overflow*,
/// and `1000 - 1100 == -100` is perfectly representable, so `checked_sub`
/// returns `Some(-100)`. The negative delta then fails the equality check. The
/// `"balance underflow"` string is therefore effectively unreachable here — it
/// only fires if the delta exceeds `i128::MIN`, which requires balances at the
/// extremes of the range. Either way the transfer is rejected, which is what
/// this test pins.
#[test]
fn safe_transfer_rejects_a_token_that_credits_the_sender() {
    let f = Fixture::new(FeeMode::SenderIsCredited(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
    });
}

// ---------------------------------------------------------------------------
// safe_transfer — the blind spot
// ---------------------------------------------------------------------------

/// **Documented limitation, not an endorsement.** `safe_transfer` measures the
/// contract's own balance, so a token that debits the requested amount but
/// short-pays the *recipient* is accepted. The recipient ends up with less than
/// the amount the bond recorded as paid out.
///
/// This is a real risk surface for this contract, not a hypothetical: in a
/// withdrawal the recipient is the bond owner. The test pins the current
/// behaviour so that closing the gap is a visible, deliberate change.
#[test]
fn safe_transfer_accepts_a_recipient_shortfall() {
    let f = Fixture::new(FeeMode::RecipientGetsLess(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let before_recipient = f.balance(&recipient);
    f.as_contract(|| safe_transfer(&f.env, &recipient, 100));

    // Contract bookkeeping looks right...
    assert_eq!(f.balance(&f.contract), 900);
    // ...but the recipient was under-paid by the token's fee.
    assert_eq!(f.balance(&recipient), before_recipient + 75);
}

/// The mirror of the shortfall case: an over-crediting token is also accepted,
/// because the contract's own delta is unaffected.
#[test]
fn safe_transfer_accepts_a_recipient_overpayment() {
    let f = Fixture::new(FeeMode::RecipientGetsMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    f.as_contract(|| safe_transfer(&f.env, &recipient, 100));

    assert_eq!(f.balance(&f.contract), 900);
    assert_eq!(f.balance(&recipient), 125);
}

/// The most serious case: a token that debits the recipient instead of crediting
/// it passes every guard in `safe_transfer`, because the contract's balance
/// moved exactly as requested. A bond owner withdrawing funds could be charged
/// by the token with no signal from this module.
#[test]
fn safe_transfer_accepts_a_recipient_debit() {
    let f = Fixture::new(FeeMode::RecipientIsDebited(40));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);
    f.mint(&recipient, 500);

    f.as_contract(|| safe_transfer(&f.env, &recipient, 100));

    assert_eq!(f.balance(&f.contract), 900);
    assert_eq!(
        f.balance(&recipient),
        460,
        "recipient was charged 40 by the token instead of being paid 100"
    );
}

// ---------------------------------------------------------------------------
// safe_transfer_from — recipient-side detection
// ---------------------------------------------------------------------------

/// Here the contract is the recipient, so the guard covers exactly what must
/// never be corrupted: the contract's incoming custody balance. Each mode that
/// changes that balance is rejected.
#[test]
fn safe_transfer_from_rejects_recipient_side_discrepancies() {
    for mode in [
        FeeMode::RecipientGetsLess(25),
        FeeMode::RecipientGetsMore(25),
        FeeMode::SenderGetsRefund(25),
    ] {
        let f = Fixture::new(mode.clone());
        let owner = Address::generate(&f.env);
        f.mint(&owner, 1_000);
        f.approve(&owner, &f.contract, 100);

        expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
            f.as_contract(|| safe_transfer_from(&f.env, &owner, 100))
        });
    }
}

/// A token that debits the contract on the way in drives the incoming delta
/// negative, and the equality guard rejects it — the same arithmetic caveat as
/// `safe_transfer_rejects_a_token_that_credits_the_sender` applies in the other
/// direction.
#[test]
fn safe_transfer_from_rejects_a_token_that_debits_the_contract() {
    let f = Fixture::new(FeeMode::RecipientIsDebited(40));
    let owner = Address::generate(&f.env);
    f.mint(&owner, 1_000);
    f.mint(&f.contract, 500);
    f.approve(&owner, &f.contract, 100);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer_from(&f.env, &owner, 100))
    });
}

/// **Documented limitation, not an endorsement.** The mirror blind spot: the
/// owner is the sender here, so a token that over-debits the owner is accepted.
/// The contract's custody balance is intact, so this helper is behaving as
/// designed, but the user is charged more than they authorised.
#[test]
fn safe_transfer_from_accepts_a_sender_surcharge() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let owner = Address::generate(&f.env);
    f.mint(&owner, 1_000);
    f.approve(&owner, &f.contract, 100);

    let before_owner = f.balance(&owner);
    f.as_contract(|| safe_transfer_from(&f.env, &owner, 100));

    assert_eq!(f.balance(&f.contract), 100, "custody balance is correct");
    assert_eq!(f.balance(&owner), before_owner - 125, "owner paid the fee");
}

/// A mint-on-transfer token credits the owner the full requested amount, so the
/// owner pays nothing at all while the contract receives it in full. The
/// contract's books are right, so this is accepted — the entire loss lands on
/// the depositor, who receives full custody credit for nothing.
#[test]
fn safe_transfer_from_accepts_a_credited_sender() {
    let f = Fixture::new(FeeMode::SenderIsCredited(0));
    let owner = Address::generate(&f.env);
    f.mint(&owner, 1_000);
    f.approve(&owner, &f.contract, 100);

    let before_owner = f.balance(&owner);
    f.as_contract(|| safe_transfer_from(&f.env, &owner, 100));

    assert_eq!(f.balance(&f.contract), 100);
    assert_eq!(
        f.balance(&owner),
        before_owner + 100,
        "owner was credited the transfer amount instead of debited"
    );
}

/// The same mode with a non-zero credit *under*-credits the contract, which the
/// recipient-side guard does catch. This is the boundary between the two tests
/// above: it is not "is the sender misbehaving" that decides, it is "did the
/// contract's own balance move by exactly the requested amount".
#[test]
fn safe_transfer_from_rejects_a_credited_sender_that_under_credits() {
    let f = Fixture::new(FeeMode::SenderIsCredited(5));
    let owner = Address::generate(&f.env);
    f.mint(&owner, 1_000);
    f.approve(&owner, &f.contract, 100);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer_from(&f.env, &owner, 100))
    });
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// A failed transfer leaves both balances untouched, and a later transfer of an
/// affordable amount succeeds normally. A guard that corrupted state on failure
/// would break the second half of this test.
#[test]
fn a_failed_transfer_leaves_balances_intact_and_a_retry_succeeds() {
    let f = Fixture::new(FeeMode::Clean);
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 100);

    let before = (f.balance(&f.contract), f.balance(&recipient));

    expect_panic_with(errors::TRANSFER_FAILED, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 500))
    });

    assert_eq!(f.balance(&f.contract), before.0);
    assert_eq!(f.balance(&recipient), before.1);

    // Contract is funded again and the retry goes through cleanly.
    f.mint(&f.contract, 400);
    f.as_contract(|| safe_transfer(&f.env, &recipient, 500));

    assert_eq!(f.balance(&f.contract), 0);
    assert_eq!(f.balance(&recipient), 500);
}

/// Switching the configured token away from a misbehaving one restores normal
/// behaviour. This is the operational recovery path: quarantine the token,
/// repoint `BondToken`, and transfers work again.
#[test]
fn repointing_at_a_conforming_token_recovers_transfers() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
    });

    // A second, well-behaved token becomes the configured one.
    let good = f.env.register(AdversarialToken, ());
    let good_client = AdversarialTokenClient::new(&f.env, &good);
    good_client.set_mode(&FeeMode::Clean);
    good_client.mint(&f.contract, &1_000i128);

    let good_for_storage = good.clone();
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .instance()
            .set(&crate::DataKey::BondToken, &good_for_storage);
    });

    f.as_contract(|| safe_transfer(&f.env, &recipient, 100));

    assert_eq!(f.balance(&recipient), 100);
}

/// A rejected fee-on-transfer token stays rejected across repeated attempts, so
/// a misconfiguration cannot intermittently let money through.
///
/// Note the balance assertion. In production the guard's panic reverts the whole
/// transaction, so nothing would move. In this harness `catch_unwind` catches a
/// bare Rust panic with no transaction boundary, so the *token's* writes — which
/// happened in a separate, already-completed contract frame — persist. The test
/// therefore asserts the verdict is stable and that the contract lost exactly
/// what the token actually moved, not that nothing moved at all. See the
/// "harness caveat" section in the module docs.
#[test]
fn rejection_of_a_fee_token_is_stable_across_retries() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 100_000);
    // SenderPaysMore debits the contract 100 + 25 = 125 per attempt.
    let moved_per_attempt = 125i128;

    for attempt in 0..5 {
        expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
            f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
        });
        assert_eq!(
            f.balance(&f.contract),
            100_000 - moved_per_attempt * (attempt as i128 + 1),
            "attempt {attempt} moved an unexpected amount"
        );
        assert_eq!(
            f.balance(&recipient),
            100 * (attempt as i128 + 1),
            "the token credited the recipient the requested amount"
        );
    }
}

/// A guard-level rejection is not atomic in this harness, but it *is* atomic in
/// the sense that matters for `atomic_transfer_and_update`: the state update
/// never runs. That is the property under test below — the accompanying balance
/// expectation accounts for the token's already-applied movement.
#[test]
fn atomic_update_is_not_run_even_though_the_token_already_moved() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let ran = Cell::new(false);
    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, 100, || ran.set(true)))
    });

    assert!(!ran.get());
    assert_eq!(f.balance(&f.contract), 875, "token debited 125 in-frame");
    assert_eq!(f.balance(&recipient), 100);
}

// ---------------------------------------------------------------------------
// atomic_transfer_and_update
// ---------------------------------------------------------------------------

/// The success path: the transfer happens and the state update runs.
#[test]
fn atomic_update_runs_after_a_successful_transfer() {
    let f = Fixture::new(FeeMode::Clean);
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let ran = Cell::new(false);
    f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, 400, || ran.set(true)));

    assert!(ran.get(), "state update did not run");
    assert_eq!(f.balance(&f.contract), 600);
    assert_eq!(f.balance(&recipient), 400);
}

/// The core guarantee: when the transfer is rejected, the state update never
/// runs. Without this, a rejected fee-on-transfer token would leave the bond's
/// records crediting an amount that never moved.
#[test]
fn atomic_update_is_skipped_when_the_transfer_is_rejected() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let ran = Cell::new(false);
    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, 100, || ran.set(true)))
    });

    assert!(!ran.get(), "state update ran despite a rejected transfer");
}

/// A zero amount skips the token entirely and runs the update. This is the
/// "record a zero-value event" path, and it works even with no token configured.
#[test]
fn atomic_update_runs_on_zero_without_a_configured_token() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(crate::CredenceBond, ());
    let other = Address::generate(&env);

    let ran = Cell::new(false);
    env.as_contract(&contract, || {
        atomic_transfer_and_update(&env, &other, 0, || ran.set(true))
    });

    assert!(ran.get(), "zero-amount state update did not run");
}

/// A negative amount is rejected before the update runs, matching every other
/// entry point.
#[test]
fn atomic_update_rejects_negative_amounts_without_running() {
    let f = Fixture::new(FeeMode::Clean);
    let recipient = Address::generate(&f.env);

    let ran = Cell::new(false);
    expect_panic_with(errors::INVALID_AMOUNT, || {
        f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, -1, || ran.set(true)))
    });

    assert!(!ran.get());
}

/// After a failed atomic transfer the next attempt succeeds and applies its
/// update exactly once — the no-half-finished-updates contract holds across
/// attempts, not just within one.
#[test]
fn atomic_update_recovers_after_a_failed_attempt() {
    let f = Fixture::new(FeeMode::SenderPaysMore(25));
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let failed_runs = Cell::new(0);
    expect_panic_with(errors::TRANSFER_AMOUNT_MISMATCH, || {
        f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, 100, || failed_runs.set(1)))
    });
    assert_eq!(failed_runs.get(), 0);

    // Quarantine the bad token behaviour, retry. The first attempt already
    // moved 125 out of the contract (see the harness caveat in the module
    // docs), so the balance is 875 rather than 1000 going in.
    f.set_mode(FeeMode::Clean);
    f.as_contract(|| atomic_transfer_and_update(&f.env, &recipient, 100, || failed_runs.set(2)));

    assert_eq!(failed_runs.get(), 2, "retry should apply its update");
    assert_eq!(f.balance(&f.contract), 775);
    assert_eq!(f.balance(&recipient), 200);
}

// ---------------------------------------------------------------------------
// Allowance mutation
// ---------------------------------------------------------------------------

/// `safe_increase_allowance` accumulates onto the current value, and a zero
/// increment is a no-op that does not even call the token.
#[test]
fn increase_allowance_accumulates_and_ignores_zero() {
    let f = Fixture::new(FeeMode::Clean);
    let spender = Address::generate(&f.env);

    f.as_contract(|| safe_approve(&f.env, &spender, 1_000));
    assert_eq!(f.client().allowance(&f.contract, &spender), 1_000);

    let calls_before = f.client().approve_calls();
    f.as_contract(|| safe_increase_allowance(&f.env, &spender, 0));
    assert_eq!(
        f.client().approve_calls(),
        calls_before,
        "zero increment must not call the token"
    );

    f.as_contract(|| safe_increase_allowance(&f.env, &spender, 500));
    assert_eq!(f.client().allowance(&f.contract, &spender), 1_500);
}

/// The accumulation is a `checked_add`, so an allowance already at `i128::MAX`
/// panics instead of wrapping into a negative allowance. A silent wrap here
/// would turn "unlimited" into "revoked" or, worse, into a large positive
/// number the owner never approved.
#[test]
fn increase_allowance_panics_on_i128_overflow() {
    let f = Fixture::new(FeeMode::Clean);
    let spender = Address::generate(&f.env);

    f.as_contract(|| safe_approve(&f.env, &spender, i128::MAX));
    assert_eq!(f.client().allowance(&f.contract, &spender), i128::MAX);

    expect_panic_containing("allowance overflow", || {
        f.as_contract(|| safe_increase_allowance(&f.env, &spender, 1))
    });
}

/// `force_approve` resets to zero before setting the new value, so it *replaces*
/// an existing allowance rather than adding to it — which is the entire point of
/// defending against the approve front-running race. The token call count proves
/// the reset-then-set sequence rather than a single overwrite.
#[test]
fn force_approve_replaces_rather_than_accumulates() {
    let f = Fixture::new(FeeMode::Clean);
    let spender = Address::generate(&f.env);

    f.as_contract(|| safe_approve(&f.env, &spender, 5_000));
    let calls_before = f.client().approve_calls();

    f.as_contract(|| force_approve(&f.env, &spender, 1_000));

    assert_eq!(
        f.client().allowance(&f.contract, &spender),
        1_000,
        "force_approve must replace, not accumulate"
    );
    assert_eq!(
        f.client().approve_calls() - calls_before,
        2,
        "force_approve should call approve twice (reset then set)"
    );
}

/// `force_approve` to zero clears an allowance entirely — the revoke path.
#[test]
fn force_approve_zero_clears_an_existing_allowance() {
    let f = Fixture::new(FeeMode::Clean);
    let spender = Address::generate(&f.env);

    f.as_contract(|| safe_approve(&f.env, &spender, 5_000));
    f.as_contract(|| force_approve(&f.env, &spender, 0));

    assert_eq!(f.client().allowance(&f.contract, &spender), 0);
}

/// `safe_approve` computes `sequence() + 10000` with no overflow guard. Near
/// `u32::MAX` the addition wraps in a release/wasm build, yielding a small
/// expiration ledger — i.e. an allowance that is already expired the moment it
/// is written, which would silently break a withdrawal rather than raise.
///
/// This cannot be exercised through the harness. Jumping the ledger sequence to
/// within 10_000 of `u32::MAX` advances it by roughly 4.2 billion entries, which
/// makes every previously written contract instance read as archived. The
/// failure arrives first as `HostError: Error(Storage, InternalError)` —
/// "Accessed contract instance key that has been archived" — and the addition is
/// never reached. Confirmed by running it. Left ignored rather than deleted, in
/// the same spirit as the `#[ignore]` in `test_batch.rs:468`, so the gap stays
/// visible. Closing it needs either an SDK affordance for driving ledger
/// sequence without instance TTL, or a source-level fix to use `saturating_add`.
#[test]
#[ignore = "Ledger sequence near u32::MAX archives contract instances before the overflow is reached"]
fn safe_approve_expiration_overflow_near_u32_max() {
    let f = Fixture::new(FeeMode::Clean);
    let spender = Address::generate(&f.env);

    let mut info = f.env.ledger().get();
    info.sequence_number = u32::MAX - 5_000;
    f.env.ledger().set(info);

    expect_panic_containing("overflow", || {
        f.as_contract(|| safe_approve(&f.env, &spender, 1_000))
    });
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

/// Every rejection above should surface as a panic, never as a silently
/// successful transfer. This walks the full fee-mode matrix and records which
/// modes are rejected on each path, so a change in coverage is visible as a
/// diff rather than as a missing test.
#[test]
fn the_fee_mode_matrix_is_recorded_in_full() {
    let modes = [
        ("clean", FeeMode::Clean, false, false),
        ("sender_pays_more", FeeMode::SenderPaysMore(5), true, false),
        (
            "recipient_gets_less",
            FeeMode::RecipientGetsLess(5),
            false,
            true,
        ),
        (
            "recipient_gets_more",
            FeeMode::RecipientGetsMore(5),
            false,
            true,
        ),
        (
            "sender_gets_refund",
            FeeMode::SenderGetsRefund(5),
            true,
            true,
        ),
        (
            "sender_is_credited",
            FeeMode::SenderIsCredited(5),
            true,
            true,
        ),
        (
            "recipient_is_debited",
            FeeMode::RecipientIsDebited(5),
            false,
            true,
        ),
    ];

    let mut out_of_rejected = Vec::new();
    let mut transfer_from_rejected = Vec::new();

    for (name, mode, out_expected_reject, from_expected_reject) in modes {
        let f = Fixture::new(mode);
        let recipient = Address::generate(&f.env);
        f.mint(&f.contract, 10_000);
        f.mint(&recipient, 1_000);

        let out_rejected = catch_unwind(AssertUnwindSafe(|| {
            f.as_contract(|| safe_transfer(&f.env, &recipient, 100))
        }))
        .is_err();
        if out_rejected {
            out_of_rejected.push(name);
        }
        assert_eq!(
            out_rejected, out_expected_reject,
            "safe_transfer verdict changed for {name}"
        );

        let owner = Address::generate(&f.env);
        f.mint(&owner, 10_000);
        f.approve(&owner, &f.contract, 100);

        let from_rejected = catch_unwind(AssertUnwindSafe(|| {
            f.as_contract(|| safe_transfer_from(&f.env, &owner, 100))
        }))
        .is_err();
        if from_rejected {
            transfer_from_rejected.push(name);
        }
        assert_eq!(
            from_rejected, from_expected_reject,
            "safe_transfer_from verdict changed for {name}"
        );
    }

    assert_eq!(
        out_of_rejected,
        vec![
            "sender_pays_more",
            "sender_gets_refund",
            "sender_is_credited"
        ],
        "safe_transfer only rejects sender-side behaviour"
    );
    assert_eq!(
        transfer_from_rejected,
        vec![
            "recipient_gets_less",
            "recipient_gets_more",
            "sender_gets_refund",
            "sender_is_credited",
            "recipient_is_debited"
        ],
        "safe_transfer_from only rejects recipient-side behaviour"
    );
}
