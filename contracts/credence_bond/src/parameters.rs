//! Protocol Parameters Module
//!
//! Implements a governance-controlled configuration system for protocol parameters.
//! All parameters have defined types, units, and enforced min/max bounds.
//!
//! ## Parameter Categories
//! 1. **Fee Rates** - Protocol fees expressed as basis points (1 bps = 0.01%)
//! 2. **Cooldown Periods** - Time delays between operations (in seconds)
//! 3. **Tier Thresholds** - Value boundaries defining user/operation tiers (in token units)
//!
//! ## Governance Control
//! All parameter updates are restricted to the governance address (contract admin).
//! Non-governance callers are rejected with "not admin" error.
//!
//! ## Bounds Enforcement
//! Every parameter write validates against min/max bounds. Out-of-range values
//! are rejected with descriptive errors.
//!
//! ## Event Emission
//! All successful parameter updates emit a `ParameterChanged` event containing:
//! - parameter name
//! - old value
//! - new value
//! - caller address
//! - timestamp

use soroban_sdk::{contracttype, Address, Env, String, Symbol};

// ============================================================================
// Parameter Bounds Constants
// ============================================================================

/// Minimum protocol fee rate in basis points (0 bps = 0%)
pub const MIN_PROTOCOL_FEE_BPS: u32 = 0;
/// Maximum protocol fee rate in basis points (1000 bps = 10%)
pub const MAX_PROTOCOL_FEE_BPS: u32 = 1000;
/// Default protocol fee rate in basis points (50 bps = 0.5%)
pub const DEFAULT_PROTOCOL_FEE_BPS: u32 = 50;

/// Minimum attestation fee rate in basis points (0 bps = 0%)
pub const MIN_ATTESTATION_FEE_BPS: u32 = 0;
/// Maximum attestation fee rate in basis points (500 bps = 5%)
pub const MAX_ATTESTATION_FEE_BPS: u32 = 500;
/// Default attestation fee rate in basis points (10 bps = 0.1%)
pub const DEFAULT_ATTESTATION_FEE_BPS: u32 = 10;

/// Minimum withdrawal cooldown period in seconds (0 = no cooldown)
pub const MIN_WITHDRAWAL_COOLDOWN_SECS: u64 = 0;
/// Maximum withdrawal cooldown period in seconds (30 days)
pub const MAX_WITHDRAWAL_COOLDOWN_SECS: u64 = 2_592_000;
/// Default withdrawal cooldown period in seconds (7 days)
pub const DEFAULT_WITHDRAWAL_COOLDOWN_SECS: u64 = 604_800;

/// Minimum slash cooldown period in seconds (0 = no cooldown)
pub const MIN_SLASH_COOLDOWN_SECS: u64 = 0;
/// Maximum slash cooldown period in seconds (7 days)
pub const MAX_SLASH_COOLDOWN_SECS: u64 = 604_800;
/// Default slash cooldown period in seconds (24 hours)
pub const DEFAULT_SLASH_COOLDOWN_SECS: u64 = 86_400;

/// Minimum bronze tier threshold (0 = no minimum)
pub const MIN_BRONZE_THRESHOLD: i128 = 0;
/// Maximum bronze tier threshold (1 million tokens)
pub const MAX_BRONZE_THRESHOLD: i128 = 1_000_000_000_000;
/// Default bronze tier threshold (100 tokens)
pub const DEFAULT_BRONZE_THRESHOLD: i128 = 100_000_000;

/// Minimum silver tier threshold (must be >= bronze)
pub const MIN_SILVER_THRESHOLD: i128 = 100_000_000;
/// Maximum silver tier threshold (10 million tokens)
pub const MAX_SILVER_THRESHOLD: i128 = 10_000_000_000_000;
/// Default silver tier threshold (1000 tokens)
pub const DEFAULT_SILVER_THRESHOLD: i128 = 1_000_000_000;

/// Minimum gold tier threshold (must be >= silver)
pub const MIN_GOLD_THRESHOLD: i128 = 1_000_000_000;
/// Maximum gold tier threshold (100 million tokens)
pub const MAX_GOLD_THRESHOLD: i128 = 100_000_000_000_000;
/// Default gold tier threshold (10000 tokens)
pub const DEFAULT_GOLD_THRESHOLD: i128 = 10_000_000_000;

/// Minimum platinum tier threshold (must be >= gold)
pub const MIN_PLATINUM_THRESHOLD: i128 = 10_000_000_000;
/// Maximum platinum tier threshold (1 billion tokens)
pub const MAX_PLATINUM_THRESHOLD: i128 = 1_000_000_000_000_000;
/// Default platinum tier threshold (100000 tokens)
pub const DEFAULT_PLATINUM_THRESHOLD: i128 = 100_000_000_000;

// ============================================================================
// Storage Keys
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub enum ParameterKey {
    ProtocolFeeBps,
    AttestationFeeBps,
    WithdrawalCooldownSecs,
    SlashCooldownSecs,
    BronzeThreshold,
    SilverThreshold,
    GoldThreshold,
    PlatinumThreshold,
}

// ============================================================================
// Parameter Getters
// ============================================================================

/// Get the current protocol fee rate in basis points.
///
/// # Returns
/// Protocol fee rate (u32) in basis points. Returns default if not set.
///
/// # Example
/// ```ignore
/// let fee_bps = get_protocol_fee_bps(&e); // e.g., 50 = 0.5%
/// ```
#[must_use]
pub fn get_protocol_fee_bps(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&ParameterKey::ProtocolFeeBps)
        .unwrap_or(DEFAULT_PROTOCOL_FEE_BPS)
}

/// Get the current attestation fee rate in basis points.
///
/// # Returns
/// Attestation fee rate (u32) in basis points. Returns default if not set.
#[must_use]
pub fn get_attestation_fee_bps(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&ParameterKey::AttestationFeeBps)
        .unwrap_or(DEFAULT_ATTESTATION_FEE_BPS)
}

/// Get the current withdrawal cooldown period in seconds.
///
/// # Returns
/// Cooldown period (u64) in seconds. Returns default if not set.
#[must_use]
pub fn get_withdrawal_cooldown_secs(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&ParameterKey::WithdrawalCooldownSecs)
        .unwrap_or(DEFAULT_WITHDRAWAL_COOLDOWN_SECS)
}

/// Get the current slash cooldown period in seconds.
///
/// # Returns
/// Cooldown period (u64) in seconds. Returns default if not set.
#[must_use]
pub fn get_slash_cooldown_secs(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&ParameterKey::SlashCooldownSecs)
        .unwrap_or(DEFAULT_SLASH_COOLDOWN_SECS)
}

/// Get the bronze tier threshold in token units.
///
/// # Returns
/// Threshold amount (i128). Returns default if not set.
#[must_use]
pub fn get_bronze_threshold(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&ParameterKey::BronzeThreshold)
        .unwrap_or(DEFAULT_BRONZE_THRESHOLD)
}

/// Get the silver tier threshold in token units.
///
/// # Returns
/// Threshold amount (i128). Returns default if not set.
#[must_use]
pub fn get_silver_threshold(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&ParameterKey::SilverThreshold)
        .unwrap_or(DEFAULT_SILVER_THRESHOLD)
}

/// Get the gold tier threshold in token units.
///
/// # Returns
/// Threshold amount (i128). Returns default if not set.
#[must_use]
pub fn get_gold_threshold(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&ParameterKey::GoldThreshold)
        .unwrap_or(DEFAULT_GOLD_THRESHOLD)
}

/// Get the platinum tier threshold in token units.
///
/// # Returns
/// Threshold amount (i128). Returns default if not set.
#[must_use]
pub fn get_platinum_threshold(e: &Env) -> i128 {
    e.storage()
        .instance()
        .get(&ParameterKey::PlatinumThreshold)
        .unwrap_or(DEFAULT_PLATINUM_THRESHOLD)
}

// ============================================================================
// Parameter Setters (Governance-Only)
// ============================================================================

/// Set the protocol fee rate. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New fee rate in basis points
///
/// # Bounds
/// Must be between MIN_PROTOCOL_FEE_BPS and MAX_PROTOCOL_FEE_BPS (0-1000 bps = 0-10%)
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "protocol_fee_bps out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_protocol_fee_bps(e: &Env, admin: &Address, value: u32) {
    validate_admin(e, admin);

    if !(MIN_PROTOCOL_FEE_BPS..=MAX_PROTOCOL_FEE_BPS).contains(&value) {
        panic!("protocol_fee_bps out of bounds");
    }

    let old_value = get_protocol_fee_bps(e);
    e.storage()
        .instance()
        .set(&ParameterKey::ProtocolFeeBps, &value);

    emit_parameter_changed(
        e,
        "protocol_fee_bps",
        old_value as i128,
        value as i128,
        admin,
    );
}

/// Set the attestation fee rate. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New fee rate in basis points
///
/// # Bounds
/// Must be between MIN_ATTESTATION_FEE_BPS and MAX_ATTESTATION_FEE_BPS (0-500 bps = 0-5%)
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "attestation_fee_bps out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_attestation_fee_bps(e: &Env, admin: &Address, value: u32) {
    validate_admin(e, admin);

    if !(MIN_ATTESTATION_FEE_BPS..=MAX_ATTESTATION_FEE_BPS).contains(&value) {
        panic!("attestation_fee_bps out of bounds");
    }

    let old_value = get_attestation_fee_bps(e);
    e.storage()
        .instance()
        .set(&ParameterKey::AttestationFeeBps, &value);

    emit_parameter_changed(
        e,
        "attestation_fee_bps",
        old_value as i128,
        value as i128,
        admin,
    );
}

/// Set the withdrawal cooldown period. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New cooldown period in seconds
///
/// # Bounds
/// Must be between MIN_WITHDRAWAL_COOLDOWN_SECS and MAX_WITHDRAWAL_COOLDOWN_SECS (0-30 days)
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "withdrawal_cooldown_secs out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_withdrawal_cooldown_secs(e: &Env, admin: &Address, value: u64) {
    validate_admin(e, admin);

    if !(MIN_WITHDRAWAL_COOLDOWN_SECS..=MAX_WITHDRAWAL_COOLDOWN_SECS).contains(&value) {
        panic!("withdrawal_cooldown_secs out of bounds");
    }

    let old_value = get_withdrawal_cooldown_secs(e);
    e.storage()
        .instance()
        .set(&ParameterKey::WithdrawalCooldownSecs, &value);

    emit_parameter_changed(
        e,
        "withdrawal_cooldown_secs",
        old_value as i128,
        value as i128,
        admin,
    );
}

/// Set the slash cooldown period. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New cooldown period in seconds
///
/// # Bounds
/// Must be between MIN_SLASH_COOLDOWN_SECS and MAX_SLASH_COOLDOWN_SECS (0-7 days)
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "slash_cooldown_secs out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_slash_cooldown_secs(e: &Env, admin: &Address, value: u64) {
    validate_admin(e, admin);

    if !(MIN_SLASH_COOLDOWN_SECS..=MAX_SLASH_COOLDOWN_SECS).contains(&value) {
        panic!("slash_cooldown_secs out of bounds");
    }

    let old_value = get_slash_cooldown_secs(e);
    e.storage()
        .instance()
        .set(&ParameterKey::SlashCooldownSecs, &value);

    emit_parameter_changed(
        e,
        "slash_cooldown_secs",
        old_value as i128,
        value as i128,
        admin,
    );
}

/// Set the bronze tier threshold. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New threshold in token units
///
/// # Bounds
/// Must be between MIN_BRONZE_THRESHOLD and MAX_BRONZE_THRESHOLD
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "bronze_threshold out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_bronze_threshold(e: &Env, admin: &Address, value: i128) {
    validate_admin(e, admin);

    if !(MIN_BRONZE_THRESHOLD..=MAX_BRONZE_THRESHOLD).contains(&value) {
        panic!("bronze_threshold out of bounds");
    }

    let old_value = get_bronze_threshold(e);
    e.storage()
        .instance()
        .set(&ParameterKey::BronzeThreshold, &value);

    emit_parameter_changed(e, "bronze_threshold", old_value, value, admin);
}

/// Set the silver tier threshold. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New threshold in token units
///
/// # Bounds
/// Must be between MIN_SILVER_THRESHOLD and MAX_SILVER_THRESHOLD
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "silver_threshold out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_silver_threshold(e: &Env, admin: &Address, value: i128) {
    validate_admin(e, admin);

    if !(MIN_SILVER_THRESHOLD..=MAX_SILVER_THRESHOLD).contains(&value) {
        panic!("silver_threshold out of bounds");
    }

    let old_value = get_silver_threshold(e);
    e.storage()
        .instance()
        .set(&ParameterKey::SilverThreshold, &value);

    emit_parameter_changed(e, "silver_threshold", old_value, value, admin);
}

/// Set the gold tier threshold. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New threshold in token units
///
/// # Bounds
/// Must be between MIN_GOLD_THRESHOLD and MAX_GOLD_THRESHOLD
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "gold_threshold out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_gold_threshold(e: &Env, admin: &Address, value: i128) {
    validate_admin(e, admin);

    if !(MIN_GOLD_THRESHOLD..=MAX_GOLD_THRESHOLD).contains(&value) {
        panic!("gold_threshold out of bounds");
    }

    let old_value = get_gold_threshold(e);
    e.storage()
        .instance()
        .set(&ParameterKey::GoldThreshold, &value);

    emit_parameter_changed(e, "gold_threshold", old_value, value, admin);
}

/// Set the platinum tier threshold. Governance-only.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Governance address (must be contract admin)
/// * `value` - New threshold in token units
///
/// # Bounds
/// Must be between MIN_PLATINUM_THRESHOLD and MAX_PLATINUM_THRESHOLD
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "platinum_threshold out of bounds" if value < min or value > max
///
/// # Events
/// Emits `parameter_changed` event with old and new values
pub fn set_platinum_threshold(e: &Env, admin: &Address, value: i128) {
    validate_admin(e, admin);

    if !(MIN_PLATINUM_THRESHOLD..=MAX_PLATINUM_THRESHOLD).contains(&value) {
        panic!("platinum_threshold out of bounds");
    }

    let old_value = get_platinum_threshold(e);
    e.storage()
        .instance()
        .set(&ParameterKey::PlatinumThreshold, &value);

    emit_parameter_changed(e, "platinum_threshold", old_value, value, admin);
}

// ============================================================================
// Internal Helpers
// ============================================================================

/// Validates that the caller is the authorized admin.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `caller` - Address to validate as admin
///
/// # Panics
/// - "not initialized" if contract not initialized
/// - "not admin" if caller is not the stored admin address
fn validate_admin(e: &Env, caller: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&crate::DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"));
    if caller != &stored_admin {
        panic!("not admin");
    }
}

/// Emits a parameter change event for off-chain tracking and auditing.
///
/// # Arguments
/// * `e` - Soroban environment for event publishing
/// * `parameter` - Name of the parameter that changed
/// * `old_value` - Previous value (normalized to i128)
/// * `new_value` - New value (normalized to i128)
/// * `updated_by` - Address that performed the update
fn emit_parameter_changed(
    e: &Env,
    parameter: &str,
    old_value: i128,
    new_value: i128,
    updated_by: &Address,
) {
    let timestamp = e.ledger().timestamp();
    e.events().publish(
        (Symbol::new(e, "parameter_changed"),),
        (
            String::from_str(e, parameter),
            old_value,
            new_value,
            updated_by.clone(),
            timestamp,
        ),
    );
}
