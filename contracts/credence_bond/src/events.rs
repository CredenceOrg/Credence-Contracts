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
///
/// # Replay semantics
/// Genesis event for a bond. A replayer initializes `IdentityBond` from it:
/// `identity`, `bonded_amount = amount`, `bond_start = start_timestamp`,
/// `bond_duration = duration`, `is_rolling`, with `slashed_amount = 0` and
/// `active = true`. There must be exactly one of these per bond, before any
/// other lifecycle event. Note: `notice_period_duration` is **not** carried
/// here, so rolling-bond notice periods are not reconstructable from events
/// alone — see `docs/indexer-replay-contract.md`.
#[allow(dead_code)]
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
#[allow(dead_code)]
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
///
/// # Replay semantics
/// A replayer sets `bonded_amount = new_total` (the authoritative post-increase
/// balance carried in the topic; `added_amount` is supplied for convenience and
/// must equal `new_total - previous`). No other field changes. Idempotent only
/// if applied in stream order — the indexer must not reorder increases against
/// withdrawals.
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
///
/// # Replay semantics
/// A replayer sets `bonded_amount = remaining` (the authoritative post-withdraw
/// balance). Because `remaining` is absolute, partial, early, and full
/// withdrawals all replay through the same path. `is_early`/`penalty_amount` are
/// informational for the indexer and do not alter the reconstructed bond. This
/// event does **not** by itself flip `active` to `false`; full-exit
/// deactivation is signalled separately by the withdraw-bond path.
#[allow(dead_code)]
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
#[allow(dead_code)]
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
///
/// # Replay semantics
/// A replayer sets `slashed_amount = total_slashed` (the cumulative, absolute
/// figure carried in the topic; the per-event `slash_amount` is the delta and
/// must equal `total_slashed - previous_slashed`). `bonded_amount` is unchanged
/// by a slash — withdrawable balance is derived as `bonded_amount -
/// slashed_amount`. The legacy `bond_slashed` event carries the same numbers and
/// is ignored by a v2 replayer to avoid double-counting.
#[allow(clippy::too_many_arguments)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn emit_claims_expired(e: &Env, user: &Address, expired_count: u32, expired_amount: i128) {
    let topics = (Symbol::new(e, "claims_expired"), user.clone());
    let data = (expired_count, expired_amount);
    e.events().publish(topics, data);
}

/// Emitted when upgrade authorization is initialized.
#[allow(dead_code)]
pub fn emit_upgrade_auth_initialized(e: &Env, admin: &Address) {
    let topics = (Symbol::new(e, "upgrade_auth_init"), admin.clone());
    e.events().publish(topics, ());
}

/// Emitted when upgrade authorization is granted.
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn emit_upgrade_auth_revoked(e: &Env, admin: &Address, address: &Address) {
    let topics = (Symbol::new(e, "upgrade_auth_revoked"), admin.clone());
    let data = address.clone();
    e.events().publish(topics, data);
}

/// Emitted when an upgrade is proposed.
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Emitted when an administrative transfer is initiated.
#[allow(dead_code)]
pub fn emit_admin_transfer_started(e: &Env, current_admin: &Address, pending_admin: &Address) {
    let topics = (
        Symbol::new(e, "admin_transfer_started"),
        current_admin.clone(),
    );
    let data = pending_admin.clone();
    e.events().publish(topics, data);
}

/// Emitted when an administrative transfer is completed.
#[allow(dead_code)]
pub fn emit_admin_transfer_completed(e: &Env, old_admin: &Address, new_admin: &Address) {
    let topics = (
        Symbol::new(e, "admin_transfer_completed"),
        old_admin.clone(),
    );
    let data = new_admin.clone();
    e.events().publish(topics, data);
}

/// Emitted when an admin is rotated (ownership transferred). Includes ledger sequence.
#[allow(dead_code)]
pub fn emit_admin_rotated(e: &Env, previous_admin: &Address, new_admin: &Address) {
    let topics = (
        Symbol::new(e, "admin_rotated"),
        previous_admin.clone(),
        new_admin.clone(),
    );
    let ledger_seq: u32 = e.ledger().sequence();
    e.events().publish(topics, ledger_seq);
}

/// Emitted when an upgrade admin transfer is initiated.
pub fn emit_upgrade_admin_transfer_started(
    e: &Env,
    current_admin: &Address,
    pending_upgrade_admin: &Address,
) {
    let topics = (
        Symbol::new(e, "upgrade_admin_transfer_started"),
        current_admin.clone(),
    );
    let data = pending_upgrade_admin.clone();
    e.events().publish(topics, data);
}

/// Emitted when an upgrade admin transfer is completed.
pub fn emit_upgrade_admin_transfer_completed(e: &Env, old_admin: &Address, new_admin: &Address) {
    let topics = (
        Symbol::new(e, "upgrade_admin_transfer_completed"),
        old_admin.clone(),
    );
    let data = new_admin.clone();
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
#[allow(dead_code)]
pub fn emit_parameter_updated(
    e: &Env,
    key: Symbol,
    category: Symbol,
    admin: &Address,
    old_value: i128,
    new_value: i128,
) {
    let topics = (
        Symbol::new(e, "param_updated"),
        key,
        category,
        admin.clone(),
    );
    e.events().publish(topics, (old_value, new_value));
}

/// Emitted when post-write drift detection finds inconsistent bond or attestation state.
///
/// # Topics (Indexed)
/// * `Symbol` - `"bond_drift_detected"`
/// * `Address` - Subject identity (bond owner or attestation subject)
///
/// # Data
/// * [`crate::invariants::BondDriftKind`] - Which invariant failed
/// * `i128` - `bonded_amount` at detection time
/// * `i128` - `slashed_amount` at detection time
/// * `u32` - `SubjectAttestationCount` value (0 if N/A)
/// * `u32` - `SubjectAttestations` list length (0 if N/A)
pub fn emit_bond_drift_detected(e: &Env, details: &crate::invariants::BondDriftDetails) {
    let topics = (
        Symbol::new(e, "bond_drift_detected"),
        details.subject.clone(),
    );
    let data = (
        details.kind.clone(),
        details.bonded_amount,
        details.slashed_amount,
        details.attestation_count,
        details.attestation_list_len,
    );
    e.events().publish(topics, data);
}

/// Emitted when a bond is finalized through `liquidate` (issue #366).
///
/// # Topics (Indexed)
/// * `Symbol` - `"bond_liquidated"`
/// * `Address` - The identity whose bond was liquidated (so an indexer can
///   slice the event stream per identity)
///
/// # Data
/// * `i128` - Residual amount swept to the treasury
///   (`bonded_amount - slashed_amount`, or `0` if fully slashed)
/// * `Symbol` - Reason for the liquidation
///   (`"fully_slashed"` or `"expired_unrenewed"`)
/// * `u64` - Ledger timestamp at which the liquidation was recorded
/// * `Address` - Admin / keeper that drove the liquidation
///
/// # Replay semantics
/// A replayer finalizes the bond on encountering this event:
/// `IdentityBond.active = false` and `DataKey::Liquidated(identity) = true`.
/// `bonded_amount` and `slashed_amount` are preserved verbatim so the
/// accounting trace can be reconstructed; any residual token sweep is
/// expressible as a function of the reported residual amount.
///
/// Exactly one `bond_liquidated` per bond is emitted — the entrypoint is
/// idempotent on an already-inactive bond (`BondNotActive`) so replayers
/// can safely collapse duplicates.
#[allow(clippy::too_many_arguments)]
pub fn emit_bond_liquidated(
    e: &Env,
    identity: &Address,
    residual: i128,
    reason: Symbol,
    timestamp: u64,
    admin: &Address,
) {
    let topics = (Symbol::new(e, "bond_liquidated"), identity.clone());
    let data = (residual, reason, timestamp, admin.clone());
    e.events().publish(topics, data);
}
