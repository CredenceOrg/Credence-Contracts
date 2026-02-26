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

use errors::*;
use types::{DataKey, FeeConfig, FixedBond};

use soroban_sdk::{contract, contractimpl, token::TokenClient, Address, Env, Symbol};

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod tests;

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
fn apply_bps(amount: i128, bps: u32) -> (i128, i128) {
    let fee = amount * (bps as i128) / 10_000_i128;
    (fee, amount - fee)
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
        e.storage().instance().set(&DataKey::Token, &token);
    }

    /// Set (or update) the optional bond-creation fee.
    /// `fee_bps` = 0 effectively disables the fee.
    pub fn set_fee_config(e: Env, admin: Address, treasury: Address, fee_bps: u32) {
        require_admin(&e, &admin);
        let cfg = FeeConfig { treasury, fee_bps };
        e.storage().instance().set(&DataKey::FeeConfig, &cfg);
    }

    /// Set the default early-exit penalty applied when `withdraw_early` is called.
    /// Pass 0 to disable early-exit withdrawal for newly created bonds.
    pub fn set_penalty_config(e: Env, admin: Address, base_penalty_bps: u32) {
        require_admin(&e, &admin);
        e.storage()
            .instance()
            .set(&DataKey::PenaltyBps, &base_penalty_bps);
    }

    /// Collect all accrued creation fees to the admin or treasury.
    /// Transfers the fee balance to `recipient` and resets the counter.
    pub fn collect_fees(e: Env, admin: Address, recipient: Address) -> i128 {
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

        let token = get_token(&e);
        let contract = e.current_contract_address();
        TokenClient::new(&e, &token).transfer(&contract, &recipient, &accrued);

        e.events().publish(
            (Symbol::new(&e, "fees_collected"),),
            (admin, recipient, accrued),
        );
        accrued
    }

    // ── Bond lifecycle ─────────────────────────────────────────────────────

    /// Lock `amount` USDC for `duration_secs` seconds.
    ///
    /// Requirements:
    /// - `amount` > 0
    /// - `duration_secs` > 0
    /// - No currently active bond for `owner`
    /// - Caller has approved the contract to spend `amount`
    ///
    /// A creation fee (if configured) is deducted from `amount`; the remaining
    /// principal is stored as `FixedBond.amount`.
    pub fn create_bond(e: Env, owner: Address, amount: i128, duration_secs: u64) -> FixedBond {
        owner.require_auth();

        if amount <= 0 {
            panic!("{}", ERR_INVALID_AMOUNT);
        }
        if duration_secs == 0 {
            panic!("{}", ERR_INVALID_DURATION);
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
        TokenClient::new(&e, &token).transfer_from(&contract, &owner, &contract, &amount);

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
                e.storage()
                    .instance()
                    .set(&DataKey::AccruedFees, &(prev_fees + fee));
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
        TokenClient::new(&e, &token).transfer(&contract, &owner, &bond.amount);

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

        // Return net amount to owner.
        token_client.transfer(&contract, &owner, &net_amount);

        // Send penalty to treasury if configured.
        if penalty > 0 {
            if let Some(cfg) = e
                .storage()
                .instance()
                .get::<_, FeeConfig>(&DataKey::FeeConfig)
            {
                token_client.transfer(&contract, &cfg.treasury, &penalty);
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

    /// Returns the number of seconds remaining until maturity.
    /// Returns 0 if already matured.
    pub fn get_time_remaining(e: Env, owner: Address) -> u64 {
        let bond: FixedBond = e
            .storage()
            .persistent()
            .get(&DataKey::Bond(owner))
            .unwrap_or_else(|| panic!("{}", ERR_NO_BOND));
        let now = e.ledger().timestamp();
        if now >= bond.bond_expiry {
            0_u64
        } else {
            bond.bond_expiry - now
        }
    }
}
