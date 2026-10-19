//! `ChaosToken` — Deterministic failure-injection mock for the SEP-41 token interface.
//!
//! Each failure toggle can be set independently so tests can craft compound scenarios
//! (e.g., balance reads succeed but transfers fail).  The contract stores toggle flags
//! in instance storage, which means they survive across calls within the same ledger.
//!
//! ## Available injection points
//!
//! | Method | Toggle key | Threat modelled |
//! |--------|-----------|-----------------|
//! | `transfer` | `"ft"` | Token contract reverts on send |
//! | `transfer_from` | `"ftf"` | Allowance-based transfer reverts |
//! | `balance` | `"fb"` | Storage read returns unexpected `None` |
//! | `approve` | `"fa"` | Allowance-write fails |
//! | `allowance` | `"fal"` | Allowance-read fails |
//!
//! Plus a hostile-token mode ([`set_reentry_attack`](ChaosToken::set_reentry_attack))
//! that re-enters the bond from inside a transfer.
//!
//! ## Invariants
//!
//! This mock is the substrate every fault-injection suite trusts, so a silent
//! arithmetic surprise here would make an unrelated test look like a protocol
//! bug. The following are therefore enforced rather than left to the caller:
//!
//! * **Conservation** — a successful `transfer` moves `amount` from `from` to
//!   `to` and changes nothing else. Total across all holders is invariant.
//! * **No silent wrap** — balances are updated with checked arithmetic, and a
//!   debit larger than the sender's balance is rejected outright. `checked_sub`
//!   on its own is *not* sufficient: `1 - 2` is a representable `i128`, so an
//!   over-spend would otherwise leave the sender holding a negative balance
//!   rather than reverting. An over-credit is likewise rejected instead of
//!   wrapping to a huge negative at `i128::MAX`.
//! * **Non-negative amounts** — negative `amount` is rejected. A negative
//!   transfer would otherwise move value *backwards*, creating supply.
//! * **No self-transfer** — `from == to` is rejected. The two writes in
//!   `transfer` share one storage key, so a self-transfer would end at
//!   `balance + amount` and silently mint supply.
//! * **Deterministic one-shot injection** — [`maybe_reenter`](ChaosToken::maybe_reenter)
//!   clears `armed` *before* invoking the target, so an armed attack can fire at
//!   most once no matter how many transfers follow or how the target responds.
//! * **Safe defaults** — every read flag falls back to `false` and every
//!   balance falls back to [`DEFAULT_BALANCE`], so an un-`initialize`d instance
//!   still behaves like a healthy token rather than panicking on `unwrap`.

#![allow(dead_code)]
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};

// Toggle storage keys (short to stay within Symbol length limit).
const KEY_FAIL_TRANSFER: &str = "ft";
const KEY_FAIL_TRANSFER_FROM: &str = "ftf";
const KEY_FAIL_BALANCE: &str = "fb";
const KEY_FAIL_APPROVE: &str = "fa";
const KEY_FAIL_ALLOWANCE: &str = "fal";
const KEY_ATTACK_TARGET: &str = "target";
const KEY_ATTACK_METHOD: &str = "method";
const KEY_ATTACK_IDENTITY: &str = "identity";
const KEY_ATTACK_ADMIN: &str = "admin";
const KEY_ATTACK_AMOUNT: &str = "amount";
const KEY_ATTACK_ARMED: &str = "armed";
const KEY_ATTACK_ATTEMPTED: &str = "hit";
const KEY_ATTACK_REJECTED: &str = "reject";

/// Balance handed to an address that has never been written to instance
/// storage. Tests rely on this being non-zero so that a transfer into a fresh
/// account is observable without a preceding `mint`.
///
/// `pub(crate)` so `test_chaos_token_boundaries` can assert against it without
/// hard-coding the literal and drifting from the implementation.
pub(crate) const DEFAULT_BALANCE: i128 = 10_000_000_i128;

#[contract]
pub struct ChaosToken;

#[contractimpl]
impl ChaosToken {
    /// Initialize with all chaos flags disabled (safe defaults).
    ///
    /// This is a *full* reset, not an additive one: every failure toggle is
    /// cleared and the previously armed hostile-token payload — including its
    /// target, method, identities and amount — is removed from storage. A test
    /// that calls `initialize` between scenarios is therefore guaranteed to
    /// start from a disarmed token, and can never inherit a stale attack target
    /// from the scenario that ran before it.
    ///
    /// Idempotent: calling it twice leaves exactly the same state.
    pub fn initialize(e: Env) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_TRANSFER), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_TRANSFER_FROM), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_BALANCE), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_APPROVE), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_ALLOWANCE), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ARMED), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ATTEMPTED), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_REJECTED), &false);

        // Drop the payload itself. Clearing `armed` alone would leave the old
        // target/method/amount readable, so a later bug that re-armed the token
        // without repopulating them would `unwrap()` on `None`.
        for key in [
            KEY_ATTACK_TARGET,
            KEY_ATTACK_METHOD,
            KEY_ATTACK_IDENTITY,
            KEY_ATTACK_ADMIN,
            KEY_ATTACK_AMOUNT,
        ] {
            e.storage().instance().remove(&Symbol::new(&e, key));
        }
    }

    // ── Failure toggles ────────────────────────────────────────────────────────

    /// chaos: make every `transfer` call panic.
    ///
    /// Threat model: host-level resource exhaustion or a compromised token contract
    /// that reverts unexpectedly, leaving the caller in an indeterminate state unless
    /// the bond contract enforces atomic rollback.
    pub fn set_fail_transfer(e: Env, fail: bool) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_TRANSFER), &fail);
    }

    /// chaos: make every `transfer_from` call panic.
    ///
    /// Threat model: allowance-based transfer revert mid-execution; tests that the
    /// bond contract does not leave partial state after a pull-payment failure.
    pub fn set_fail_transfer_from(e: Env, fail: bool) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_TRANSFER_FROM), &fail);
    }

    /// chaos: make every `balance` read panic.
    ///
    /// Threat model: token storage key unexpectedly missing (e.g., ledger compaction
    /// or incorrect TTL management), causing `unwrap()` sites to crash.
    pub fn set_fail_balance(e: Env, fail: bool) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_BALANCE), &fail);
    }

    /// chaos: make every `approve` call panic.
    ///
    /// Threat model: host rejection of allowance writes; callers must not assume
    /// approval succeeded without verifying the return path.
    pub fn set_fail_approve(e: Env, fail: bool) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_APPROVE), &fail);
    }

    /// chaos: make every `allowance` read panic.
    ///
    /// Threat model: allowance storage key unexpectedly `None`; pull-transfer paths
    /// must handle this without corrupting the caller's state.
    pub fn set_fail_allowance(e: Env, fail: bool) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_FAIL_ALLOWANCE), &fail);
    }

    /// hostile-token mode: re-enter `target.method(...)` from the next token transfer.
    ///
    /// The injected call is made through `try_invoke_contract` so the malicious
    /// token can observe that the bond rejected re-entry and still allow the
    /// outer token transfer to complete. `method` must be one of:
    /// `withdraw`, `withdraw_early`, `slash`, `top_up`, or `collect_fees`.
    pub fn set_reentry_attack(
        e: Env,
        target: Address,
        method: Symbol,
        identity: Address,
        admin: Address,
        amount: i128,
    ) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_TARGET), &target);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_METHOD), &method);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_IDENTITY), &identity);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ADMIN), &admin);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_AMOUNT), &amount);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ARMED), &true);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ATTEMPTED), &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_REJECTED), &false);
    }

    pub fn attack_attempted(e: Env) -> bool {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_ATTEMPTED))
            .unwrap_or(false)
    }

    pub fn attack_rejected(e: Env) -> bool {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_REJECTED))
            .unwrap_or(false)
    }

    // ── SEP-41 token interface ─────────────────────────────────────────────────

    pub fn decimals(_e: Env) -> u32 {
        7
    }

    /// chaos injection point #3 — storage read failure.
    ///
    /// Falls back to [`DEFAULT_BALANCE`] for an address that has never been
    /// written, so a scenario that transfers into a fresh account still has a
    /// non-zero starting point to assert against.
    pub fn balance(e: Env, id: Address) -> i128 {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_BALANCE))
            .unwrap_or(false)
        {
            panic!("chaos: balance storage read failed");
        }
        e.storage()
            .instance()
            .get::<_, i128>(&id)
            .unwrap_or(DEFAULT_BALANCE)
    }

    /// chaos injection point #1 — token transfer revert.
    ///
    /// Order matters and is asserted by the boundary suite:
    /// 1. `fail_transfer` toggle (host-level token revert),
    /// 2. amount / self-transfer validation,
    /// 3. hostile-token re-entry injection,
    /// 4. checked balance mutation.
    ///
    /// A failure at any step reverts the whole call, so a reverted transfer
    /// never leaves a partial write behind.
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_TRANSFER))
            .unwrap_or(false)
        {
            panic!("chaos: transfer panicked");
        }
        Self::require_transferable(&from, &to, amount);
        Self::maybe_reenter(e.clone());
        let from_bal = Self::balance(e.clone(), from.clone());
        let to_bal = Self::balance(e.clone(), to.clone());
        // `checked_sub` alone is not enough: `1 - 2` is a representable `i128`
        // (it just goes negative), so an over-spend would silently hand the
        // sender a negative balance and every later assertion would read
        // garbage. Compare against the balance explicitly.
        if amount > from_bal {
            panic!("chaos: transfer exceeds balance");
        }
        let new_from = from_bal - amount;
        let new_to = to_bal
            .checked_add(amount)
            .unwrap_or_else(|| panic!("chaos: transfer overflows recipient balance"));
        e.storage().instance().set(&from, &new_from);
        e.storage().instance().set(&to, &new_to);
    }

    /// chaos injection point #2 — allowance-based transfer revert.
    ///
    /// The `fail_transfer_from` toggle is checked first so this injection point
    /// stays distinguishable from the `fail_transfer` one it delegates to.
    pub fn transfer_from(e: Env, _spender: Address, from: Address, to: Address, amount: i128) {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_TRANSFER_FROM))
            .unwrap_or(false)
        {
            panic!("chaos: transfer_from panicked");
        }
        Self::transfer(e, from, to, amount);
    }

    /// chaos injection point #5 — allowance read failure.
    ///
    /// Always reports unlimited allowance when healthy, so an allowance-gated
    /// call path only has to model the *revert* branch, not a partial spend.
    pub fn allowance(e: Env, _from: Address, _spender: Address) -> i128 {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_ALLOWANCE))
            .unwrap_or(false)
        {
            panic!("chaos: allowance read failed");
        }
        i128::MAX
    }

    /// chaos injection point #4 — approve write failure.
    pub fn approve(e: Env, _from: Address, _spender: Address, _amount: i128, _expiration: u32) {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_APPROVE))
            .unwrap_or(false)
        {
            panic!("chaos: approve panicked");
        }
    }

    /// Increase `to`'s balance by `amount`.
    ///
    /// Checked: `amount` must be non-negative and the resulting balance must
    /// fit in an `i128`. Without the check, `mint(i128::MAX)` on an already
    /// funded account wraps to a negative balance under this crate's release
    /// profile and every later assertion in the scenario reads garbage.
    ///
    /// A zero-value mint is accepted as a no-op so callers can drive
    /// "mint exactly the amount I am going to transfer" without branching.
    pub fn mint(e: Env, to: Address, amount: i128) {
        if amount < 0 {
            panic!("chaos: mint amount must be non-negative");
        }
        let current = Self::balance(e.clone(), to.clone());
        let next = current
            .checked_add(amount)
            .unwrap_or_else(|| panic!("chaos: mint overflows balance"));
        e.storage().instance().set(&to, &next);
    }

    /// Shared pre-conditions for every value-moving entrypoint.
    ///
    /// Rejecting these here — rather than letting them wrap — is what keeps a
    /// misuse in a test from being reported as a protocol-level accounting bug.
    fn require_transferable(from: &Address, to: &Address, amount: i128) {
        if amount < 0 {
            panic!("chaos: transfer amount must be non-negative");
        }
        if from == to {
            // Both writes share one key, so `from_bal - amount` is immediately
            // overwritten by `to_bal + amount` and the account gains `amount`.
            panic!("chaos: self-transfer is not a supported scenario");
        }
    }

    fn maybe_reenter(e: Env) {
        let armed_key = Symbol::new(&e, KEY_ATTACK_ARMED);
        let armed = e.storage().instance().get(&armed_key).unwrap_or(false);
        if !armed {
            return;
        }

        e.storage().instance().set(&armed_key, &false);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_ATTEMPTED), &true);

        let target: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_TARGET))
            .unwrap();
        let method: Symbol = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_METHOD))
            .unwrap();
        let identity: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_IDENTITY))
            .unwrap();
        let admin: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_ADMIN))
            .unwrap();
        let amount: i128 = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, KEY_ATTACK_AMOUNT))
            .unwrap_or(1);

        let args = if method == Symbol::new(&e, "withdraw")
            || method == Symbol::new(&e, "withdraw_early")
            || method == Symbol::new(&e, "top_up")
        {
            Vec::from_array(&e, [identity.into_val(&e), amount.into_val(&e)])
        } else if method == Symbol::new(&e, "slash") {
            Vec::from_array(&e, [admin.into_val(&e), amount.into_val(&e)])
        } else if method == Symbol::new(&e, "collect_fees") {
            Vec::from_array(&e, [admin.into_val(&e)])
        } else {
            panic!("unsupported hostile-token method");
        };

        let rejected = !matches!(
            e.try_invoke_contract::<Val, soroban_sdk::Error>(&target, &method, args),
            Ok(Ok(_))
        );
        e.storage()
            .instance()
            .set(&Symbol::new(&e, KEY_ATTACK_REJECTED), &rejected);
    }
}
