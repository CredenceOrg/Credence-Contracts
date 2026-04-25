//! Fixed-Duration Bond Contract
//!
//! Allows any address to lock USDC for an exact, predetermined time period.
//! After the period elapses the owner may withdraw their full principal.
//! Early withdrawal is permitted but incurs a configurable penalty.
//!
//! ## Key design decisions
//!
//! - **One active bond per owner**: avoids complex multi-bond accounting.
//! - **Checks-Effects-Interactions**: storage is updated *before* token transfers.
//! - **Overflow-safe expiry**: `bond_start.checked_add(duration)` panics on overflow.
//! - **Auth-gated mutations**: `owner.require_auth()` on create/withdraw.
//! - **Admin-only admin ops**: fee config, penalty config, fee collection.

#![no_std]

mod errors;
mod types;
mod validation;

use credence_math::{add_i128, mul_i128, split_bps};
use errors::*;
use types::{DataKey, FeeConfig, FixedBond, OracleSafety};
use validation::validate_recipient;

pub mod pausable;

use soroban_sdk::{contract, contractimpl, token::TokenClient, Address, Env, Symbol};

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_pausable;

/// Maximum fee in basis points (1000 = 10%).
pub const MAX_FEE_BPS: u32 = 1000;
/// Default staleness window for oracle answers (seconds) when no per-asset
/// override is configured. Default to 1 hour.
pub const DEFAULT_MAX_STALENESS: u64 = 3600;
/// Minimum allowed fixed-duration bond lock period (seconds).
pub const MIN_BOND_DURATION_SECS: u64 = 1;
/// Maximum allowed fixed-duration bond lock period (seconds).
/// Bound to one year to avoid unreasonably long locks.
pub const MAX_BOND_DURATION_SECS: u64 = 365 * 86_400;

// ─── Helpers ───────────────────────────────────────────────────────────────

fn require_admin(e: &Env, caller: &Address) {
    caller.require_auth();
    let stored: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("{}", ERR_NOT_INITIALIZED));
    if stored != *caller {
        panic!("{}", ERR_UNAUTHORIZED);
    }
}

fn get_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic!("{}", ERR_TOKEN_NOT_SET))
}

/// Apply basis-point fee: returns `(fee, net)`.
///
/// Uses checked arithmetic throughout:
/// - `amount * bps` uses `checked_mul` to catch intermediate overflow
///   (e.g. very large deposits with non-zero bps).
/// - The net `amount - fee` uses `checked_sub` even though the result is
///   mathematically non-negative, so any future change to call sites cannot
///   silently wrap.
fn apply_bps(amount: i128, bps: u32) -> (i128, i128) {
    split_bps(
        amount,
        bps,
        ERR_FEE_MUL_OVERFLOW,
        "fee bps division overflow",
        "fee net underflow",
    )
}

/// Verify balance-delta for token transfer to reject fee-on-transfer tokens.
/// Call before performing transfer_from to verify received amount.
///
/// # Panics
/// If balance increased by less than expected amount after transfer_from.
fn verify_transfer_in(token_client: &TokenClient, contract: &Address, expected_amount: i128) {
    let balance_before = token_client.balance(contract);
    let balance_after = token_client.balance(contract);
    let actual_received = balance_after
        .checked_sub(balance_before)
        .expect(ERR_FEE_MUL_OVERFLOW);

    if actual_received != expected_amount {
        panic!("{}", ERR_UNSUPPORTED_TOKEN);
    }
}

/// Verify balance-delta for token transfer to reject fee-on-transfer tokens.
/// Call after performing transfer to verify sent amount.
///
/// # Panics
/// If balance decreased by less than expected amount after transfer.
fn verify_transfer_out(
    token_client: &TokenClient,
    contract: &Address,
    balance_before: i128,
    expected_amount: i128,
) {
    let balance_after = token_client.balance(contract);
    let actual_sent = balance_before
        .checked_sub(balance_after)
        .expect(ERR_FEE_MUL_OVERFLOW);

    if actual_sent != expected_amount {
        panic!("{}", ERR_UNSUPPORTED_TOKEN);
    }
}

#[inline]
fn validate_oracle_answer(answer: i128, safety: &OracleSafety) {
    if answer <= 0 {
        panic!("{}", ERR_ORACLE_ANSWER_NON_POSITIVE);
    }
    if answer < safety.min_answer || answer > safety.max_answer {
        panic!("{}", ERR_ORACLE_ANSWER_OUT_OF_RANGE);
    }
}

/// Minimal oracle freshness/round validator as required by issue #125.
/// Keeps checks deliberately small and deterministic per instructions.
fn get_max_staleness(e: &Env, asset: &Address) -> u64 {
    e.storage()
        .instance()
        .get::<_, u64>(&DataKey::OracleStaleness(asset.clone()))
        .unwrap_or(DEFAULT_MAX_STALENESS)
}

/// Validate receiver allowlist: panics if the allowlist is enabled and the
/// recipient is not allowed. Default allowlist state is disabled.
fn validate_receiver_allowed(e: &Env, recipient: &Address) {
    let enabled: bool = e
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::ReceiverAllowlistEnabled)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let allowed: bool = e
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::ReceiverAllowlist(recipient.clone()))
        .unwrap_or(false);
    if !allowed {
        panic!("{}", ERR_UNAUTHORIZED_RECEIVER);
    }
}

#[inline]
fn validate_oracle(
    answer: i128,
    updated_at: u64,
    round_id: u64,
    answered_in_round: u64,
    max_staleness: u64,
    now: u64,
) {
    if answer <= 0 {
        panic!("{}", ERR_ORACLE_INVALID_ANSWER);
    }
    if answered_in_round < round_id {
        panic!("{}", ERR_ORACLE_INCOMPLETE_ROUND);
    }
    if now.checked_sub(updated_at).unwrap_or(u64::MAX) > max_staleness {
        panic!("{}", ERR_ORACLE_STALE);
    }
}

// ─── Contract ──────────────────────────────────────────────────────────────

#[contract]
pub struct FixedDurationBond;

#[contractimpl]
impl FixedDurationBond {
    // ── Admin setup ────────────────────────────────────────────────────────

    /// One-time initialization. Stores `admin` and `token`.
    /// Panics if called again after initialization.
    pub fn initialize(e: Env, admin: Address, token: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic!("{}", ERR_ALREADY_INITIALIZED);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);
        e.storage().instance().set(&DataKey::Token, &token);
    }

    /// Set (or update) the optional bond-creation fee.
    /// `fee_bps` = 0 effectively disables the fee.
    pub fn set_fee_config(e: Env, admin: Address, treasury: Address, fee_bps: u32) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        if fee_bps > MAX_FEE_BPS {
            panic!("{}", ERR_FEE_BPS_TOO_HIGH);
        }

        let previous: Option<FeeConfig> = e.storage().instance().get(&DataKey::FeeConfig);
        let cfg = FeeConfig { treasury, fee_bps };
        e.storage().instance().set(&DataKey::FeeConfig, &cfg);

        let (old_treasury, old_fee_bps) = match previous {
            Some(prev) => (Some(prev.treasury), prev.fee_bps),
            None => (None, 0_u32),
        };

        e.events().publish(
            (Symbol::new(&e, "fee_config_updated"),),
            (old_treasury, old_fee_bps, cfg.treasury, cfg.fee_bps),
        );
    }

    /// Set the default early-exit penalty applied when `withdraw_early` is called.
    /// Pass 0 to disable early-exit withdrawal for newly created bonds.
    pub fn set_penalty_config(e: Env, admin: Address, base_penalty_bps: u32) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        e.storage()
            .instance()
            .set(&DataKey::PenaltyBps, &base_penalty_bps);
    }

    /// Set per-asset oracle safety bounds.
    ///
    /// Bounds are inclusive and must satisfy:
    /// - `min_answer` > 0
    /// - `max_answer` >= `min_answer`
    pub fn set_oracle_safety(
        e: Env,
        admin: Address,
        asset: Address,
        min_answer: i128,
        max_answer: i128,
    ) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        if min_answer <= 0 || max_answer < min_answer {
            panic!("{}", ERR_ORACLE_BOUNDS_INVALID);
        }
        let safety = OracleSafety {
            min_answer,
            max_answer,
        };
        e.storage()
            .instance()
            .set(&DataKey::OracleSafety(asset.clone()), &safety);
        e.events().publish(
            (Symbol::new(&e, "oracle_safety_set"), asset),
            (min_answer, max_answer),
        );
    }

    /// Enable or disable the receiver allowlist. Default is disabled.
    pub fn set_receiver_allowlist_enabled(e: Env, admin: Address, enabled: bool) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        e.storage()
            .instance()
            .set(&DataKey::ReceiverAllowlistEnabled, &enabled);
        e.events()
            .publish((Symbol::new(&e, "receiver_allowlist_toggled"),), (enabled,));
    }

    /// Allow a receiver address to receive protocol-controlled funds when the
    /// allowlist is enabled.
    pub fn allow_receiver(e: Env, admin: Address, receiver: Address) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        e.storage()
            .instance()
            .set(&DataKey::ReceiverAllowlist(receiver.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "receiver_allowed"),), (receiver,));
    }

    /// Revoke an allowed receiver.
    pub fn revoke_receiver(e: Env, admin: Address, receiver: Address) {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        e.storage()
            .instance()
            .set(&DataKey::ReceiverAllowlist(receiver.clone()), &false);
        e.events()
            .publish((Symbol::new(&e, "receiver_revoked"),), (receiver,));
    }

    /// Collect all accrued creation fees to the admin or treasury.
    /// Transfers the fee balance to `recipient` and resets the counter.
    pub fn collect_fees(e: Env, admin: Address, recipient: Address) -> i128 {
        pausable::require_not_paused(&e);
        require_admin(&e, &admin);
        let accrued: i128 = e
            .storage()
            .instance()
            .get(&DataKey::AccruedFees)
            .unwrap_or(0_i128);
        if accrued == 0 {
            panic!("{}", ERR_NO_FEES);
        }
        // CEI: clear state before transfer.
        e.storage().instance().set(&DataKey::AccruedFees, &0_i128);

        // Guard: validate recipient against allowlist if enabled (Phase 1: only collect_fees)
        validate_receiver_allowed(&e, &recipient);

        let token = get_token(&e);
        let contract = e.current_contract_address();

        // Validate recipient to prevent transfers to invalid addresses
        validate_recipient(&recipient, &contract);

        TokenClient::new(&e, &token).transfer(&contract, &recipient, &accrued);

        e.events().publish(
            (Symbol::new(&e, "fees_collected"),),
            (admin, recipient, accrued),
        );
        accrued
    }

    /// Converts `amount` of `asset` into quote value using an oracle answer.
    ///
    /// Reverts unless:
    /// - oracle safety is configured for this asset
    /// - answer is strictly positive
    /// - answer is within the configured min/max bounds
    pub fn quote_value(
        e: Env,
        asset: Address,
        amount: i128,
        oracle_answer: i128,
        updated_at: u64,
        round_id: u64,
        answered_in_round: u64,
    ) -> i128 {
        if amount <= 0 {
            panic!("{}", ERR_INVALID_AMOUNT);
        }
        let safety: OracleSafety = e
            .storage()
            .instance()
            .get(&DataKey::OracleSafety(asset.clone()))
            .unwrap_or_else(|| panic!("{}", ERR_ORACLE_SAFETY_NOT_SET));
        
        validate_oracle_answer(oracle_answer, &safety);
        let now = e.ledger().timestamp();
        let max_staleness = get_max_staleness(&e, &asset);
        validate_oracle(
            oracle_answer,
            updated_at,
            round_id,
            answered_in_round,
            max_staleness,
            now,
        );
        mul_i128(amount, oracle_answer, ERR_VALUATION_OVERFLOW)
    }

    // ── Bond lifecycle ─────────────────────────────────────────────────────

    /// Lock `amount` USDC for `duration_secs` seconds.
    ///
    /// Requirements:
    /// - `amount` > 0
    /// - `duration_secs` is within `[MIN_BOND_DURATION_SECS, MAX_BOND_DURATION_SECS]`
    /// - No currently active bond for `owner`
    /// - Caller has approved the contract to spend `amount`
    ///
    /// A creation fee (if configured) is deducted from `amount`; the remaining
    /// principal is stored as `FixedBond.amount`.
    pub fn create_bond(e: Env, owner: Address, amount: i128, duration_secs: u64) -> FixedBond {
        pausable::require_not_paused(&e);
        owner.require_auth();

        if amount <= 0 {
            panic!("{}", ERR_INVALID_AMOUNT);
        }
        if duration_secs == 0 {
            panic!("{}", ERR_INVALID_DURATION);
        }
        if !(MIN_BOND_DURATION_SECS..=MAX_BOND_DURATION_SECS).contains(&duration_secs) {
            panic!("{}", ERR_DURATION_OUT_OF_BOUNDS);
        }

        // Reject if owner already has an active bond.
        if let Some(existing) = e
            .storage()
            .persistent()
            .get::<_, FixedBond>(&DataKey::Bond(owner.clone()))
        {
            if existing.active {
                panic!("{}", ERR_BOND_ACTIVE);
            }
        }

        let bond_start = e.ledger().timestamp();
        let bond_expiry = bond_start
            .checked_add(duration_secs)
            .expect(ERR_DURATION_OVERFLOW);

        // Pull tokens in first (caller must have approved).
        let token = get_token(&e);
        let contract = e.current_contract_address();
        let token_client = TokenClient::new(&e, &token);

        // Check balance before transfer to detect fee-on-transfer tokens
        let balance_before = token_client.balance(&contract);

        token_client.transfer_from(&contract, &owner, &contract, &amount);

        // Verify balance increased by exactly the expected amount
        let balance_after = token_client.balance(&contract);
        let actual_received = balance_after
            .checked_sub(balance_before)
            .expect(ERR_FEE_MUL_OVERFLOW);

        if actual_received != amount {
            panic!("{}", ERR_UNSUPPORTED_TOKEN);
        }

        // Apply optional creation fee.
        let net_amount = if let Some(cfg) = e
            .storage()
            .instance()
            .get::<_, FeeConfig>(&DataKey::FeeConfig)
        {
            if cfg.fee_bps > 0 {
                let (fee, net) = apply_bps(amount, cfg.fee_bps);
                // Accumulate fee; treasury receives it at collect_fees.
                let prev_fees: i128 = e
                    .storage()
                    .instance()
                    .get(&DataKey::AccruedFees)
                    .unwrap_or(0);
                let new_fees = add_i128(prev_fees, fee, ERR_FEE_ACCRUE_OVERFLOW);
                e.storage().instance().set(&DataKey::AccruedFees, &new_fees);
                net
            } else {
                amount
            }
        } else {
            amount
        };

        // Read default penalty for early exits.
        let penalty_bps: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PenaltyBps)
            .unwrap_or(0);

        let bond = FixedBond {
            owner: owner.clone(),
            amount: net_amount,
            bond_start,
            bond_duration: duration_secs,
            bond_expiry,
            penalty_bps,
            active: true,
        };

        e.storage()
            .persistent()
            .set(&DataKey::Bond(owner.clone()), &bond);

        e.events().publish(
            (Symbol::new(&e, "bond_created"), owner),
            (net_amount, bond_expiry),
        );

        bond
    }

    /// Withdraw the full bonded amount after the lock period has elapsed.
    ///
    /// Panics if there is no active bond or the lock period has not yet elapsed.
    /// Deactivates the bond after successful transfer.
    pub fn withdraw(e: Env, owner: Address) -> FixedBond {
        pausable::require_not_paused(&e);
        owner.require_auth();

        let mut bond: FixedBond = e
            .storage()
            .persistent()
            .get(&DataKey::Bond(owner.clone()))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND));

        if !bond.active {
            panic!("{}", ERR_NO_BOND);
        }

        let now = e.ledger().timestamp();
        if now < bond.bond_expiry {
            panic!("{}", ERR_LOCK_PERIOD_NOT_ELAPSED);
        }

        // CEI: mark inactive before transfer.
        bond.active = false;
        e.storage()
            .persistent()
            .set(&DataKey::Bond(owner.clone()), &bond);

        let token = get_token(&e);
        let contract = e.current_contract_address();
        let token_client = TokenClient::new(&e, &token);

        // Check balance before transfer to detect fee-on-transfer tokens
        let balance_before = token_client.balance(&contract);

        token_client.transfer(&contract, &owner, &bond.amount);

        // Verify balance decreased by exactly the expected amount
        let balance_after = token_client.balance(&contract);
        let actual_sent = balance_before
            .checked_sub(balance_after)
            .expect(ERR_FEE_MUL_OVERFLOW);

        if actual_sent != bond.amount {
            panic!("{}", ERR_UNSUPPORTED_TOKEN);
        }

        e.events()
            .publish((Symbol::new(&e, "bond_withdrawn"), owner), bond.amount);

        bond
    }

    /// Withdraw before the lock period elapses, paying a penalty fee.
    ///
    /// Panics if:
    /// - No active bond exists for `owner`.
    /// - The bond has already matured (use `withdraw` instead).
    /// - `penalty_bps` is 0 (early exit not enabled for this bond).
    ///
    /// Net amount = `bond.amount - penalty`. Penalty goes to the configured
    /// treasury; if no fee config is set, the penalty is burned (not transferred).
    pub fn withdraw_early(e: Env, owner: Address) -> FixedBond {
        pausable::require_not_paused(&e);
        owner.require_auth();

        let mut bond: FixedBond = e
            .storage()
            .persistent()
            .get(&DataKey::Bond(owner.clone()))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND));

        if !bond.active {
            panic!("{}", ERR_NO_BOND);
        }

        let now = e.ledger().timestamp();
        if now >= bond.bond_expiry {
            panic!("bond has matured; use withdraw instead");
        }

        if bond.penalty_bps == 0 {
            panic!("{}", ERR_PENALTY_NOT_CONFIGURED);
        }

        let (penalty, net_amount) = apply_bps(bond.amount, bond.penalty_bps);

        // CEI: mark inactive before transfers.
        bond.active = false;
        e.storage()
            .persistent()
            .set(&DataKey::Bond(owner.clone()), &bond);

        let token = get_token(&e);
        let contract = e.current_contract_address();
        let token_client = TokenClient::new(&e, &token);

        // Return net amount to owner - verify balance delta to reject fee-on-transfer tokens
        let balance_before_net = token_client.balance(&contract);
        token_client.transfer(&contract, &owner, &net_amount);
        let balance_after_net = token_client.balance(&contract);
        let actual_net_sent = balance_before_net
            .checked_sub(balance_after_net)
            .expect(ERR_FEE_MUL_OVERFLOW);

        if actual_net_sent != net_amount {
            panic!("{}", ERR_UNSUPPORTED_TOKEN);
        }

        // Send penalty to treasury if configured - verify balance delta
        if penalty > 0 {
            if let Some(cfg) = e
                .storage()
                .instance()
                .get::<_, FeeConfig>(&DataKey::FeeConfig)
            {
                let balance_before_penalty = token_client.balance(&contract);
                token_client.transfer(&contract, &cfg.treasury, &penalty);
                let balance_after_penalty = token_client.balance(&contract);
                let actual_penalty_sent = balance_before_penalty
                    .checked_sub(balance_after_penalty)
                    .expect(ERR_FEE_MUL_OVERFLOW);

                if actual_penalty_sent != penalty {
                    panic!("{}", ERR_UNSUPPORTED_TOKEN);
                }
            }
        }

        e.events().publish(
            (Symbol::new(&e, "bond_early_exit"), owner),
            (net_amount, penalty),
        );

        bond
    }

    // ── Queries ────────────────────────────────────────────────────────────

    /// Returns the bond state for `owner`.
    /// Panics if no bond record exists.
    pub fn get_bond(e: Env, owner: Address) -> FixedBond {
        e.storage()
            .persistent()
            .get(&DataKey::Bond(owner))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND))
    }

    /// Returns `true` if the bond's lock period has elapsed.
    pub fn is_matured(e: Env, owner: Address) -> bool {
        let bond: FixedBond = e
            .storage()
            .persistent()
            .get(&DataKey::Bond(owner))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND));
        e.ledger().timestamp() >= bond.bond_expiry
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        pausable::is_paused(&e)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        pausable::execute_pause_proposal(&e, proposal_id)
    }

    /// Returns the number of seconds remaining until maturity.
    /// Returns 0 if already matured.
    pub fn get_time_remaining(e: Env, owner: Address) -> u64 {
        let bond: FixedBond = e
            .storage()
            .persistent()
            .get(&DataKey::Bond(owner))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND));
        let now = e.ledger().timestamp();
        bond.bond_expiry.saturating_sub(now)
    }
}
