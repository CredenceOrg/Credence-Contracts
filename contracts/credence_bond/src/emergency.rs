//! Emergency Withdrawal Mechanism
//!
//! Enables governance-approved withdrawals in crisis scenarios with mandatory
//! fee application, event emission, and immutable audit records.

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Storage key for emergency configuration.
const KEY_EMERGENCY_CONFIG: &str = "emergency_config";
/// Storage key for latest emergency withdrawal record id.
const KEY_EMERGENCY_RECORD_SEQ: &str = "emergency_record_seq";

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

/// Dynamic key for emergency audit records.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmergencyDataKey {
    Record(u64),
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
    if emergency_fee_bps > 10_000 {
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

/// @notice Update emergency enabled state.
/// @param enabled New emergency mode status.
pub fn set_enabled(e: &Env, enabled: bool) {
    let mut cfg = get_config(e);
    cfg.enabled = enabled;
    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_EMERGENCY_CONFIG), &cfg);
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
    amount
        .checked_mul(fee_bps as i128)
        .expect("emergency fee multiplication overflow")
        / 10_000
}

/// @notice Persist an immutable emergency withdrawal record.
/// @param identity Bond identity address.
/// @param gross_amount Gross emergency withdrawal amount.
/// @param fee_amount Fee amount charged.
/// @param net_amount Net amount after fee.
/// @param treasury Treasury receiving emergency fee.
/// @param approved_admin Admin approver address.
/// @param approved_governance Governance approver address.
/// @param reason Symbolic reason code for audit trail.
/// @return Created record id.
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
    let next_id = e
        .storage()
        .instance()
        .get::<_, u64>(&Symbol::new(e, KEY_EMERGENCY_RECORD_SEQ))
        .unwrap_or(0)
        .checked_add(1)
        .expect("emergency record id overflow");

    let record = EmergencyWithdrawalRecord {
        id: next_id,
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
        .instance()
        .set(&Symbol::new(e, KEY_EMERGENCY_RECORD_SEQ), &next_id);
    e.storage()
        .instance()
        .set(&EmergencyDataKey::Record(next_id), &record);
    next_id
}

/// @notice Get latest emergency withdrawal record id, or 0 if no records.
/// @return Latest record id.
#[must_use]
pub fn latest_record_id(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get::<_, u64>(&Symbol::new(e, KEY_EMERGENCY_RECORD_SEQ))
        .unwrap_or(0)
}

/// @notice Get emergency withdrawal record by id.
/// @param id Emergency record id.
/// @return Matching emergency withdrawal record.
pub fn get_record(e: &Env, id: u64) -> EmergencyWithdrawalRecord {
    e.storage()
        .instance()
        .get::<_, EmergencyWithdrawalRecord>(&EmergencyDataKey::Record(id))
        .unwrap_or_else(|| panic!("emergency record not found"))
}

/// @notice Emit emergency mode event.
/// @param enabled New emergency mode status.
/// @param admin Admin approver.
/// @param governance Governance approver.
pub fn emit_emergency_mode_event(e: &Env, enabled: bool, admin: &Address, governance: &Address) {
    e.events().publish(
        (Symbol::new(e, "emergency_mode"),),
        (
            enabled,
            admin.clone(),
            governance.clone(),
            e.ledger().timestamp(),
        ),
    );
}

/// @notice Emit emergency withdrawal event.
/// @param record_id Emergency record id.
/// @param identity Bond identity.
/// @param gross_amount Gross emergency withdrawal amount.
/// @param fee_amount Emergency fee amount.
/// @param net_amount Net amount after fee.
/// @param reason Symbolic reason code.
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
        (Symbol::new(e, "emergency_withdrawal"),),
        (
            record_id,
            identity.clone(),
            gross_amount,
            fee_amount,
            net_amount,
            reason.clone(),
            e.ledger().timestamp(),
        ),
    );
}
