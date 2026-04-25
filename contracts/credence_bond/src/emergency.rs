//! Emergency Withdrawal Mechanism
//!
//! Enables governance-approved withdrawals in crisis scenarios with mandatory
//! fee application, event emission, and immutable audit records.

use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::math;

/// Storage key for emergency configuration.
const KEY_EMERGENCY_CONFIG: &str = "emergency_config";

/// @notice Emergency mode configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyConfig {
    pub governance: Address,
    pub treasury: Address,
    pub emergency_fee_bps: u32,
    pub enabled: bool,
}

/// @notice Immutable audit record for an emergency withdrawal execution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawalRecord {
    pub id: u64,
    pub identity: Address,
    pub gross_amount: i128,
    pub fee_amount: i128,
    pub net_amount: i128,
    pub treasury: Address,
    pub approved_admin: Address,
    pub approved_governance: Address,
    pub reason: Symbol,
    pub timestamp: u64,
}

/// @notice Immutable audit record for an emergency mode state transition.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyModeTransition {
    pub id: u64,
    pub enabled: bool,
    pub approved_admin: Address,
    pub approved_governance: Address,
    pub reason: Symbol,
    pub timestamp: u64,
}

/// Dynamic key for emergency audit records.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmergencyDataKey {
    Record(u64),
    Transition(u64),
    RecordSeq,
    TransitionSeq,
}

/// @notice Set emergency configuration.
/// @dev Rejects fee bps values above 10000.
/// @param governance Governance approver address.
/// @param treasury Treasury address receiving emergency fees.
/// @param emergency_fee_bps Emergency fee in basis points.
/// @param enabled Initial emergency mode.
pub fn set_config(
    e: &Env,
    governance: Address,
    treasury: Address,
    emergency_fee_bps: u32,
    enabled: bool,
) {
    if emergency_fee_bps > math::BPS_DENOMINATOR as u32 {
        panic!("emergency fee bps must be <= 10000 (100%)");
    }
    let cfg = EmergencyConfig {
        governance,
        treasury,
        emergency_fee_bps,
        enabled,
    };
    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_EMERGENCY_CONFIG), &cfg);
}

/// @notice Get emergency configuration.
/// @return Current emergency configuration.
pub fn get_config(e: &Env) -> EmergencyConfig {
    e.storage()
        .instance()
        .get::<_, EmergencyConfig>(&Symbol::new(e, KEY_EMERGENCY_CONFIG))
        .unwrap_or_else(|| panic!("emergency config not set"))
}

/// @notice Update emergency enabled state with full audit trail.
/// @param enabled New emergency mode status.
/// @param admin Authorized administrator who initiated the change.
/// @param governance Authorized governance address that approved the change.
/// @param reason Reason for the state transition.
pub fn set_enabled(e: &Env, enabled: bool, admin: &Address, governance: &Address, reason: Symbol) {
    let mut cfg = get_config(e);
    if cfg.enabled == enabled {
        return; // No change needed
    }
    cfg.enabled = enabled;
    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_EMERGENCY_CONFIG), &cfg);

    // Record the transition
    let transition_id = increment_seq(e, EmergencyDataKey::TransitionSeq);
    let transition = EmergencyModeTransition {
        id: transition_id,
        enabled,
        approved_admin: admin.clone(),
        approved_governance: governance.clone(),
        reason,
        timestamp: e.ledger().timestamp(),
    };

    e.storage()
        .persistent()
        .set(&EmergencyDataKey::Transition(transition_id), &transition);
}

/// @notice Calculate emergency fee for a withdrawal amount.
/// @param amount Gross withdrawal amount.
/// @param fee_bps Emergency fee basis points.
/// @return Calculated fee amount.
#[must_use]
pub fn calculate_fee(amount: i128, fee_bps: u32) -> i128 {
    if fee_bps == 0 {
        return 0;
    }
    math::bps(amount, fee_bps, "emergency fee mul", "emergency fee div")
}

/// @notice Get latest record ID.
pub fn latest_record_id(e: &Env) -> u64 {
    e.storage()
        .persistent()
        .get(&EmergencyDataKey::RecordSeq)
        .unwrap_or(0)
}

/// @notice Get withdrawal record by ID.
pub fn get_record(e: &Env, id: u64) -> EmergencyWithdrawalRecord {
    e.storage()
        .persistent()
        .get(&EmergencyDataKey::Record(id))
        .unwrap_or_else(|| panic!("record not found"))
}

/// @notice Get latest transition ID.
pub fn latest_transition_id(e: &Env) -> u64 {
    e.storage()
        .persistent()
        .get(&EmergencyDataKey::TransitionSeq)
        .unwrap_or(0)
}

/// @notice Get transition record by ID.
pub fn get_transition(e: &Env, id: u64) -> EmergencyModeTransition {
    e.storage()
        .persistent()
        .get(&EmergencyDataKey::Transition(id))
        .unwrap_or_else(|| panic!("transition not found"))
}

/// @notice Persist immutable emergency withdrawal record.
/// @dev Uses persistent storage for forensic traceability and to prevent storage bloat in instance storage.
pub fn store_record(
    e: &Env,
    identity: Address,
    gross_amount: i128,
    fee_amount: i128,
    net_amount: i128,
    treasury: Address,
    approved_admin: Address,
    approved_governance: Address,
    reason: Symbol,
) -> u64 {
    let id = increment_seq(e, EmergencyDataKey::RecordSeq);
    let record = EmergencyWithdrawalRecord {
        id,
        identity,
        gross_amount,
        fee_amount,
        net_amount,
        treasury,
        approved_admin,
        approved_governance,
        reason,
        timestamp: e.ledger().timestamp(),
    };

    e.storage()
        .persistent()
        .set(&EmergencyDataKey::Record(id), &record);
    id
}

/// @notice Internal sequence incrementer.
fn increment_seq(e: &Env, key: EmergencyDataKey) -> u64 {
    let seq: u64 = e.storage().persistent().get(&key).unwrap_or(0);
    let next = seq.checked_add(1).expect("sequence overflow");
    e.storage().persistent().set(&key, &next);
    next
}

pub fn emit_emergency_mode_event(
    e: &Env,
    enabled: bool,
    admin: &Address,
    governance: &Address,
    reason: &Symbol,
) {
    e.events().publish(
        (Symbol::new(e, "emergency_mode_changed"),),
        (enabled, admin.clone(), governance.clone(), reason.clone()),
    );
}

pub fn emit_emergency_withdrawal_event(
    e: &Env,
    record_id: u64,
    identity: &Address,
    gross_amount: i128,
    fee_amount: i128,
    net_amount: i128,
    reason: &Symbol,
) {
    e.events().publish(
        (Symbol::new(e, "emergency_withdrawal"), record_id, identity.clone()),
        (gross_amount, fee_amount, net_amount, reason.clone()),
    );
}
