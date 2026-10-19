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

// ============================================================================
// Boundary and Recovery Tests (Issue #1349)
// ============================================================================

#[cfg(test)]
mod boundary_recovery_tests {
    use super::*;
    use crate::parameters::MAX_QUERY_LIMIT;
    use crate::CredenceBond;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env, Symbol, Vec};
    use std::panic::AssertUnwindSafe;

    /// Registers a bare contract so `e.storage().persistent()` is bound to a contract ID.
    fn register(e: &Env) -> Address {
        e.register(CredenceBond, ())
    }

    // ── 1. Unset & Empty State Boundaries ────────────────────────────────────

    #[test]
    fn count_defaults_to_zero_for_unslashed_identity() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let count = get_slash_count(&e, &identity);
            assert_eq!(count, 0, "fresh identity must have 0 slash records");
        });
    }

    #[test]
    fn get_record_on_empty_history_panics_deterministically() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let result_0 = std::panic::catch_unwind(AssertUnwindSafe(|| {
                get_slash_record(&e, &identity, 0);
            }));
            assert!(result_0.is_err(), "reading index 0 on empty history must panic");

            let result_max = std::panic::catch_unwind(AssertUnwindSafe(|| {
                get_slash_record(&e, &identity, u32::MAX);
            }));
            assert!(result_max.is_err(), "reading index u32::MAX on empty history must panic");
        });
    }

    #[test]
    fn page_on_empty_history_returns_empty_vec_across_offsets() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            // Offset 0, standard limit
            let p0 = get_slash_history_page(&e, &identity, 0, 10);
            assert_eq!(p0.len(), 0);

            // Offset 0, limit 0 (clamped to MAX_QUERY_LIMIT)
            let p_zero_limit = get_slash_history_page(&e, &identity, 0, 0);
            assert_eq!(p_zero_limit.len(), 0);

            // Offset beyond 0
            let p_offset = get_slash_history_page(&e, &identity, 100, 10);
            assert_eq!(p_offset.len(), 0);

            // Offset at u32::MAX
            let p_max = get_slash_history_page(&e, &identity, u32::MAX, 10);
            assert_eq!(p_max.len(), 0);
        });
    }

    // ── 2. Single Record & Off-by-One Boundaries ────────────────────────────

    #[test]
    fn single_record_stores_exact_fields_and_bounds_index() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let timestamp = e.ledger().timestamp();

            append_slash_history(&e, &identity, 500, reason.clone(), 500);

            assert_eq!(get_slash_count(&e, &identity), 1);

            let record = get_slash_record(&e, &identity, 0);
            assert_eq!(record.identity, identity);
            assert_eq!(record.slash_amount, 500);
            assert_eq!(record.reason, reason);
            assert_eq!(record.timestamp, timestamp);
            assert_eq!(record.total_slashed_after, 500);

            // Off-by-one boundary: index 1 must panic since count is 1
            let out_of_bounds = std::panic::catch_unwind(AssertUnwindSafe(|| {
                get_slash_record(&e, &identity, 1);
            }));
            assert!(out_of_bounds.is_err(), "reading index == count must panic");
        });
    }

    // ── 3. Sequential Invariants & Monotonic Indexing ────────────────────────

    #[test]
    fn sequential_appends_maintain_contiguous_zero_based_indices() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let count_target: u32 = 7;

            for i in 0..count_target {
                let amount = ((i as i128) + 1) * 100;
                let cum = amount * ((i as i128) + 1);
                append_slash_history(&e, &identity, amount, reason.clone(), cum);
                assert_eq!(
                    get_slash_count(&e, &identity),
                    i + 1,
                    "count must increment strictly by 1 after each append"
                );
            }

            // Verify all indices 0..count_target are contiguous and readable
            for i in 0..count_target {
                let record = get_slash_record(&e, &identity, i);
                let expected_amount = ((i as i128) + 1) * 100;
                assert_eq!(record.slash_amount, expected_amount);
            }

            // Off-by-one: index == count must panic
            let past_end = std::panic::catch_unwind(AssertUnwindSafe(|| {
                get_slash_record(&e, &identity, count_target);
            }));
            assert!(past_end.is_err(), "index == count must fail");
        });
    }

    #[test]
    fn total_slashed_from_history_recovers_cumulative_amount() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let amounts: [i128; 4] = [150, 350, 500, 1000];
            let mut cumulative: i128 = 0;

            for &amt in amounts.iter() {
                cumulative += amt;
                append_slash_history(&e, &identity, amt, reason.clone(), cumulative);
            }

            let total_from_records = testutils::get_total_slashed_from_history(&e, &identity);
            assert_eq!(total_from_records, 2000);

            // Latest record must match cumulative total
            let latest = get_slash_record(&e, &identity, 3);
            assert_eq!(latest.total_slashed_after, total_from_records);
        });
    }

    // ── 4. Numeric & Boundary Amounts ───────────────────────────────────────

    #[test]
    fn zero_slash_amount_persists_without_state_corruption() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            append_slash_history(&e, &identity, 0, reason.clone(), 1000);

            assert_eq!(get_slash_count(&e, &identity), 1);
            let r = get_slash_record(&e, &identity, 0);
            assert_eq!(r.slash_amount, 0);
            assert_eq!(r.total_slashed_after, 1000);
        });
    }

    #[test]
    fn extreme_i128_values_handled_without_overflow() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            append_slash_history(&e, &identity, i128::MAX, reason.clone(), i128::MAX);

            assert_eq!(get_slash_count(&e, &identity), 1);
            let r = get_slash_record(&e, &identity, 0);
            assert_eq!(r.slash_amount, i128::MAX);
            assert_eq!(r.total_slashed_after, i128::MAX);
        });
    }

    #[test]
    fn custom_reason_symbols_persisted_deterministically() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reasons = [
                Symbol::new(&e, "admin_slash"),
                Symbol::new(&e, "governance"),
                Symbol::new(&e, "penalty_v2"),
            ];

            for (i, reason) in reasons.iter().enumerate() {
                append_slash_history(&e, &identity, 100, reason.clone(), ((i as i128) + 1) * 100);
            }

            assert_eq!(get_slash_count(&e, &identity), 3);
            for (i, reason) in reasons.iter().enumerate() {
                let rec = get_slash_record(&e, &identity, i as u32);
                assert_eq!(&rec.reason, reason);
            }
        });
    }

    // ── 5. Pagination Boundaries & Windowing ─────────────────────────────────

    #[test]
    fn page_limit_zero_defaults_to_max_query_limit() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let total = MAX_QUERY_LIMIT + 10;
            for i in 0..total {
                append_slash_history(&e, &identity, 10, reason.clone(), (i as i128 + 1) * 10);
            }

            // limit 0 should clamp to MAX_QUERY_LIMIT
            let page = get_slash_history_page(&e, &identity, 0, 0);
            assert_eq!(page.len(), MAX_QUERY_LIMIT);
        });
    }

    #[test]
    fn page_limit_above_max_clamped_to_max_query_limit() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let total = MAX_QUERY_LIMIT + 15;
            for i in 0..total {
                append_slash_history(&e, &identity, 10, reason.clone(), (i as i128 + 1) * 10);
            }

            let page = get_slash_history_page(&e, &identity, 0, MAX_QUERY_LIMIT + 100);
            assert_eq!(page.len(), MAX_QUERY_LIMIT);
        });
    }

    #[test]
    fn page_offset_boundary_conditions() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            for i in 0..5 {
                append_slash_history(&e, &identity, 10, reason.clone(), (i as i128 + 1) * 10);
            }

            // Boundary: offset == count returns empty page
            let at_count = get_slash_history_page(&e, &identity, 5, 10);
            assert_eq!(at_count.len(), 0);

            // Boundary: offset == count - 1 returns exactly last record
            let at_last = get_slash_history_page(&e, &identity, 4, 10);
            assert_eq!(at_last.len(), 1);
            assert_eq!(at_last.get(0).unwrap().slash_amount, 10);

            // Boundary: offset > count returns empty page
            let beyond = get_slash_history_page(&e, &identity, 10, 10);
            assert_eq!(beyond.len(), 0);

            // Boundary: saturating add with huge offset does not panic or wrap
            let huge_offset = get_slash_history_page(&e, &identity, u32::MAX - 2, 10);
            assert_eq!(huge_offset.len(), 0);
        });
    }

    #[test]
    fn multipage_walk_reconstructs_full_history_in_order() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            let total_records = 23;
            for i in 0..total_records {
                append_slash_history(&e, &identity, (i as i128) + 1, reason.clone(), (i as i128) + 1);
            }

            let page_size = 5;
            let mut offset = 0;
            let mut collected = Vec::new(&e);

            loop {
                let page = get_slash_history_page(&e, &identity, offset, page_size);
                if page.is_empty() {
                    break;
                }
                for rec in page.iter() {
                    collected.push_back(rec);
                }
                offset += page.len();
            }

            assert_eq!(collected.len(), total_records);

            // Verify order matches 0..total_records
            for i in 0..total_records {
                let r = collected.get(i).unwrap();
                assert_eq!(r.slash_amount, (i as i128) + 1);
            }
        });
    }

    // ── 6. Multi-Identity Isolation ─────────────────────────────────────────

    #[test]
    fn interleaved_appends_isolate_histories_cleanly() {
        let e = Env::default();
        let cid = register(&e);
        let a = Address::generate(&e);
        let b = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");

            // Interleaved appends: A0, B0, A1, B1, A2
            append_slash_history(&e, &a, 100, reason.clone(), 100);
            append_slash_history(&e, &b, 50, reason.clone(), 50);
            append_slash_history(&e, &a, 200, reason.clone(), 300);
            append_slash_history(&e, &b, 150, reason.clone(), 200);
            append_slash_history(&e, &a, 300, reason.clone(), 600);

            // Identity A verification
            assert_eq!(get_slash_count(&e, &a), 3);
            let a_page = get_slash_history_page(&e, &a, 0, 10);
            assert_eq!(a_page.len(), 3);
            assert_eq!(a_page.get(0).unwrap().slash_amount, 100);
            assert_eq!(a_page.get(1).unwrap().slash_amount, 200);
            assert_eq!(a_page.get(2).unwrap().slash_amount, 300);

            // Identity B verification
            assert_eq!(get_slash_count(&e, &b), 2);
            let b_page = get_slash_history_page(&e, &b, 0, 10);
            assert_eq!(b_page.len(), 2);
            assert_eq!(b_page.get(0).unwrap().slash_amount, 50);
            assert_eq!(b_page.get(1).unwrap().slash_amount, 150);
        });
    }

    // ── 7. Recovery, Failure Paths, and Idempotence ──────────────────────────

    #[test]
    fn failed_read_does_not_corrupt_count_or_existing_records() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            append_slash_history(&e, &identity, 100, reason.clone(), 100);
            append_slash_history(&e, &identity, 200, reason.clone(), 300);

            // Induce panic by requesting out-of-bounds index
            let panic_attempt = std::panic::catch_unwind(AssertUnwindSafe(|| {
                get_slash_record(&e, &identity, 99);
            }));
            assert!(panic_attempt.is_err());

            // Post-panic state recovery: count and existing records remain untouched
            assert_eq!(get_slash_count(&e, &identity), 2);
            let r0 = get_slash_record(&e, &identity, 0);
            let r1 = get_slash_record(&e, &identity, 1);
            assert_eq!(r0.slash_amount, 100);
            assert_eq!(r1.slash_amount, 200);
        });
    }

    #[test]
    fn repeated_reads_are_deterministic_and_idempotent() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            append_slash_history(&e, &identity, 400, reason.clone(), 400);

            let first_count = get_slash_count(&e, &identity);
            let first_rec = get_slash_record(&e, &identity, 0);
            let first_page = get_slash_history_page(&e, &identity, 0, 5);

            for _ in 0..10 {
                assert_eq!(get_slash_count(&e, &identity), first_count);
                assert_eq!(get_slash_record(&e, &identity, 0), first_rec);
                assert_eq!(get_slash_history_page(&e, &identity, 0, 5), first_page);
            }
        });
    }

    #[test]
    fn testutils_mirror_functions_match_module_behavior() {
        let e = Env::default();
        let cid = register(&e);
        let identity = Address::generate(&e);

        e.as_contract(&cid, || {
            let reason = Symbol::new(&e, "admin_slash");
            append_slash_history(&e, &identity, 300, reason.clone(), 300);
            append_slash_history(&e, &identity, 700, reason.clone(), 1000);

            let all = testutils::get_slash_history(&e, &identity);
            assert_eq!(all.len(), 2);
            assert_eq!(all.get(0).unwrap(), get_slash_record(&e, &identity, 0));
            assert_eq!(all.get(1).unwrap(), get_slash_record(&e, &identity, 1));
            assert_eq!(testutils::get_slash_record(&e, &identity, 1), get_slash_record(&e, &identity, 1));
            assert_eq!(testutils::get_total_slashed_from_history(&e, &identity), 1000);
        });
    }
}

