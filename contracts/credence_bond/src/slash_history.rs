use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::parameters::MAX_QUERY_LIMIT;

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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[must_use]
pub fn get_slash_history(e: &Env, identity: &Address) -> Vec<SlashRecord> {
    let count = get_slash_count(e, identity);
    let mut history = Vec::new(e);

    for i in 0..count {
        let key = SlashStorageKey::SlashRecord(identity.clone(), i);
        if let Some(record) = e.storage().persistent().get(&key) {
            e.storage().persistent().extend_ttl(
                &key,
                crate::PERSISTENT_TTL_MAX / 2,
                crate::PERSISTENT_TTL_MAX,
            );
            history.push_back(record);
        }
    }

    history
}

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

#[allow(dead_code)]
#[must_use]
pub fn get_total_slashed_from_history(e: &Env, identity: &Address) -> i128 {
    let history = get_slash_history(e, identity);
    let mut total: i128 = 0;
    for record in history.iter() {
        total += record.slash_amount;
    }
    total
}

/// Return a bounded page of slash records for `identity`.
///
/// `limit` is silently clamped to [`MAX_QUERY_LIMIT`] (200). Pass `0` to use
/// the cap directly. Returns an empty vec when `offset >= get_slash_count(e, identity)`.
///
/// # Arguments
/// * `e`        - Soroban environment
/// * `identity` - Address whose slash history to page through
/// * `offset`   - Zero-based start index within the slash-record sequence
/// * `limit`    - Maximum records to return; clamped to `MAX_QUERY_LIMIT`
///
/// # Example — page through all records
/// ```text
/// let mut offset = 0u32;
/// loop {
///     let page = get_slash_history_page(e, &identity, offset, 50);
///     if page.is_empty() { break; }
///     offset += page.len();
/// }
/// ```
#[allow(dead_code)]
#[must_use]
pub fn get_slash_history_page(
    e: &Env,
    identity: &Address,
    offset: u32,
    limit: u32,
) -> Vec<SlashRecord> {
    let count = get_slash_count(e, identity);
    let mut page = Vec::new(e);

    if offset >= count {
        return page;
    }

    let effective_limit = if limit == 0 {
        MAX_QUERY_LIMIT
    } else {
        limit.min(MAX_QUERY_LIMIT)
    };

    let end = (offset + effective_limit).min(count);

    for i in offset..end {
        let key = SlashStorageKey::SlashRecord(identity.clone(), i);
        if let Some(record) = e.storage().persistent().get(&key) {
            page.push_back(record);
        }
    }

    page
}
