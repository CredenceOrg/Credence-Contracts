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

#![allow(dead_code)]
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

// Toggle storage keys (short to stay within Symbol length limit).
const KEY_FAIL_TRANSFER: &str = "ft";
const KEY_FAIL_TRANSFER_FROM: &str = "ftf";
const KEY_FAIL_BALANCE: &str = "fb";
const KEY_FAIL_APPROVE: &str = "fa";
const KEY_FAIL_ALLOWANCE: &str = "fal";

#[contract]
pub struct ChaosToken;

#[contractimpl]
impl ChaosToken {
    /// Initialize with all chaos flags disabled (safe defaults).
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

    // ── SEP-41 token interface ─────────────────────────────────────────────────

    pub fn decimals(_e: Env) -> u32 {
        7
    }

    /// chaos injection point #3 — storage read failure.
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
            .unwrap_or(10_000_000_i128)
    }

    /// chaos injection point #1 — token transfer revert.
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        if e.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&e, KEY_FAIL_TRANSFER))
            .unwrap_or(false)
        {
            panic!("chaos: transfer panicked");
        }
        let from_bal = Self::balance(e.clone(), from.clone());
        let to_bal = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&from, &(from_bal - amount));
        e.storage().instance().set(&to, &(to_bal + amount));
    }

    /// chaos injection point #2 — allowance-based transfer revert.
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

    pub fn mint(e: Env, to: Address, amount: i128) {
        let current = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&to, &(current + amount));
    }
}
