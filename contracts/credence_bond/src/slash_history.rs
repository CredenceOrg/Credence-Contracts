use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashRecord {
    pub identity: Address,
    pub slash_amount: i128,
    pub reason: Symbol,
    pub timestamp: u64,
    pub total_slashed_after: i128,
}

// Use a proper contracttype enum for storage keys
#[contracttype]
#[derive(Clone)]
pub enum SlashStorageKey {
    SlashCount(Address),
    SlashRecord(Address, u32),
}

/// Append a new slash record for `identity`. Called by production slashing code.
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
// Read helpers — available in tests, tooling, and release for the paginated
// contract entry-points (get_slash_history_page / get_slash_count).
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

// ============================================================================
// Test/tooling helpers — excluded from release WASM
// ============================================================================
#[allow(dead_code)]
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

/// Full-history read helpers. Only needed by tests and off-chain tooling;
/// excluded from release WASM via `#[cfg(any(test, feature = "testutils"))]`.
#[cfg(any(test, feature = "testutils"))]
pub mod testutils {
    use super::*;

    /// Return the complete slash history for `identity` as a single vec.
    ///
    /// For large histories prefer iterating with [`super::get_slash_history_page`].
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
