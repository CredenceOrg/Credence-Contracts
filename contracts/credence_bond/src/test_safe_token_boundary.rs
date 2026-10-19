//! Boundary-case coverage for `safe_token.rs` (issue #1346).
//!
//! `safe_token` is the lowest layer of every token movement in the bond
//! contract: withdrawals, protocol fees, treasury transfers and top-ups all
//! route through it. Its failure mode is asymmetric — a panic reverts the
//! transaction, but a *wrong-but-successful* balance change silently moves the
//! wrong amount of user money. These tests pin the exact numeric edges where
//! that could happen.
//!
//! ## What the existing coverage missed
//!
//! `safe_token.rs` carries two inline tests, one of which is a no-op
//! (`test_zero_address_validation` only constructs an address and asserts
//! nothing). A 382-line `safe_token_tests.rs` also exists in this directory but
//! is **never declared as a module**, so it has never been compiled or run. It
//! pointed every case at a token address with no contract behind it, so each
//! assertion degenerated to "it panicked" and could not distinguish a correct
//! rejection from an unrelated host error. Several of its assertions accepted
//! `err.contains("HostError")`, which any missing-contract panic satisfies.
//!
//! ## Invariants pinned here
//!
//! - **P1 — negative amounts are rejected before any state is touched.**
//!   `validate_amount` runs first in every entry point, so a negative amount
//!   panics with a stable message and never reaches the token contract.
//! - **P2 — a zero amount is a true no-op.** `safe_transfer`,
//!   `safe_transfer_from`, `safe_require_allowance` and
//!   `safe_increase_allowance` all return before reading storage, so they
//!   succeed even on a contract with no token configured. `safe_approve` and
//!   `force_approve` deliberately do *not* do this; see P3.
//! - **P3 — `safe_approve` has no zero fast path.** `safe_approve(.., 0)` still
//!   reads the token and calls `approve`. Pinned so the asymmetry is visible.
//! - **P4 — allowance comparison is inclusive.** `allowance == amount` passes,
//!   `allowance == amount - 1` panics. An off-by-one here would let a spender
//!   drain one unit more than authorised.
//! - **P5 — an exact-amount transfer moves exactly `amount`.** Verified
//!   against a real SEP-41 token, with balances read before and after.
//! - **P6 — the accepted amount domain is `[0, i128::MAX]`.** `i128::MIN` is
//!   rejected; the upper bound must not be rejected by validation itself, and
//!   the check must not overflow while testing the bound.
//! - **P7 — `get_token` returns the configured address and panics with a
//!   stable, diagnosable message otherwise.
//! - **P8 — the `errors` constants are the user-visible contract surface and
//!   must not drift or collide.
//!
//! Adversarial token behaviour (fee-on-transfer, sender/recipient taxation,
//! balance-underflow) and failure recovery live in `test_safe_token_recovery.rs`.
//!
//! ## Test-harness notes (verified against soroban-sdk 22.0.11)
//!
//! Three harness facts shape this file, each confirmed by running a probe
//! against this crate rather than assumed:
//!
//! 1. The crate is `#![no_std]`, so the `std` prelude is not in scope. `Box`,
//!    `String`, `ToString` and the `format!`/`println!` macros all need an
//!    explicit `use std::…` even inside a `#[cfg(test)]` module.
//! 2. `Env::as_contract` requires a **registered contract** address. A
//!    `Address::generate(&env)` account address fails with
//!    `HostError: Error(Storage, MissingValue)`, so the fixture registers the
//!    real `CredenceBond` contract.
//! 3. `catch_unwind` around an `Env`-using closure works on this SDK and the
//!    `Env` is fully reusable afterwards — balance reads and further transfers
//!    succeed, and a second `catch_unwind` still catches. (The unrelated
//!    `#[ignore]` in `test_batch.rs:468`, documented in
//!    `docs/known-simplifications.md` § 5.1, is not reproduced here.)
//!
//! Panic payloads are heterogeneous: `panic!("{}", msg)` yields a `String`,
//! while a literal `panic!("msg")` yields a `&'static str`. `panic_message`
//! below handles both.

#![allow(clippy::disallowed_macros)]
extern crate std;

use crate::safe_token::{
    errors, force_approve, get_token, safe_approve, safe_increase_allowance,
    safe_require_allowance, safe_transfer, safe_transfer_from, token_client,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::{String, ToString};

/// A registered bond contract plus a real SEP-41 token bound to it.
///
/// Using the built-in Stellar Asset Contract (rather than the hand-rolled
/// `test_helpers::MockStellarAsset`, whose `allowance` is hard-coded to
/// `i128::MAX` and whose `approve` is a no-op) is what makes the allowance
/// boundaries in P4 meaningful: a mock that always returns `i128::MAX` can
/// never demonstrate an inclusive comparison.
struct Fixture {
    env: Env,
    contract: Address,
    token: Address,
}

impl Fixture {
    /// A bond contract with `DataKey::BondToken` pointing at a real token.
    fn new() -> Self {
        let f = Self::unconfigured();
        let token = f
            .env
            .register_stellar_asset_contract_v2(Address::generate(&f.env))
            .address();
        let token_for_storage = token.clone();
        f.env.as_contract(&f.contract, || {
            f.env
                .storage()
                .instance()
                .set(&crate::DataKey::BondToken, &token_for_storage);
        });
        Fixture { token, ..f }
    }

    /// A registered bond contract with **no** token configured. Every helper
    /// that reads storage must fail on this fixture.
    fn unconfigured() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        // A registered contract is required: `as_contract` unwraps the host
        // frame, which has no instance to enter for a bare account address.
        let contract = env.register(crate::CredenceBond, ());
        let token = Address::generate(&env);
        Fixture {
            env,
            contract,
            token,
        }
    }

    fn client(&self) -> TokenClient<'_> {
        TokenClient::new(&self.env, &self.token)
    }

    fn balance(&self, who: &Address) -> i128 {
        self.client().balance(who)
    }

    fn mint(&self, to: &Address, amount: i128) {
        StellarAssetClient::new(&self.env, &self.token).mint(to, &amount);
    }

    /// Run `f` with the fixture contract as the executing contract, so that
    /// `e.current_contract_address()` resolves to the funded address.
    fn as_contract<R>(&self, f: impl FnOnce() -> R) -> R {
        self.env.as_contract(&self.contract, f)
    }

    /// `approve` from `owner` to `spender`, using a far-future expiry.
    fn approve(&self, owner: &Address, spender: &Address, amount: i128) {
        let expiration = self.env.ledger().sequence().saturating_add(100_000);
        self.client().approve(owner, spender, &amount, &expiration);
    }

    fn allowance(&self, owner: &Address, spender: &Address) -> i128 {
        self.client().allowance(owner, spender)
    }
}

/// Capture the payload of an expected panic as a `String`.
///
/// `safe_token` mixes `panic!("{}", msg)` (a `String` payload) with
/// `token_integration::get_token`'s literal `panic!("msg")` (a `&'static str`
/// payload), so both shapes are handled.
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

/// Assert that `f` panics with exactly `expected`.
#[track_caller]
fn expect_panic_with<F: FnOnce()>(expected: &str, f: F) {
    let message = panic_message(f);
    assert_eq!(message, expected);
}

// ---------------------------------------------------------------------------
// P1 / P6 — amount validation
// ---------------------------------------------------------------------------

/// P1 + P6: the accepted amount domain is exactly `[0, i128::MAX]`.
///
/// Zero short-circuits, so it must not panic at all. Every larger accepted
/// amount reaches the token call and fails there for want of balance — the
/// point is that it fails as a *transfer* failure, never as an *amount*
/// failure, which proves `validate_amount` accepted the bound without
/// overflowing while checking it.
#[test]
fn amount_domain_boundaries() {
    for accepted in [0i128, 1, 2, i128::MAX - 1, i128::MAX] {
        let f = Fixture::new();
        let recipient = Address::generate(&f.env);

        let verdict = catch_unwind(AssertUnwindSafe(|| {
            f.as_contract(|| safe_transfer(&f.env, &recipient, accepted))
        }));

        if accepted == 0 {
            assert!(
                verdict.is_ok(),
                "zero amount must short-circuit, but it panicked"
            );
        } else {
            let message = verdict
                .err()
                .map(|p| {
                    p.downcast_ref::<String>().cloned().unwrap_or_else(|| {
                        p.downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                    })
                })
                .expect("a non-zero amount should have reached the token call");
            assert_eq!(
                message,
                errors::TRANSFER_FAILED,
                "amount {accepted} must pass validation and fail at the token, \
                 not during validation"
            );
        }
    }
}

/// P1: every negative amount is rejected with the documented message by every
/// entry point, on a contract that *does* have a token configured. `i128::MIN`
/// is called out because it is the input most likely to overflow a naive bounds
/// check.
#[test]
fn negative_amounts_are_rejected_at_every_entry_point() {
    let f = Fixture::new();
    let other = Address::generate(&f.env);

    for negative in [i128::MIN, -1_000_000, -2, -1] {
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_transfer(&f.env, &other, negative))
        });
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_transfer_from(&f.env, &other, negative))
        });
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_require_allowance(&f.env, &other, negative))
        });
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_approve(&f.env, &other, negative))
        });
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_increase_allowance(&f.env, &other, negative))
        });
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| force_approve(&f.env, &other, negative))
        });
    }
}

/// P1: validation happens *before* the token is looked up, so a negative amount
/// is rejected identically on a contract with no token configured. Without this
/// ordering a caller bug would be reported as a deployment problem (and vice
/// versa), which makes production incidents misdiagnosable.
#[test]
fn negative_amount_is_rejected_before_token_lookup() {
    let f = Fixture::unconfigured();
    let other = Address::generate(&f.env);

    let message = panic_message(|| f.as_contract(|| safe_transfer(&f.env, &other, -1)));

    assert_eq!(message, errors::INVALID_AMOUNT);
    assert_ne!(
        message, "token not configured - contract not properly initialized",
        "validation must run before the token lookup"
    );
}

// ---------------------------------------------------------------------------
// P2 / P3 — zero-amount handling
// ---------------------------------------------------------------------------

/// P2: a zero amount is a true no-op that never reads storage, so it succeeds
/// even with no token configured at all. This is what makes "transfer 0" a safe
/// call for a caller that has not yet initialised.
#[test]
fn zero_amount_is_a_no_op_without_any_token_configured() {
    let f = Fixture::unconfigured();
    let other = Address::generate(&f.env);

    f.as_contract(|| {
        safe_transfer(&f.env, &other, 0);
        safe_transfer_from(&f.env, &other, 0);
        safe_require_allowance(&f.env, &other, 0);
        safe_increase_allowance(&f.env, &other, 0);
    });
}

/// P2: the zero fast path is genuinely reached — a non-zero amount on the same
/// unconfigured contract *does* panic. Guards against the previous test passing
/// for the wrong reason (e.g. the whole body silently being skipped).
#[test]
fn non_zero_amount_on_the_same_unconfigured_contract_does_panic() {
    let f = Fixture::unconfigured();
    let other = Address::generate(&f.env);

    expect_panic_with(
        "token not configured - contract not properly initialized",
        || f.as_contract(|| safe_require_allowance(&f.env, &other, 1)),
    );
}

/// P3: `safe_approve` has **no** zero fast path, unlike the other four entry
/// points. `safe_approve(spender, 0)` is a real token call and therefore
/// requires a configured token. Pinned so the asymmetry cannot be removed by
/// accident, and so callers are not surprised by it.
#[test]
fn safe_approve_zero_still_requires_a_configured_token() {
    let f = Fixture::unconfigured();
    let spender = Address::generate(&f.env);

    let message = panic_message(|| f.as_contract(|| safe_approve(&f.env, &spender, 0)));

    assert!(
        message.contains("token not configured"),
        "safe_approve(0) unexpectedly short-circuited; got: {message}"
    );
}

/// P3: `force_approve(0)` is two `safe_approve` calls, so it also requires a
/// configured token.
#[test]
fn force_approve_zero_still_requires_a_configured_token() {
    let f = Fixture::unconfigured();
    let spender = Address::generate(&f.env);

    let message = panic_message(|| f.as_contract(|| force_approve(&f.env, &spender, 0)));

    assert!(
        message.contains("token not configured"),
        "force_approve(0) unexpectedly short-circuited; got: {message}"
    );
}

/// P2: a zero-amount transfer against a fully configured contract moves no
/// balances. Pins that the early return means "amount 0 does nothing", not
/// merely "amount 0 is allowed".
#[test]
fn zero_amount_transfer_moves_no_balances() {
    let f = Fixture::new();
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 1_000);

    let before = (f.balance(&f.contract), f.balance(&recipient));

    f.as_contract(|| safe_transfer(&f.env, &recipient, 0));

    assert_eq!(f.balance(&f.contract), before.0);
    assert_eq!(f.balance(&recipient), before.1);
}

// ---------------------------------------------------------------------------
// P4 — allowance boundaries (inclusive comparison)
// ---------------------------------------------------------------------------

/// P4: the check is `allowance < amount` → reject, so `allowance == amount`
/// must pass. An off-by-one either way is a direct authorisation bug: too
/// strict blocks a legitimate withdrawal, too loose lets a spender move one
/// unit more than the owner authorised.
#[test]
fn allowance_comparison_is_inclusive_at_the_boundary() {
    let amount = 1_000i128;

    // Exactly enough: must pass.
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    f.approve(&owner, &f.contract, amount);
    assert_eq!(f.allowance(&owner, &f.contract), amount);
    f.as_contract(|| safe_require_allowance(&f.env, &owner, amount));

    // One unit short: must fail, with the documented message.
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    f.approve(&owner, &f.contract, amount - 1);
    expect_panic_with(errors::INSUFFICIENT_ALLOWANCE, || {
        f.as_contract(|| safe_require_allowance(&f.env, &owner, amount))
    });

    // One unit over: must pass.
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    f.approve(&owner, &f.contract, amount + 1);
    f.as_contract(|| safe_require_allowance(&f.env, &owner, amount + 1));
}

/// P4: with no approval the allowance is 0, so any positive requirement is
/// rejected. Also pins that the default is a real 0 and not an
/// "unlimited" sentinel — which is exactly what the crate's own
/// `MockStellarAsset` would have hidden.
#[test]
fn allowance_defaults_to_zero_and_rejects_positive_amounts() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);

    assert_eq!(f.allowance(&owner, &f.contract), 0);

    expect_panic_with(errors::INSUFFICIENT_ALLOWANCE, || {
        f.as_contract(|| safe_require_allowance(&f.env, &owner, 1))
    });
}

/// P4: allowance 1 is the smallest value that satisfies a 1-unit requirement,
/// and 0 is the smallest that fails it.
#[test]
fn smallest_allowance_boundaries() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    f.approve(&owner, &f.contract, 1);

    f.as_contract(|| safe_require_allowance(&f.env, &owner, 1));

    expect_panic_with(errors::INSUFFICIENT_ALLOWANCE, || {
        f.as_contract(|| safe_require_allowance(&f.env, &owner, 2))
    });
}

/// P4: allowance is scoped to the owner. An approval from a *different* owner
/// must not satisfy a check for this one, or one user's approval would
/// authorise spending another user's tokens.
#[test]
fn allowance_is_scoped_to_the_owning_address() {
    let f = Fixture::new();
    let approved = Address::generate(&f.env);
    let other = Address::generate(&f.env);
    f.approve(&approved, &f.contract, 1_000);

    assert_eq!(f.allowance(&approved, &f.contract), 1_000);
    assert_eq!(f.allowance(&other, &f.contract), 0);

    expect_panic_with(errors::INSUFFICIENT_ALLOWANCE, || {
        f.as_contract(|| safe_require_allowance(&f.env, &other, 1_000))
    });
}

/// P4: allowance is scoped to the spender too. The contract approving itself
/// must not authorise a third party.
#[test]
fn allowance_is_scoped_to_the_spender() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    let third_party = Address::generate(&f.env);
    f.approve(&owner, &f.contract, 1_000);

    assert_eq!(f.allowance(&owner, &f.contract), 1_000);
    assert_eq!(f.allowance(&owner, &third_party), 0);
}

// ---------------------------------------------------------------------------
// P5 — exact-amount movement
// ---------------------------------------------------------------------------

/// P5: a successful transfer moves exactly `amount` out of the contract and
/// exactly `amount` into the recipient. This is the core accounting invariant
/// of the module, checked against a real SEP-41 token rather than a stub.
#[test]
fn successful_transfer_moves_exactly_the_requested_amount() {
    let f = Fixture::new();
    let recipient = Address::generate(&f.env);
    let amount = 12_345i128;
    f.mint(&f.contract, 100_000);

    let before = (f.balance(&f.contract), f.balance(&recipient));

    f.as_contract(|| safe_transfer(&f.env, &recipient, amount));

    assert_eq!(f.balance(&f.contract), before.0 - amount);
    assert_eq!(f.balance(&recipient), before.1 + amount);
}

/// P5: transferring the contract's entire balance is the upper boundary and
/// must not trip the balance-delta guard, since the delta is exactly `amount`.
#[test]
fn transferring_the_entire_balance_is_accepted() {
    let f = Fixture::new();
    let recipient = Address::generate(&f.env);
    let total = 777i128;
    f.mint(&f.contract, total);

    f.as_contract(|| safe_transfer(&f.env, &recipient, total));

    assert_eq!(f.balance(&f.contract), 0);
    assert_eq!(f.balance(&recipient), total);
}

/// P5: the smallest non-zero transfer is the lower boundary of the real token
/// path and must move exactly one unit.
#[test]
fn smallest_positive_transfer_moves_exactly_one_unit() {
    let f = Fixture::new();
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 10);

    f.as_contract(|| safe_transfer(&f.env, &recipient, 1));

    assert_eq!(f.balance(&f.contract), 9);
    assert_eq!(f.balance(&recipient), 1);
}

/// P5: `safe_transfer_from` moves exactly `amount` from the owner into the
/// contract when the allowance covers it.
#[test]
fn transfer_from_moves_exactly_the_requested_amount() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    let amount = 4_200i128;
    f.mint(&owner, 50_000);
    f.approve(&owner, &f.contract, amount);

    let before = (f.balance(&f.contract), f.balance(&owner));

    f.as_contract(|| safe_transfer_from(&f.env, &owner, amount));

    assert_eq!(f.balance(&f.contract), before.0 + amount);
    assert_eq!(f.balance(&owner), before.1 - amount);
}

/// P5: `safe_transfer_from` delegates enforcement to `try_transfer_from`, so an
/// allowance one unit short must fail even though `safe_require_allowance` is
/// never called on this path. Also asserts the failure is atomic: the contract
/// must not be credited even partially.
#[test]
fn transfer_from_enforces_allowance_without_a_pre_check() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    let amount = 1_000i128;
    f.mint(&owner, 50_000);
    f.approve(&owner, &f.contract, amount - 1);

    let before = f.balance(&f.contract);

    expect_panic_with(errors::TRANSFER_FAILED, || {
        f.as_contract(|| safe_transfer_from(&f.env, &owner, amount))
    });

    assert_eq!(f.balance(&f.contract), before, "partial credit occurred");
}

/// P5: an owner with ample balance but no allowance is rejected, confirming
/// that allowance — not balance — is the binding constraint on this path.
#[test]
fn transfer_from_requires_allowance_even_with_sufficient_balance() {
    let f = Fixture::new();
    let owner = Address::generate(&f.env);
    f.mint(&owner, 1_000_000);

    expect_panic_with(errors::TRANSFER_FAILED, || {
        f.as_contract(|| safe_transfer_from(&f.env, &owner, 1))
    });
}

/// P5: exceeding the contract's own balance is rejected, and nothing moves.
#[test]
fn transfer_beyond_contract_balance_is_rejected_without_partial_movement() {
    let f = Fixture::new();
    let recipient = Address::generate(&f.env);
    f.mint(&f.contract, 10);

    let before = (f.balance(&f.contract), f.balance(&recipient));

    expect_panic_with(errors::TRANSFER_FAILED, || {
        f.as_contract(|| safe_transfer(&f.env, &recipient, 11))
    });

    assert_eq!(f.balance(&f.contract), before.0);
    assert_eq!(f.balance(&recipient), before.1);
}

// ---------------------------------------------------------------------------
// P7 — token configuration
// ---------------------------------------------------------------------------

/// P7: `get_token` returns the configured address.
#[test]
fn get_token_returns_the_configured_address() {
    let f = Fixture::new();

    assert_eq!(f.as_contract(|| get_token(&f.env)), f.token);
}

/// P7: an unconfigured contract panics with a message that names the cause, so a
/// failed deployment is diagnosable from the error alone.
#[test]
fn get_token_panics_with_a_diagnosable_message_when_unconfigured() {
    let f = Fixture::unconfigured();

    let message = panic_message(|| {
        f.as_contract(|| {
            get_token(&f.env);
        })
    });

    assert!(
        message.contains("token not configured"),
        "error does not name the cause: {message}"
    );
}

/// P7: `token_client` is built from the same configured address, so a missing
/// configuration surfaces there too rather than as an opaque client error.
#[test]
fn token_client_requires_a_configured_token() {
    let f = Fixture::unconfigured();

    let message = panic_message(|| {
        f.as_contract(|| {
            let _ = token_client(&f.env);
        })
    });

    assert!(message.contains("token not configured"), "got: {message}");
}

/// P7: with a token configured, `token_client` targets exactly that token.
#[test]
fn token_client_targets_the_configured_token() {
    let f = Fixture::new();
    f.mint(&f.contract, 42);

    f.as_contract(|| {
        let client = token_client(&f.env);
        // The client is live: a balance read round-trips to the real token.
        assert_eq!(client.balance(&f.contract), 42);
    });
}

/// P7: swapping the configured token changes what every other helper reads, so
/// a stale `BondToken` cannot be masked by a second fixture.
#[test]
fn reconfiguring_the_token_changes_the_effective_token() {
    let f = Fixture::new();
    let replacement = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();

    assert_eq!(f.as_contract(|| get_token(&f.env)), f.token);

    let replacement_for_storage = replacement.clone();
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .instance()
            .set(&crate::DataKey::BondToken, &replacement_for_storage);
    });

    assert_eq!(f.as_contract(|| get_token(&f.env)), replacement);
}

// ---------------------------------------------------------------------------
// P8 — error surface
// ---------------------------------------------------------------------------

/// P8: the `errors` constants are the strings operators and integrators match
/// against. Pin them exactly so a refactor cannot silently change an error a
/// caller depends on.
#[test]
fn error_constants_are_stable() {
    assert_eq!(errors::INVALID_AMOUNT, "amount must be non-negative");
    assert_eq!(
        errors::INSUFFICIENT_ALLOWANCE,
        "insufficient token allowance"
    );
    assert_eq!(errors::APPROVE_FAILED, "token approve failed");
    assert_eq!(errors::TRANSFER_FAILED, "token transfer failed");
    assert_eq!(errors::ALLOWANCE_FAILED, "token allowance check failed");
    assert_eq!(errors::ZERO_ADDRESS, "token address cannot be zero");
    assert_eq!(errors::TOKEN_NOT_SET, "token not configured");
    assert_eq!(
        errors::TRANSFER_AMOUNT_MISMATCH,
        "unsupported token: transfer amount mismatch (code 213)"
    );
}

/// P8: the error strings are mutually distinct, so a caller can tell a failed
/// transfer from a failed approve without matching on a substring that might
/// overlap.
#[test]
fn error_constants_are_mutually_distinct() {
    let all = [
        errors::INVALID_AMOUNT,
        errors::INSUFFICIENT_ALLOWANCE,
        errors::APPROVE_FAILED,
        errors::TRANSFER_FAILED,
        errors::ALLOWANCE_FAILED,
        errors::ZERO_ADDRESS,
        errors::TOKEN_NOT_SET,
        errors::TRANSFER_AMOUNT_MISMATCH,
    ];

    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "error constants {i} and {j} are identical");
            }
        }
    }
}

/// P8: `INVALID_AMOUNT` must not be a substring of any other error, otherwise the
/// "rejected the amount" path is indistinguishable from a token failure that
/// happens to mention an amount.
#[test]
fn invalid_amount_message_does_not_collide_with_other_errors() {
    assert!(!errors::TRANSFER_AMOUNT_MISMATCH.contains(errors::INVALID_AMOUNT));
    assert!(!errors::TRANSFER_FAILED.contains(errors::INVALID_AMOUNT));
    assert!(!errors::INSUFFICIENT_ALLOWANCE.contains(errors::INVALID_AMOUNT));
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

/// Boundary behaviour must be identical across repeated evaluation, so a flaky
/// result cannot be mistaken for a real finding.
#[test]
fn amount_validation_is_deterministic_across_repeats() {
    let f = Fixture::unconfigured();
    let other = Address::generate(&f.env);

    for _ in 0..25 {
        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_require_allowance(&f.env, &other, -1))
        });
    }
}

/// Two independently constructed fixtures with the same input reach the same
/// verdict — no dependence on address identity or storage ordering.
#[test]
fn verdicts_are_independent_of_address_identity() {
    for _ in 0..3 {
        let f = Fixture::unconfigured();
        let other = Address::generate(&f.env);

        expect_panic_with(errors::INVALID_AMOUNT, || {
            f.as_contract(|| safe_approve(&f.env, &other, -1))
        });
    }
}
