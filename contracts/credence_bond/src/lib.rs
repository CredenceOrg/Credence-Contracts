#![no_std]

mod early_exit_penalty;
mod migration;
mod nonce;
mod rolling_bond;
mod slashing;
mod tiered_bond;
mod weighted_attestation;

#[path = "types/mod.rs"]
pub mod types;

/// Reusable bond-invariant assertion library (test-only).
#[cfg(test)]
pub mod test_invariants;

/// Tests exercising the reusable bond-invariant library (test-only).
#[cfg(test)]
mod test_invariants_usage;

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

/// Maximum bond duration in seconds (365 days).
pub(crate) const MAX_BOND_DURATION_SECONDS: u64 = 31_536_000;
/// Soroban ledger TTLs are expressed in ledgers; assume a 5s ledger cadence.
const SECONDS_PER_LEDGER: u64 = 5;
/// Keep instance-storage entries alive for the full maximum bond duration.
pub(crate) const STORAGE_TTL_EXTEND_TO: u32 =
    (MAX_BOND_DURATION_SECONDS / SECONDS_PER_LEDGER) as u32;
/// Extend from the halfway point to the full configured lifetime.
pub(crate) const STORAGE_TTL_THRESHOLD: u32 = STORAGE_TTL_EXTEND_TO / 2;

pub(crate) fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_THRESHOLD, STORAGE_TTL_EXTEND_TO);
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    /// Initialize the contract with admin authority.
    ///
    /// Errors:
    /// - `ContractError::AlreadyInitialized` if called more than once.
    pub fn initialize(e: Env, admin: Address) {
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Configure early exit penalty parameters.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    pub fn set_early_exit_config(e: Env, admin: Address, treasury: Address, penalty_bps: u32) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        early_exit_penalty::set_config(&e, treasury, penalty_bps);
    }

    /// Register an authorized attester.
    pub fn register_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::Attester(attester.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), attester);
    }

    /// Remove an authorized attester.
    pub fn unregister_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Attester(attester.clone()));
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), attester);
    }

    /// Check whether an address is an authorized attester.
    pub fn is_attester(e: Env, attester: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Attester(attester))
            .unwrap_or(false)
    }

    /// Create a new bond for an identity.
    ///
    /// Authority: `identity` must authorize the call.
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
        identity.require_auth();
        let bond_start = e.ledger().timestamp();

        let _end_timestamp = bond_start
            .checked_add(duration)
            .expect("bond end timestamp would overflow");

        let bond = IdentityBond {
            identity: identity.clone(),
            bonded_amount: amount,
            bond_start,
            bond_duration: duration,
            slashed_amount: 0,
            active: true,
            is_rolling,
            withdrawal_requested_at: 0,
            notice_period_duration,
        };
        let key = DataKey::Bond;
        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        let tier = tiered_bond::get_tier_for_amount(amount);
        tiered_bond::emit_tier_change_if_needed(&e, &identity, BondTier::Bronze, tier);
        bond
    }

    /// Retrieve the current bond state.
    pub fn get_identity_state(e: Env) -> IdentityBond {
        // Ensure storage is migrated from v1 to v2 before accessing bond state
        migration::migrate_v1_to_v2(&e);
        let key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);
        bond
    }

    /// Add a weighted attestation for a subject.
    pub fn add_attestation(
        e: Env,
        attester: Address,
        subject: Address,
        attestation_data: String,
        nonce: u64,
    ) -> Attestation {
        attester.require_auth();

        let is_authorized = e
            .storage()
            .instance()
            .get(&DataKey::Attester(attester.clone()))
            .unwrap_or(false);
        if !is_authorized {
            panic_with_error!(e, ContractError::UnauthorizedAttester);
        }

        nonce::consume_nonce(&e, &attester, nonce);

        let dedup_key = types::AttestationDedupKey {
            verifier: attester.clone(),
            identity: subject.clone(),
            attestation_data: attestation_data.clone(),
        };
        if e.storage().instance().has(&dedup_key) {
            panic_with_error!(e, ContractError::DuplicateAttestation);
        }

        let counter_key = DataKey::AttestationCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        e.storage().instance().set(&counter_key, &next_id);

        let weight = weighted_attestation::compute_weight(&e, &attester);
        types::Attestation::validate_weight(weight);

        let attestation = types::Attestation {
            id,
            verifier: attester.clone(),
            identity: subject.clone(),
            timestamp: e.ledger().timestamp(),
            weight,
            attestation_data: attestation_data.clone(),
            revoked: false,
        };

        e.storage()
            .instance()
            .set(&DataKey::Attestation(id), &attestation);

        let subject_key = DataKey::SubjectAttestations(subject.clone());
        let mut attestations: Vec<u64> = e
            .storage()
            .instance()
            .get(&subject_key)
            .unwrap_or(Vec::new(&e));
        attestations.push_back(id);
        e.storage().instance().set(&subject_key, &attestations);
        bump_instance_ttl(&e);

        e.events().publish(
            (Symbol::new(&e, "attestation_added"), subject.clone()),
            (id, attester.clone(), attestation_data.clone()),
        );

        attestation
    }

    /// Withdraw from a bond after the relevant notice period.
    pub fn withdraw(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                panic!("withdrawal not requested");
            }
            let earliest = bond
                .withdrawal_requested_at
                .checked_add(bond.notice_period_duration)
                .expect("notice period overflow");
            if e.ledger().timestamp() < earliest {
                panic!("notice period not elapsed");
            }
        } else if e.ledger().timestamp() < bond.bond_start.saturating_add(bond.bond_duration) {
            panic_with_error!(e, ContractError::LockupNotExpired);
        }

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::SlashExceedsBond));
        if amount > available {
            panic_with_error!(e, ContractError::InsufficientBalance);
        }

        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
        if bond.slashed_amount > bond.bonded_amount {
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        bond
    }

    /// Withdraw early and apply the configured early exit penalty.
    pub fn withdraw_early(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::SlashExceedsBond));
        if amount > available {
            panic_with_error!(e, ContractError::InsufficientBalance);
        }

        let now = e.ledger().timestamp();
        let end = bond.bond_start.saturating_add(bond.bond_duration);
        if now >= end {
            panic_with_error!(e, ContractError::LockupNotExpired);
        }

        let (treasury, penalty_bps) = early_exit_penalty::get_config(&e);
        let remaining = end.saturating_sub(now);
        let penalty = early_exit_penalty::calculate_penalty(
            amount,
            remaining,
            bond.bond_duration,
            penalty_bps,
        );
        early_exit_penalty::emit_penalty_event(&e, &bond.identity, amount, penalty, &treasury);

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
        if bond.slashed_amount > bond.bonded_amount {
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        bond
    }

    /// Request withdrawal for a rolling bond.
    pub fn request_withdrawal(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);
        if !bond.is_rolling {
            panic_with_error!(e, ContractError::NotRollingBond);
        }
        if bond.withdrawal_requested_at != 0 {
            panic_with_error!(e, ContractError::WithdrawalAlreadyRequested);
        }
        bond.withdrawal_requested_at = e.ledger().timestamp();
        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        e.events().publish(
            (Symbol::new(&e, "withdrawal_requested"),),
            (bond.identity.clone(), bond.withdrawal_requested_at),
        );
        bond
    }

    /// Renew a rolling bond if the current period ended and withdrawal was not requested.
    pub fn renew_if_rolling(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if !bond.is_rolling {
            return bond;
        }
        if bond.withdrawal_requested_at != 0 {
            return bond;
        }
        let now = e.ledger().timestamp();
        if !rolling_bond::is_period_ended(now, bond.bond_start, bond.bond_duration) {
            return bond;
        }
        rolling_bond::apply_renewal(&mut bond, now);
        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        e.events().publish(
            (Symbol::new(&e, "bond_renewed"),),
            (bond.identity.clone(), bond.bond_start, bond.bond_duration),
        );
        bond
    }

    /// Get the current bond tier.
    pub fn get_tier(e: Env) -> BondTier {
        let bond = Self::get_identity_state(e);
        tiered_bond::get_tier_for_amount(bond.bonded_amount)
    }

    /// Slash a bond and return the updated bond state.
    pub fn slash(e: Env, admin: Address, amount: i128) -> IdentityBond {
        slashing::slash_bond(&e, &admin, amount)
    }

    /// Top up the bond amount.
    pub fn top_up(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

        bond.bonded_amount = bond
            .bonded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        bond
    }

    /// Extend the bond duration.
    pub fn extend_duration(e: Env, additional_duration: u64) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);

        bond.bond_duration = bond
            .bond_duration
            .checked_add(additional_duration)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        let _end_timestamp = bond
            .bond_start
            .checked_add(bond.bond_duration)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        bond
    }

    /// Deposit fees into the contract.
    pub fn deposit_fees(e: Env, amount: i128) {
        let key = Symbol::new(&e, "fees");
        let current: i128 = e.storage().instance().get(&key).unwrap_or(0);
        e.storage().instance().set(&key, &(current + amount));
    }

    /// Withdraw the full bonded amount with a reentrancy guard.
    pub fn withdraw_bond(e: Env, identity: Address) -> i128 {
        identity.require_auth();
        Self::acquire_lock(&e);

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);

        if bond.identity != identity {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        if !bond.active {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::BondNotActive);
        }

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                Self::release_lock(&e);
                panic!("withdrawal not requested");
            }
            let earliest = bond
                .withdrawal_requested_at
                .checked_add(bond.notice_period_duration)
                .expect("notice period overflow");
            if e.ledger().timestamp() < earliest {
                Self::release_lock(&e);
                panic!("notice period not elapsed");
            }
        }

        let withdraw_amount = bond.bonded_amount - bond.slashed_amount;

        let updated = IdentityBond {
            identity: identity.clone(),
            bonded_amount: 0,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: bond.slashed_amount,
            active: false,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);
        bump_instance_ttl(&e);

        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_withdraw");
            let args: Vec<Val> = Vec::from_array(&e, [withdraw_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        withdraw_amount
    }

    /// Slash a portion of the bond with a reentrancy guard.
    pub fn slash_bond(e: Env, admin: Address, slash_amount: i128) -> i128 {
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotAdmin);
        }

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

        if !bond.active {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::BondNotActive);
        }

        let new_slashed = bond.slashed_amount + slash_amount;
        if new_slashed > bond.bonded_amount {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }

        let updated = IdentityBond {
            identity: bond.identity.clone(),
            bonded_amount: bond.bonded_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: new_slashed,
            active: bond.active,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);
        bump_instance_ttl(&e);
        bump_instance_ttl(&e);

        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_slash");
            let args: Vec<Val> = Vec::from_array(&e, [slash_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        new_slashed
    }

    /// Collect protocol fees.
    pub fn collect_fees(e: Env, admin: Address) -> i128 {
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotAdmin);
        }

        let fee_key = Symbol::new(&e, "fees");
        let fees: i128 = e.storage().instance().get(&fee_key).unwrap_or(0);
        e.storage().instance().set(&fee_key, &0_i128);

        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_collect");
            let args: Vec<Val> = Vec::from_array(&e, [fees.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        fees
    }

    /// Register a callback contract for testing hooks.
    pub fn set_callback(e: Env, addr: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "callback"), &addr);
    }

    /// Check if the reentrancy lock is held.
    pub fn is_locked(e: Env) -> bool {
        Self::check_lock(&e)
    }

    fn acquire_lock(e: &Env) {
        let key = Symbol::new(e, "locked");
        let locked: bool = e.storage().instance().get(&key).unwrap_or(false);
        if locked {
            panic_with_error!(e, ContractError::ReentrancyDetected);
        }
        e.storage().instance().set(&key, &true);
    }

    fn release_lock(e: &Env) {
        let key = Symbol::new(e, "locked");
        e.storage().instance().set(&key, &false);
    }

    fn check_lock(e: &Env) -> bool {
        let key = Symbol::new(e, "locked");
        e.storage().instance().get(&key).unwrap_or(false)
    }

    #[allow(dead_code)]
    fn load_bond_and_require_owner_auth(e: &Env, key: &DataKey) -> IdentityBond {
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(key)
            .unwrap_or_else(|| panic!("no bond"));
        bond.identity.require_auth();
        bond
    }
}

// ---------------------------------------------------------------------------
// Pure Rust bond validation helpers
// ---------------------------------------------------------------------------

/// Represents a validated, created bond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bond {
    pub amount: i128,
    pub bond_start: u64,
    pub duration: u64,
    pub is_rolling: bool,
    pub notice_period_duration: u64,
}

/// Returns true when `amount` is a valid bond amount.
pub fn is_valid_bond(amount: i128) -> bool {
    amount > 0
}

/// Creates and returns a validated bond object.
pub fn create_bond(
    amount: i128,
    bond_start: u64,
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
    if is_rolling {
        if notice_period_duration == 0 {
            return Err(ContractError::InvalidNoticePeriod);
        }
        if notice_period_duration > duration {
            return Err(ContractError::InvalidNoticePeriod);
        }
    }
    bond_start
        .checked_add(duration)
        .ok_or(ContractError::Overflow)?;
    Ok(Bond {
        amount,
        bond_start,
        duration,
        is_rolling,
        notice_period_duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn is_valid_bond_positive_amount() {
        assert!(is_valid_bond(1));
        assert!(is_valid_bond(1_000_000));
        assert!(is_valid_bond(i128::MAX));
    }

    #[test]
    fn is_valid_bond_zero_is_invalid() {
        assert!(!is_valid_bond(0));
    }

    #[test]
    fn is_valid_bond_negative_is_invalid() {
        assert!(!is_valid_bond(-1));
        assert!(!is_valid_bond(-5));
        assert!(!is_valid_bond(i128::MIN));
    }

    #[test]
    fn create_bond_rejects_zero_amount() {
        let err = create_bond(0, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_negative_amount() {
        let err = create_bond(-1, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_large_negative_amount() {
        let err = create_bond(i128::MIN, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_zero_duration() {
        let err = create_bond(100, 0, 0, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }

    #[test]
    fn create_bond_rejects_zero_duration_rolling() {
        let err = create_bond(100, 0, 0, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }

    #[test]
    fn create_bond_rejects_zero_notice_for_rolling_bond() {
        let err = create_bond(100, 0, 3600, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_notice_greater_than_duration() {
        let err = create_bond(100, 0, 3600, true, 3601).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_notice_much_greater_than_duration() {
        let err = create_bond(100, 0, 100, true, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_overflow_on_bond_end() {
        let err = create_bond(100, u64::MAX, 1, false, 0).unwrap_err();
        assert_eq!(err, ContractError::Overflow);
    }

    #[test]
    fn create_bond_rejects_overflow_both_max() {
        let err = create_bond(100, u64::MAX, u64::MAX, false, 0).unwrap_err();
        assert_eq!(err, ContractError::Overflow);
    }

    #[test]
    fn create_bond_valid_non_rolling() {
        let bond = create_bond(100, 1000, 3600, false, 0).unwrap();
        assert_eq!(bond.amount, 100);
        assert_eq!(bond.bond_start, 1000);
        assert_eq!(bond.duration, 3600);
        assert!(!bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 0);
    }

    #[test]
    fn create_bond_valid_rolling_notice_less_than_duration() {
        let bond = create_bond(50, 0, 7200, true, 3600).unwrap();
        assert!(bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 3600);
    }

    #[test]
    fn create_bond_valid_rolling_notice_equals_duration() {
        let bond = create_bond(50, 0, 3600, true, 3600).unwrap();
        assert!(bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 3600);
    }

    #[test]
    fn create_bond_valid_max_amount() {
        let bond = create_bond(i128::MAX, 0, 1, false, 0).unwrap();
        assert_eq!(bond.amount, i128::MAX);
    }

    #[test]
    fn create_bond_valid_minimum_positive_amount() {
        let bond = create_bond(1, 0, 1, false, 0).unwrap();
        assert_eq!(bond.amount, 1);
    }

    #[test]
    fn create_bond_valid_minimum_duration() {
        let bond = create_bond(100, 0, 1, false, 0).unwrap();
        assert_eq!(bond.duration, 1);
    }

    #[test]
    fn create_bond_valid_rolling_minimum_notice() {
        let bond = create_bond(100, 0, 1, true, 1).unwrap();
        assert_eq!(bond.notice_period_duration, 1);
    }

    #[test]
    fn create_bond_non_rolling_ignores_notice_period() {
        let bond = create_bond(100, 0, 3600, false, 9999).unwrap();
        assert!(!bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 9999);
    }

    #[test]
    fn create_bond_valid_no_overflow_at_boundary() {
        let bond = create_bond(100, 0, u64::MAX, false, 0).unwrap();
        assert_eq!(bond.duration, u64::MAX);
    }

    #[test]
    fn create_bond_amount_checked_before_duration() {
        let err = create_bond(0, 0, 0, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_duration_checked_before_notice() {
        let err = create_bond(100, 0, 0, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }

    #[test]
    fn bond_state_survives_ledger_advance_after_ttl_bump() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let identity = Address::generate(&e);
        client.initialize(&admin);

        client.create_bond(&identity, &50_i128, &MAX_BOND_DURATION_SECONDS as u64, &false, &0_u64);

        let mut info = e.ledger().get();
        info.sequence_number = (STORAGE_TTL_EXTEND_TO as u64).saturating_add(10_000);
        info.timestamp = info.timestamp.saturating_add(
            (STORAGE_TTL_EXTEND_TO as u64).saturating_mul(SECONDS_PER_LEDGER).saturating_add(1),
        );
        e.ledger().set(info);

        let bond = client.get_identity_state();
        assert_eq!(bond.bonded_amount, 50);
        assert_eq!(bond.identity, identity);
    }

    #[test]
    fn attestation_state_survives_ledger_advance_after_ttl_bump() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let attester = Address::generate(&e);
        client.initialize(&admin);
        client.register_attester(&attester);

        let subject = Address::generate(&e);
        let attestation = crate::test_attestation::add(
            &client,
            &e,
            &contract_id,
            &attester,
            &subject,
            "ttl",
        );

        let mut info = e.ledger().get();
        info.sequence_number = (STORAGE_TTL_EXTEND_TO as u64).saturating_add(10_000);
        info.timestamp = info.timestamp.saturating_add(
            (STORAGE_TTL_EXTEND_TO as u64).saturating_mul(SECONDS_PER_LEDGER).saturating_add(1),
        );
        e.ledger().set(info);

        let loaded = client.get_attestation(&attestation.id);
        assert_eq!(loaded.id, attestation.id);
        assert_eq!(loaded.identity, subject);
    }

    #[test]
    fn attester_stake_state_survives_ledger_advance_after_ttl_bump() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let attester = Address::generate(&e);
        client.initialize(&admin);
        client.register_attester(&attester);

        weighted_attestation::set_attester_stake(&e.as_contract(&contract_id, || e.clone()), &attester, 123);

        let mut info = e.ledger().get();
        info.sequence_number = (STORAGE_TTL_EXTEND_TO as u64).saturating_add(10_000);
        info.timestamp = info.timestamp.saturating_add(
            (STORAGE_TTL_EXTEND_TO as u64).saturating_mul(SECONDS_PER_LEDGER).saturating_add(1),
        );
        e.ledger().set(info);

        let weight = weighted_attestation::compute_weight(&e.as_contract(&contract_id, || e.clone()), &attester);
        assert_eq!(weight, 123u32);
    }
}
