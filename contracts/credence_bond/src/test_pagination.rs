//! Tests for (offset, limit) paginated reads and the MAX_QUERY_LIMIT cap.
//!
//! Coverage
//! --------
//! ### `get_subject_attestations_page`
//! - Empty subject returns empty vec
//! - Single page covering all IDs
//! - Multi-page walk covers every ID without duplicates or gaps
//! - `offset` at or beyond total returns empty vec
//! - `limit` larger than `MAX_QUERY_LIMIT` is clamped to 200
//! - `limit = 0` uses `MAX_QUERY_LIMIT` as the effective limit
//!
//! ### `get_slash_history_page`
//! - Empty history returns empty vec
//! - Single page covering all records
//! - Multi-page walk is consistent with the count returned by `get_slash_count`
//! - `limit` larger than `MAX_QUERY_LIMIT` is clamped
//!
//! ### `get_pending_claims_paginated` / `get_pending_claims_count`
//! - Empty user returns 0 count and empty page
//! - Count matches the number of claims added
//! - Multi-page walk returns every claim exactly once (no duplicates, no gaps)
//! - `limit` is clamped to `MAX_QUERY_LIMIT`
//! - `offset` at or beyond total returns empty vec
//!
//! ### Backwards-compatibility
//! - The original `get_subject_attestations` still returns all IDs unchanged.

#![cfg(test)]

extern crate std;

use crate::{
    claims::{self, ClaimType},
    parameters::MAX_QUERY_LIMIT,
    slash_history, CredenceBond, CredenceBondClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Symbol};

// ============================================================================
// Shared helpers
// ============================================================================

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

/// Register an attester and add `n` attestations for `subject`.
/// Returns the list of IDs in insertion order.
fn add_attestations(
    e: &Env,
    client: &CredenceBondClient,
    admin: &Address,
    subject: &Address,
    n: u32,
) -> std::vec::Vec<u64> {
    let attester = Address::generate(e);
    client.register_attester(&attester);
    let mut ids = std::vec::Vec::new();
    for i in 0..n {
        // Every attestation must have a unique (attester, subject, data) triple.
        let data = String::from_str(e, &std::format!("kyc:v{i}"));
        // consume nonce i for this attester
        let att = client.add_attestation(&attester, subject, &data, &(i as u64));
        ids.push(att.id);
    }
    // suppress unused warning
    let _ = admin;
    ids
}

/// Add `n` slash records directly via the internal module.
fn add_slash_records(e: &Env, identity: &Address, n: u32) {
    for i in 0..n {
        let reason = Symbol::new(e, "test");
        slash_history::append_slash_history(e, identity, (i as i128) + 1, reason, (i as i128) + 1);
    }
}

/// Add `n` pending claims directly via the internal module.
fn add_claims(e: &Env, user: &Address, n: u32) {
    for i in 0..n {
        let meta = Symbol::new(e, "m");
        claims::add_pending_claim(
            e,
            user,
            ClaimType::VerifierReward,
            100,
            i as u64,
            Some(meta),
        );
    }
}

// ============================================================================
// get_subject_attestations_page
// ============================================================================

#[test]
fn test_attestations_page_empty_subject() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let subject = Address::generate(&env);

    let page = client.get_subject_attestations_page(&subject, &0, &10);
    assert_eq!(page.len(), 0, "empty subject should return empty page");
}

#[test]
fn test_attestations_page_single_page_all_ids() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    let inserted = add_attestations(&env, &client, &admin, &subject, 5);

    let page = client.get_subject_attestations_page(&subject, &0, &10);
    assert_eq!(page.len() as usize, inserted.len());

    for (idx, expected_id) in inserted.iter().enumerate() {
        assert_eq!(page.get(idx as u32).unwrap(), *expected_id);
    }
}

#[test]
fn test_attestations_page_multipage_walk_no_duplicates() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    // 7 attestations, page size 3 → pages of [3, 3, 1]
    let inserted = add_attestations(&env, &client, &admin, &subject, 7);

    let mut all_seen: std::vec::Vec<u64> = std::vec::Vec::new();
    let mut offset: u32 = 0;
    let page_size: u32 = 3;

    loop {
        let page = client.get_subject_attestations_page(&subject, &offset, &page_size);
        if page.is_empty() {
            break;
        }
        for i in 0..page.len() {
            all_seen.push(page.get(i).unwrap());
        }
        offset += page.len();
    }

    assert_eq!(all_seen.len(), inserted.len(), "total IDs must match");
    // order must match insertion order
    for (i, id) in inserted.iter().enumerate() {
        assert_eq!(all_seen[i], *id, "ID at position {i} must match");
    }
}

#[test]
fn test_attestations_page_offset_at_total_returns_empty() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    add_attestations(&env, &client, &admin, &subject, 3);
    // offset == total → empty
    let page = client.get_subject_attestations_page(&subject, &3, &10);
    assert_eq!(page.len(), 0);
    // offset > total → also empty
    let page = client.get_subject_attestations_page(&subject, &99, &10);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_attestations_page_limit_clamped_to_max_query_limit() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    // Insert more items than MAX_QUERY_LIMIT to verify the cap is applied.
    // We can only call add_attestation MAX_ATTESTATIONS times, but 210 is fine.
    let n = (MAX_QUERY_LIMIT + 10) as u32; // 210
    add_attestations(&env, &client, &admin, &subject, n);

    // Passing a limit larger than MAX_QUERY_LIMIT must not return more than MAX_QUERY_LIMIT.
    let oversized_limit = MAX_QUERY_LIMIT + 100;
    let page = client.get_subject_attestations_page(&subject, &0, &oversized_limit);
    assert!(
        page.len() <= MAX_QUERY_LIMIT,
        "page len {} must be <= MAX_QUERY_LIMIT {}",
        page.len(),
        MAX_QUERY_LIMIT
    );
    assert_eq!(
        page.len(),
        MAX_QUERY_LIMIT,
        "should return exactly MAX_QUERY_LIMIT items when more are available"
    );
}

#[test]
fn test_attestations_page_limit_zero_uses_max_query_limit() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    let n = (MAX_QUERY_LIMIT + 5) as u32;
    add_attestations(&env, &client, &admin, &subject, n);

    // limit = 0 should behave the same as limit = MAX_QUERY_LIMIT
    let page = client.get_subject_attestations_page(&subject, &0, &0);
    assert_eq!(page.len(), MAX_QUERY_LIMIT);
}

#[test]
fn test_get_subject_attestations_backwards_compat() {
    // The original unbounded entrypoint must still work and return all IDs.
    let env = Env::default();
    let (client, admin) = setup(&env);
    let subject = Address::generate(&env);

    let inserted = add_attestations(&env, &client, &admin, &subject, 8);

    let all = client.get_subject_attestations(&subject);
    assert_eq!(all.len() as usize, inserted.len());
}

// ============================================================================
// get_slash_history_page
// ============================================================================

#[test]
fn test_slash_history_page_empty_identity() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);

    let page = client.get_slash_history_page(&identity, &0, &10);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_slash_history_page_single_page() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);

    add_slash_records(&env, &identity, 5);

    let page = client.get_slash_history_page(&identity, &0, &10);
    assert_eq!(page.len(), 5, "all 5 records should fit in one page");

    // Records are in insertion order
    for i in 0..5u32 {
        let record = page.get(i).unwrap();
        assert_eq!(record.slash_amount, (i as i128) + 1);
    }
}

#[test]
fn test_slash_history_page_multipage_walk() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);

    let n = 11u32;
    add_slash_records(&env, &identity, n);

    let mut collected: std::vec::Vec<i128> = std::vec::Vec::new();
    let mut offset = 0u32;
    let page_size = 4u32;

    loop {
        let page = client.get_slash_history_page(&identity, &offset, &page_size);
        if page.is_empty() {
            break;
        }
        for i in 0..page.len() {
            collected.push(page.get(i).unwrap().slash_amount);
        }
        offset += page.len();
    }

    assert_eq!(collected.len(), n as usize);
    // Amounts should be 1, 2, ..., n in order
    for (i, amt) in collected.iter().enumerate() {
        assert_eq!(*amt, (i as i128) + 1);
    }
}

#[test]
fn test_slash_history_page_limit_clamped() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);

    // Insert MAX_QUERY_LIMIT + 5 records
    let n = MAX_QUERY_LIMIT + 5;
    add_slash_records(&env, &identity, n);

    let page = client.get_slash_history_page(&identity, &0, &(MAX_QUERY_LIMIT + 50));
    assert_eq!(
        page.len(),
        MAX_QUERY_LIMIT,
        "limit must be clamped to MAX_QUERY_LIMIT"
    );
}

#[test]
fn test_slash_history_page_offset_beyond_count_returns_empty() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);

    add_slash_records(&env, &identity, 3);

    let page = client.get_slash_history_page(&identity, &3, &10);
    assert_eq!(page.len(), 0);
    let page = client.get_slash_history_page(&identity, &100, &10);
    assert_eq!(page.len(), 0);
}

// ============================================================================
// get_pending_claims_paginated / get_pending_claims_count
// ============================================================================

#[test]
fn test_claims_count_empty_user() {
    let env = Env::default();

    let user = Address::generate(&env);
    let count = claims::get_pending_claims_count(&env, &user);
    assert_eq!(count, 0);
}

#[test]
fn test_claims_count_matches_adds() {
    let env = Env::default();
    let user = Address::generate(&env);

    add_claims(&env, &user, 7);
    assert_eq!(claims::get_pending_claims_count(&env, &user), 7);
}

#[test]
fn test_claims_paginated_empty_user() {
    let env = Env::default();
    let user = Address::generate(&env);

    let page = claims::get_pending_claims_paginated(&env, &user, 0, 10);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_claims_paginated_single_page() {
    let env = Env::default();
    let user = Address::generate(&env);

    add_claims(&env, &user, 5);

    let page = claims::get_pending_claims_paginated(&env, &user, 0, 10);
    assert_eq!(page.len(), 5);
}

#[test]
fn test_claims_paginated_multipage_walk_no_gaps_no_duplicates() {
    let env = Env::default();
    let user = Address::generate(&env);
    let n = 22u32;
    add_claims(&env, &user, n);

    let mut collected: std::vec::Vec<u64> = std::vec::Vec::new();
    let mut offset = 0u32;
    let page_size = 7u32;

    loop {
        let page = claims::get_pending_claims_paginated(&env, &user, offset, page_size);
        if page.is_empty() {
            break;
        }
        for i in 0..page.len() {
            collected.push(page.get(i).unwrap().claim_id);
        }
        offset += page.len();
    }

    assert_eq!(
        collected.len(),
        n as usize,
        "multi-page walk must cover all claims"
    );
    // All IDs should be unique (no duplicates)
    let mut sorted = collected.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), n as usize, "no duplicate claim IDs");
}

#[test]
fn test_claims_paginated_limit_clamped_to_max_query_limit() {
    let env = Env::default();
    let user = Address::generate(&env);

    let n = MAX_QUERY_LIMIT + 10;
    add_claims(&env, &user, n);

    let page = claims::get_pending_claims_paginated(&env, &user, 0, MAX_QUERY_LIMIT + 100);
    assert_eq!(
        page.len(),
        MAX_QUERY_LIMIT,
        "limit must be clamped to MAX_QUERY_LIMIT"
    );
}

#[test]
fn test_claims_paginated_limit_zero_uses_max_query_limit() {
    let env = Env::default();
    let user = Address::generate(&env);

    let n = MAX_QUERY_LIMIT + 5;
    add_claims(&env, &user, n);

    let page = claims::get_pending_claims_paginated(&env, &user, 0, 0);
    assert_eq!(page.len(), MAX_QUERY_LIMIT);
}

#[test]
fn test_claims_paginated_offset_at_total_returns_empty() {
    let env = Env::default();
    let user = Address::generate(&env);

    add_claims(&env, &user, 4);

    let page = claims::get_pending_claims_paginated(&env, &user, 4, 10);
    assert_eq!(page.len(), 0);
    let page = claims::get_pending_claims_paginated(&env, &user, 99, 10);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_claims_paginated_is_pure_read() {
    // get_pending_claims_paginated must not mutate the stored claims.
    let env = Env::default();
    let user = Address::generate(&env);

    add_claims(&env, &user, 10);

    let count_before = claims::get_pending_claims_count(&env, &user);
    let _ = claims::get_pending_claims_paginated(&env, &user, 0, 5);
    let _ = claims::get_pending_claims_paginated(&env, &user, 5, 5);
    let count_after = claims::get_pending_claims_count(&env, &user);

    assert_eq!(
        count_before, count_after,
        "paginated read must not remove claims"
    );
}
