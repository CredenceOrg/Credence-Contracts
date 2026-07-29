use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

/// Normalized slash history record stored persistently per identity.
///
/// This schema is stable — the five fields are the canonical shape consumed by
/// the backend reputation engine. Do NOT add fields without a migration step.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashRecord {
    /// Address of the slashed identity.
    pub identity: Address,
    /// Validated amount applied to the bond in this slash event.
    pub slash_amount: i128,
    /// Reason symbol — currently always `"admin_slash"`.
    pub reason: Symbol,
    /// Ledger timestamp at the time this slash was applied.
    pub timestamp: u64,
    /// Cumulative `slashed_amount` for this identity after this slash.
    pub total_slashed_after: i128,
}

/// Storage key discriminator for per-identity slash history entries.
// Use a proper contracttype enum for storage keys
#[contracttype]
#[derive(Clone)]
pub enum SlashStorageKey {
    SlashCount(Address),
    SlashRecord(Address, u32),
}

/// Append a new slash record for `identity`. Called by production slashing code.
///
/// Stores a [`SlashRecord`] at index `count` and increments the count.
/// Both entries have their TTL extended to [`crate::PERSISTENT_TTL_MAX`].
pub fn append_slash_history(
    e: &Env,
    identity: &Address,
    slash_amount: i128,
    reason: Symbol,
    total_slashed_after: i128,
) {
    let ttl_threshold = crate::PERSISTENT_TTL_MAX / 2;
    let ttl_max = crate::PERSISTENT_TTL_MAX;

    let count_key = SlashStorageKey::SlashCount(identity.clone());

    let mut count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);

    let record = SlashRecord {
        identity: identity.clone(),
        slash_amount,
        reason,
        timestamp: e.ledger().timestamp(),
        total_slashed_after,
    };

    let history_key = SlashStorageKey::SlashRecord(identity.clone(), count);
    e.storage().persistent().set(&history_key, &record);
    e.storage()
        .persistent()
        .extend_ttl(&history_key, ttl_threshold, ttl_max);

    count += 1;
    e.storage().persistent().set(&count_key, &count);
    e.storage()
        .persistent()
        .extend_ttl(&count_key, ttl_threshold, ttl_max);
}

// ============================================================================
// Read helpers — available in all build configurations (test and release)
// for use by contract entry-points (get_slash_history_page / get_slash_count).
// ============================================================================

/// Return the number of slash records stored for `identity`. O(1).
#[must_use]
pub fn get_slash_count(e: &Env, identity: &Address) -> u32 {
    let key = SlashStorageKey::SlashCount(identity.clone());
    let count: u32 = e.storage().persistent().get(&key).unwrap_or(0);
    if count > 0 {
        e.storage().persistent().extend_ttl(
            &key,
            crate::PERSISTENT_TTL_MAX / 2,
            crate::PERSISTENT_TTL_MAX,
        );
    }
    count
}

/// Return a single slash record by index.
///
/// Available in all build configurations (not test-only) so that contract
/// entrypoints can read individual records.
///
/// # Panics
/// Panics with `"slash record not found"` when `index >= slash_count`.
#[must_use]
pub fn get_slash_record(e: &Env, identity: &Address, index: u32) -> SlashRecord {
    let key = SlashStorageKey::SlashRecord(identity.clone(), index);
    let record = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("slash record not found"));
    e.storage().persistent().extend_ttl(
        &key,
        crate::PERSISTENT_TTL_MAX / 2,
        crate::PERSISTENT_TTL_MAX,
    );
    record
}

/// Return a bounded page of slash records for `identity`, starting at `offset`.
///
/// This is the canonical paginated read function used by the contract entrypoint
/// `get_slash_history_page`. `limit` is clamped to
/// [`crate::parameters::MAX_QUERY_LIMIT`] so a caller can never request an
/// unbounded page. Pass `limit = 0` to use the default maximum.
///
/// Records are returned in ascending insertion order (index 0 first).
///
/// # Arguments
/// * `e` - Soroban environment
/// * `identity` - Identity whose history to read
/// * `offset` - Starting index (0-based)
/// * `limit` - Maximum records to return (clamped to MAX_QUERY_LIMIT)
///
/// # Returns
/// A `Vec<SlashRecord>` of at most `effective_limit` records.
#[must_use]
pub fn get_slash_history_page(
    e: &Env,
    identity: &Address,
    offset: u32,
    limit: u32,
) -> Vec<SlashRecord> {
    let max_limit = crate::parameters::MAX_QUERY_LIMIT;
    let effective_limit = if limit == 0 || limit > max_limit {
        max_limit
    } else {
        limit
    };

    let count = get_slash_count(e, identity);
    let mut page = Vec::new(e);

    if offset >= count {
        return page;
    }

    let end = count.min(offset.saturating_add(effective_limit));
    for i in offset..end {
        let key = SlashStorageKey::SlashRecord(identity.clone(), i);
        if let Some(record) = e.storage().persistent().get::<_, SlashRecord>(&key) {
            e.storage().persistent().extend_ttl(
                &key,
                crate::PERSISTENT_TTL_MAX / 2,
                crate::PERSISTENT_TTL_MAX,
            );
            page.push_back(record);
        }
    }
    page
}

// ============================================================================
// Test/tooling helpers — excluded from release WASM
// ============================================================================

/// Full-history read helpers. Only needed by tests and off-chain tooling;
/// excluded from release WASM via `#[cfg(any(test, feature = "testutils"))]`.
#[cfg(any(test, feature = "testutils"))]
pub mod testutils {
    use super::*;

    /// Return the complete slash history for `identity` as a single vec.
    ///
    /// For large histories prefer the paginated [`super::get_slash_history_page`].
    #[must_use]
    pub fn get_slash_history(e: &Env, identity: &Address) -> Vec<SlashRecord> {
        let count = super::get_slash_count(e, identity);
        let mut history = Vec::new(e);
        for i in 0..count {
            let key = SlashStorageKey::SlashRecord(identity.clone(), i);
            if let Some(record) = e.storage().persistent().get(&key) {
                history.push_back(record);
            }
        }
        history
    }

    /// Return a single slash record by index.
    ///
    /// # Panics
    /// Panics with `"slash record not found"` when `index >= slash_count`.
    #[must_use]
    pub fn get_slash_record(e: &Env, identity: &Address, index: u32) -> SlashRecord {
        let key = SlashStorageKey::SlashRecord(identity.clone(), index);
        e.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("slash record not found"))
    }

    /// Sum all slash amounts from history. O(n) — use only in tests.
    #[must_use]
    pub fn get_total_slashed_from_history(e: &Env, identity: &Address) -> i128 {
        let history = get_slash_history(e, identity);
        let mut total: i128 = 0;
        for record in history.iter() {
            total += record.slash_amount;
        }
        total
    }
}

/// Re-export the full-history helper at module level for test convenience.
///
/// This alias allows test code to call `slash_history::get_slash_history(&e, &identity)`
/// without qualifying through the `testutils` submodule.
#[cfg(any(test, feature = "testutils"))]
pub use testutils::get_slash_history;
