use soroban_sdk::{Address, Env, String, Symbol};

/// Emitted when a new bond is created.
///
/// # Topics (Indexed)
/// * `Symbol` - "bond_created_v2"
/// * `Address` - The identity owning the bond
/// * `i128` - The initial bonded amount (indexed for amount-based queries)
/// * `u64` - The bond start timestamp (indexed for time-based queries)
///
/// # Data
/// * `u64` - The duration of the bond in seconds
/// * `bool` - Whether the bond is rolling
/// * `u64` - Bond end timestamp (calculated)
pub fn emit_bond_created_v2(
    e: &Env,
    identity: &Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
    start_timestamp: u64,
) {
    let topics = (
        Symbol::new(e, "bond_created_v2"),
        identity.clone(),
        amount,
        start_timestamp,
    );
    let end_timestamp = start_timestamp
        .checked_add(duration)
        .expect("timestamp overflow");
    let data = (duration, is_rolling, end_timestamp);
    e.events().publish(topics, data);
}

/// Emitted when a new bond is created.
///
/// # Topics
/// * `Symbol` - "bond_created"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The initial bonded amount
/// * `u64` - The duration of the bond in seconds
/// * `bool` - Whether the bond is rolling
///
/// @deprecated Use emit_bond_created_v2 for better indexing
pub fn emit_bond_created(
    e: &Env,
    identity: &Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
) {
    let topics = (Symbol::new(e, "bond_created"), identity.clone());
    let data = (amount, duration, is_rolling);
    e.events().publish(topics, data);
}

/// Emitted when an existing bond is increased (topped up).
///
/// # Topics (Indexed)
/// * `Symbol` - "bond_increased_v2"
/// * `Address` - The identity owning the bond
/// * `i128` - The additional amount added (indexed for amount-based queries)
/// * `i128` - The new total bonded amount (indexed for balance queries)
/// * `u64` - The increase timestamp (indexed for time-based queries)
///
/// # Data
/// * `bool` - Whether this increase crossed a tier threshold
/// * `crate::BondTier` - New bond tier after increase
#[allow(dead_code)]
pub fn emit_bond_increased_v2(
    e: &Env,
    identity: &Address,
    added_amount: i128,
    new_total: i128,
    timestamp: u64,
    tier_changed: bool,
    new_tier: crate::BondTier,
) {
    let topics = (
        Symbol::new(e, "bond_increased_v2"),
        identity.clone(),
        added_amount,
        new_total,
        timestamp,
    );
    let data = (tier_changed, new_tier);
    e.events().publish(topics, data);
}

/// Emitted when an existing bond is increased (topped up).
///
/// # Topics
/// * `Symbol` - "bond_increased"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The additional amount added
/// * `i128` - The new total bonded amount
///
/// @deprecated Use emit_bond_increased_v2 for better indexing
#[allow(dead_code)]
pub fn emit_bond_increased(e: &Env, identity: &Address, added_amount: i128, new_total: i128) {
    let topics = (Symbol::new(e, "bond_increased"), identity.clone());
    let data = (added_amount, new_total);
    e.events().publish(topics, data);
}

/// Emitted when funds are successfully withdrawn from a bond.
///
/// # Topics (Indexed)
/// * `Symbol` - "bond_withdrawn_v2"
/// * `Address` - The identity owning the bond
/// * `i128` - The amount withdrawn (indexed for amount-based queries)
/// * `i128` - The remaining bonded amount (indexed for balance queries)
/// * `u64` - The withdrawal timestamp (indexed for time-based queries)
///
/// # Data
/// * `bool` - Whether this was an early withdrawal (penalty applied)
/// * `i128` - Penalty amount if early withdrawal
pub fn emit_bond_withdrawn_v2(
    e: &Env,
    identity: &Address,
    amount_withdrawn: i128,
    remaining: i128,
    timestamp: u64,
    is_early: bool,
    penalty_amount: i128,
) {
    let topics = (
        Symbol::new(e, "bond_withdrawn_v2"),
        identity.clone(),
        amount_withdrawn,
        remaining,
        timestamp,
    );
    let data = (is_early, penalty_amount);
    e.events().publish(topics, data);
}

/// Emitted when funds are successfully withdrawn from a bond.
///
/// # Topics
/// * `Symbol` - "bond_withdrawn"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The amount withdrawn
/// * `i128` - The remaining bonded amount
///
/// @deprecated Use emit_bond_withdrawn_v2 for better indexing
pub fn emit_bond_withdrawn(e: &Env, identity: &Address, amount_withdrawn: i128, remaining: i128) {
    let topics = (Symbol::new(e, "bond_withdrawn"), identity.clone());
    let data = (amount_withdrawn, remaining);
    e.events().publish(topics, data);
}

/// Emitted when a bond is slashed by an admin.
///
/// # Topics (Indexed)
/// * `Symbol` - "bond_slashed_v2"
/// * `Address` - The identity owning the bond
/// * `i128` - The amount slashed in this event (indexed for amount-based queries)
/// * `i128` - The new total slashed amount for this bond (indexed for tracking)
/// * `u64` - The slash timestamp (indexed for time-based queries)
/// * `Address` - The admin who performed the slash (indexed for accountability)
///
/// # Data
/// * `String` - Reason for the slash
/// * `bool` - Whether this was a full slash (bond completely liquidated)
pub fn emit_bond_slashed_v2(
    e: &Env,
    identity: &Address,
    slash_amount: i128,
    total_slashed: i128,
    timestamp: u64,
    admin: &Address,
    reason: String,
    is_full_slash: bool,
) {
    let topics = (
        Symbol::new(e, "bond_slashed_v2"),
        identity.clone(),
        slash_amount,
        total_slashed,
        timestamp,
        admin.clone(),
    );
    let data = (reason, is_full_slash);
    e.events().publish(topics, data);
}

/// Emitted when a bond is slashed by an admin.
///
/// # Topics
/// * `Symbol` - "bond_slashed"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The amount slashed in this event
/// * `i128` - The new total slashed amount for this bond
///
/// @deprecated Use emit_bond_slashed_v2 for better indexing
#[allow(dead_code)]
pub fn emit_bond_slashed(e: &Env, identity: &Address, slash_amount: i128, total_slashed: i128) {
    let topics = (Symbol::new(e, "bond_slashed"), identity.clone());
    let data = (slash_amount, total_slashed);
    e.events().publish(topics, data);
}

/// Emitted when a new claim is added for a user.
///
/// # Topics
/// * `Symbol` - "claim_added"
/// * `Address` - The user who can claim
///
/// # Data
/// * `crate::claims::ClaimType` - The type of claim
/// * `i128` - The amount to be claimed
/// * `u64` - The source ID that generated this claim
pub fn emit_claim_added(e: &Env, user: &Address, claim: &crate::claims::PendingClaim) {
    let topics = (Symbol::new(e, "claim_added"), user.clone());
    let data = (claim.claim_type, claim.amount, claim.source_id);
    e.events().publish(topics, data);
}

/// Emitted when claims are processed by a user.
///
/// # Topics
/// * `Symbol` - "claims_processed"
/// * `Address` - The user who claimed
///
/// # Data
/// * `u32` - Number of claims processed
/// * `i128` - Total amount claimed
/// * `soroban_sdk::Vec<crate::claims::ClaimType>` - Types of claims processed
pub fn emit_claims_processed(
    e: &Env,
    user: &Address,
    result: &crate::claims::ClaimResult,
    _processed_claims: &soroban_sdk::Vec<crate::claims::PendingClaim>,
) {
    let topics = (Symbol::new(e, "claims_processed"), user.clone());
    let data = (
        result.processed_count,
        result.total_amount,
        result.claim_types.clone(),
    );
    e.events().publish(topics, data);
}

/// Emitted when expired claims are cleaned up.
///
/// # Topics
/// * `Symbol` - "claims_expired"
/// * `Address` - The user whose claims expired
///
/// # Data
/// * `u32` - Number of expired claims removed
/// * `i128` - Total amount of expired claims
pub fn emit_claims_expired(e: &Env, user: &Address, expired_count: u32, expired_amount: i128) {
    let topics = (Symbol::new(e, "claims_expired"), user.clone());
    let data = (expired_count, expired_amount);
    e.events().publish(topics, data);
}

/// Emitted when upgrade authorization is initialized.
pub fn emit_upgrade_auth_initialized(e: &Env, admin: &Address) {
    let topics = (Symbol::new(e, "upgrade_auth_init"), admin.clone());
    e.events().publish(topics, ());
}

/// Emitted when upgrade authorization is granted.
pub fn emit_upgrade_auth_granted(
    e: &Env,
    admin: &Address,
    address: &Address,
    role: crate::upgrade_auth::UpgradeRole,
) {
    let topics = (Symbol::new(e, "upgrade_auth_granted"), admin.clone());
    let data = (address.clone(), role);
    e.events().publish(topics, data);
}

/// Emitted when upgrade authorization is revoked.
pub fn emit_upgrade_auth_revoked(e: &Env, admin: &Address, address: &Address) {
    let topics = (Symbol::new(e, "upgrade_auth_revoked"), admin.clone());
    let data = address.clone();
    e.events().publish(topics, data);
}

/// Emitted when an upgrade is proposed.
pub fn emit_upgrade_proposed(
    e: &Env,
    proposer: &Address,
    proposal_id: u64,
    new_implementation: &Address,
) {
    let topics = (Symbol::new(e, "upgrade_proposed"), proposer.clone());
    let data = (proposal_id, new_implementation.clone());
    e.events().publish(topics, data);
}

/// Emitted when an upgrade proposal is approved.
pub fn emit_upgrade_approved(e: &Env, approver: &Address, proposal_id: u64) {
    let topics = (Symbol::new(e, "upgrade_approved"), approver.clone());
    let data = proposal_id;
    e.events().publish(topics, data);
}

/// Emitted when an upgrade is executed.
pub fn emit_upgrade_executed(
    e: &Env,
    executor: &Address,
    new_implementation: &Address,
    proposal_id: Option<u64>,
) {
    let topics = (Symbol::new(e, "upgrade_executed"), executor.clone());
    let data = (new_implementation.clone(), proposal_id);
    e.events().publish(topics, data);
}


/// Emitted when a protocol parameter is updated.
/// 
/// # Topics (Indexed)
/// * `Symbol` - "param_updated"
/// * `Symbol` - Parameter Key (e.g., "leverage")
/// * `Symbol` - Category (e.g., "risk")
/// * `Address` - Admin who performed the update
///
/// # Data
/// * `i128` - Old value
/// * `i128` - New value
pub fn emit_parameter_updated(
    e: &Env,
    key: Symbol,
    category: Symbol,
    admin: &Address,
    old_value: i128,
    new_value: i128,
) {
    let topics = (Symbol::new(e, "param_updated"), key, category, admin.clone());
    e.events().publish(topics, (old_value, new_value));
}
