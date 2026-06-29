#![no_std]

mod early_exit_penalty;
mod nonce;
mod rolling_bond;
mod slashing;
mod tiered_bond;
mod weighted_attestation;

pub mod types;

#[cfg(test)]
mod test_unauthorized_token;

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal, String, Symbol,
    Val, Vec,
};

/// Identity tier based on bonded amount.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct IdentityBond {
    pub identity: Address,
    pub bonded_amount: i128,
    pub bond_start: u64,
    pub bond_duration: u64,
    pub slashed_amount: i128,
    pub active: bool,
    pub is_rolling: bool,
    pub withdrawal_requested_at: u64,
    pub notice_period_duration: u64,
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    pub fn initialize(e: Env, admin: Address) {
        if storage::get_admin(&e).is_some() {
            panic!("already initialized");
        }
        storage::set_admin(&e, &admin);
    }

    /// Set the set of accepted token addresses.
    /// Only callable by admin.
    pub fn set_accepted_tokens(e: Env, admin: Address, accepted_tokens: Vec<Address>) {
        admin.require_auth();
        if Some(admin) != storage::get_admin(&e) {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        storage::set_accepted_tokens(&e, &accepted_tokens);
    }

    /// Creates and persists a new bond for an identity.
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> Result<Bond, ContractError> {
        identity.require_auth();

        if storage::has_bond(&e, &identity) {
            return Err(ContractError::BondAlreadyExists);
        }

        let bond = validate_and_create_bond_struct(
            &e,
            identity.clone(),
            amount,
            duration,
            is_rolling,
            notice_period_duration,
        )?;

        // Safe token transfer in from the user
        safe_token::transfer_in(&e, &identity, amount);

        storage::set_bond(&e, &identity, &bond);
        events::emit_bond_created_v2(&e, &identity, amount, duration, is_rolling, e.ledger().timestamp());

        Ok(bond)
    }

    /// Increases the bonded amount for an existing bond.
    pub fn top_up(e: Env, identity: Address, amount: i128) -> Result<(), ContractError> {
        identity.require_auth();
        if !is_valid_bond(amount) {
            return Err(ContractError::InvalidBondAmount);
        }

        let mut bond = storage::get_bond(&e, &identity)?;
        
        safe_token::transfer_in(&e, &identity, amount);

        bond.amount = bond.amount.checked_add(amount)
            .ok_or(ContractError::Overflow)?;
        
        storage::set_bond(&e, &identity, &bond);
        events::emit_bond_increased_v2(&e, &identity, amount, bond.amount, e.ledger().timestamp());
        Ok(())
    }

    /// Extends the duration of an existing bond.
    pub fn extend_duration(e: Env, identity: Address, extra_duration: u64) -> Result<(), ContractError> {
        identity.require_auth();
        let mut bond = storage::get_bond(&e, &identity)?;
        
        bond.duration = bond.duration.checked_add(extra_duration)
            .ok_or(ContractError::Overflow)?;
            
        storage::set_bond(&e, &identity, &bond);
        events::emit_duration_extended_v2(&e, &identity, bond.duration, e.ledger().timestamp());
        Ok(())
    }

    pub fn request_withdrawal(e: Env, identity: Address) -> Result<(), ContractError> {
        identity.require_auth();
        let mut bond = storage::get_bond(&e, &identity)?;
        if !bond.is_rolling {
            return Err(ContractError::NotRollingBond);
        }
        if bond.withdrawal_requested_at != 0 {
            return Err(ContractError::WithdrawalAlreadyRequested);
        }
        bond.withdrawal_requested_at = e.ledger().timestamp();
        storage::set_bond(&e, &identity, &bond);
        Ok(())
    }

    pub fn withdraw(e: Env, identity: Address, amount: i128) -> Result<(), ContractError> {
        identity.require_auth();
        acquire_lock(&e);
        
        let mut bond = storage::get_bond(&e, &identity)?;
        let now = e.ledger().timestamp();

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 { panic!("notice not started"); }
            if now < bond.withdrawal_requested_at + bond.notice_period_duration {
                panic!("notice period not elapsed");
            }
        } else if now < bond.bond_start + bond.duration {
            return Err(ContractError::LockupNotExpired);
        }

        let available = bond.amount - bond.slashed_amount;
        if amount > available { return Err(ContractError::InsufficientBalance); }

        bond.amount = bond.amount.checked_sub(amount).ok_or(ContractError::Underflow)?;
        storage::set_bond(&e, &identity, &bond);
        
        safe_token::transfer_out(&e, &identity, amount);
        events::emit_withdrawal_v2(&e, &identity, amount, bond.amount, now);
        
        release_lock(&e);
        Ok(())
    }

    pub fn slash(e: Env, admin: Address, identity: Address, amount: i128) -> Result<(), ContractError> {
        admin.require_auth();
        if Some(admin) != storage::get_admin(&e) { return Err(ContractError::NotAdmin); }

        let mut bond = storage::get_bond(&e, &identity)?;
        let new_slashed = bond.slashed_amount.checked_add(amount).ok_or(ContractError::Overflow)?;
        
        bond.slashed_amount = if new_slashed > bond.amount { bond.amount } else { new_slashed };
        storage::set_bond(&e, &identity, &bond);
        
        events::emit_bond_slashed_v2(&e, &identity, amount, bond.slashed_amount, e.ledger().timestamp());
        Ok(())
    }
}

fn acquire_lock(e: &Env) {
    if storage::is_locked(e) { panic_with_error!(e, ContractError::ReentrancyDetected); }
    storage::set_lock(e, true);
}

fn release_lock(e: &Env) {
    storage::set_lock(e, false);
}

/// Internal validator for bond construction.
fn validate_and_create_bond_struct(
    e: &Env,
    identity: Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
    notice_period_duration: u64,
) -> Result<Bond, ContractError> {
    if !is_valid_bond(amount) {
        return Err(ContractError::InvalidBondAmount);
    }

    if duration == 0 {
        return Err(ContractError::InvalidBondDuration);
    }

    if is_rolling && (notice_period_duration == 0 || notice_period_duration > duration) {
        return Err(ContractError::InvalidNoticePeriod);
    }

    e.ledger().timestamp()
        .checked_add(duration)
        .ok_or(ContractError::Overflow)?;

    Ok(Bond {
        identity,
        amount,
        bond_start: e.ledger().timestamp(),
        duration,
        is_rolling,
        notice_period_duration,
        cooldown,
    })
}

// Re-export attestation type for external callers.
pub use types::Attestation;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Bond,
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
    SubjectAttestationCount(Address),
    Nonce(Address),
    AttesterStake(Address),
    WeightConfig,
    EarlyExitConfig,
    GraceWindow,
}

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}
