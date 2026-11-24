//! Regression tests for the deterministic-ordering guarantee of every
//! list-returning read in `credence_bond`.
//!
//! Each paginated / chunked read must satisfy the pagination contract: walking
//! the full collection one page at a time returns every entry exactly once
//! (no duplicates, no omissions), and the concatenated order is the stable
//! total order of the collection's natural key.
//!
//! Ordering keys per API:
//! - `claims::get_pending_claims_paginated` (offset/limit) -> ascending `claim_id`
//! - `claims::get_pending_claims_page`      (cursor)       -> ascending `claim_id`
//! - `slash_history` index reads                           -> ascending record index
//! - `iter_chunks::vec_chunks`                             -> the source `Vec` order
//!
//! `claim_id` and the slash record index are both drawn from monotonically
//! increasing counters, so the stored insertion order already coincides with the
//! ascending-key order these tests lock in.

#![cfg(test)]

extern crate std;

use crate::{
    claims::{self, ClaimType, PendingClaim},
    iter_chunks::vec_chunks,
    slash_history, DataKey,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol, Vec};

// ============================================================================
// Shared helpers
// ============================================================================

/// Add `n` pending claims for `user` via the public writer. `claim_id` is
/// assigned from the monotonic counter, so the returned ids are `1..=n` in
/// insertion order.
fn add_claims_monotonic(e: &Env, user: &Address, n: u32) -> std::vec::Vec<u64> {
    let mut ids = std::vec::Vec::new();
    for i in 0..n {
        let id = claims::add_pending_claim(
            e,
            user,
            ClaimType::VerifierReward,
            100 + (i as i128),
            i as u64,
            Some(Symbol::new(e, "m")),
        );
        ids.push(id);
    }
    ids
}

/// Build a `PendingClaim` directly (used to seed scrambled storage order).
fn raw_claim(e: &Env, id: u64) -> PendingClaim {
    PendingClaim {
        claim_id: id,
        claim_type: ClaimType::VerifierReward,
        amount: (id as i128) * 10,
        created_at: id,
        expires_at: 0,
        source_id: id,
        metadata: Symbol::new(e, ""),
        processed: false,
    }
}

/// Walk every page of `get_pending_claims_paginated` (offset/limit) and return
/// the concatenated `claim_id`s in page order.
fn walk_offset_limit(e: &Env, user: &Address, page_size: u32) -> std::vec::Vec<u64> {
    let mut collected = std::vec::Vec::new();
    let mut offset = 0u32;
    loop {
        let page = claims::get_pending_claims_paginated(e, user, offset, page_size);
        if page.is_empty() {
            break;
        }
        for i in 0..page.len() {
            collected.push(page.get(i).unwrap().claim_id);
        }
        offset += page.len();
    }
    collected
}

/// Walk every page of the `get_pending_claims_page` cursor API and return the
/// concatenated `claim_id`s in page order.
fn walk_cursor(e: &Env, user: &Address, page_size: u32) -> std::vec::Vec<u64> {
    let mut collected = std::vec::Vec::new();
    let mut cursor = 0u64;
    loop {
        let (page, next) = claims::get_pending_claims_page(e, user, cursor, page_size);
        if page.is_empty() {
            break;
        }
        for i in 0..page.len() {
            collected.push(page.get(i).unwrap().claim_id);
        }
        match next {
            Some(c) => cursor = c,
            None => break,
        }
    }
    collected
}

/// Assert the walk covered `expected` exactly once each (no duplicates, no
/// omissions) and that the concatenated order is strictly ascending.
fn assert_ascending_complete(collected: &std::vec::Vec<u64>, expected_sorted: &std::vec::Vec<u64>) {
    let mut dedup = collected.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        collected.len(),
        "no id may appear on more than one page"
    );
    assert_eq!(
        &dedup, expected_sorted,
        "pages must cover exactly the inserted id set (no omissions)"
    );
    assert!(
        collected.windows(2).all(|w| w[0] < w[1]),
        "concatenated page order must be strictly ascending by key"
    );
}

/// Assert the walk covered `expected` exactly once each, without constraining
/// the returned order (used when storage order is deliberately scrambled).
fn assert_complete_multiset(collected: &std::vec::Vec<u64>, expected_sorted: &std::vec::Vec<u64>) {
    let mut dedup = collected.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        collected.len(),
        "no id may appear on more than one page"
    );
    assert_eq!(
        &dedup, expected_sorted,
        "pages must cover exactly the stored id set (no omissions)"
    );
}

// ============================================================================
// claims::get_pending_claims_paginated  (offset / limit)
// ============================================================================

#[test]
fn test_claims_offset_limit_walk_ascending_no_dup_no_gap() {
    let env = Env::default();
    let user = Address::generate(&env);

    // 23 does not divide evenly by the page size of 5 -> uneven final page.
    let n = 23u32;
    add_claims_monotonic(&env, &user, n);
    let expected: std::vec::Vec<u64> = (1..=n as u64).collect();

    let collected = walk_offset_limit(&env, &user, 5);
    assert_ascending_complete(&collected, &expected);
}

#[test]
fn test_claims_offset_limit_complete_under_scrambled_storage() {
    // Offset/limit indexing must return every stored claim exactly once
    // regardless of the stored order. We seed the claim vector in descending
    // claim_id order (the opposite of the production insertion invariant) and
    // confirm the walk still omits and duplicates nothing.
    let env = Env::default();
    let user = Address::generate(&env);

    let n = 12u64;
    let mut scrambled: Vec<PendingClaim> = Vec::new(&env);
    let mut id = n;
    loop {
        scrambled.push_back(raw_claim(&env, id));
        if id == 1 {
            break;
        }
        id -= 1;
    }
    env.storage()
        .persistent()
        .set(&DataKey::PendingClaims(user.clone()), &scrambled);

    let expected: std::vec::Vec<u64> = (1..=n).collect();
    let collected = walk_offset_limit(&env, &user, 4);
    assert_complete_multiset(&collected, &expected);
    assert_eq!(
        collected.len() as u64,
        n,
        "walk must cover every stored claim"
    );
}

// ============================================================================
// claims::get_pending_claims_page  (cursor)
// ============================================================================

#[test]
fn test_claims_cursor_walk_ascending_no_dup_no_gap() {
    let env = Env::default();
    let user = Address::generate(&env);

    let n = 23u32;
    add_claims_monotonic(&env, &user, n);
    let expected: std::vec::Vec<u64> = (1..=n as u64).collect();

    let collected = walk_cursor(&env, &user, 5);
    assert_ascending_complete(&collected, &expected);
}

#[test]
fn test_claims_cursor_and_offset_limit_agree() {
    // Both paginated reads over the same collection must yield the identical
    // ordered id sequence.
    let env = Env::default();
    let user = Address::generate(&env);

    let n = 17u32;
    add_claims_monotonic(&env, &user, n);

    let via_offset = walk_offset_limit(&env, &user, 6);
    let via_cursor = walk_cursor(&env, &user, 6);
    assert_eq!(
        via_offset, via_cursor,
        "cursor and offset/limit pagination must produce the same order"
    );
}

// ============================================================================
// slash_history  (index-keyed records)
// ============================================================================

#[test]
fn test_slash_history_index_order_ascending_no_dup_no_gap() {
    let env = Env::default();
    let identity = Address::generate(&env);

    // Append 13 records; amount i+1 encodes the insertion index so we can assert
    // the read order matches the write order.
    let n = 13u32;
    for i in 0..n {
        slash_history::append_slash_history(
            &env,
            &identity,
            (i as i128) + 1,
            Symbol::new(&env, "r"),
            (i as i128) + 1,
        );
    }

    assert_eq!(slash_history::get_slash_count(&env, &identity), n);

    let history = slash_history::testutils::get_slash_history(&env, &identity);
    assert_eq!(
        history.len(),
        n,
        "history must contain every appended record"
    );

    let mut amounts = std::vec::Vec::new();
    for r in history.iter() {
        amounts.push(r.slash_amount);
    }
    // Records read back in ascending index order == ascending amount 1..=n.
    let expected: std::vec::Vec<i128> = (1..=n as i128).collect();
    assert_eq!(
        amounts, expected,
        "records must read back in insertion index order"
    );

    // The indexed accessor agrees with the full-history order at every slot.
    for i in 0..n {
        let record = slash_history::testutils::get_slash_record(&env, &identity, i);
        assert_eq!(record.slash_amount, (i as i128) + 1);
    }
}

// ============================================================================
// iter_chunks::vec_chunks  (order-preserving chunking)
// ============================================================================

#[test]
fn test_vec_chunks_out_of_order_preserves_order_no_dup_no_omit() {
    // vec_chunks must reproduce the source order exactly and touch every element
    // once, even when the source values are not themselves sorted.
    let env = Env::default();

    let scrambled = [50u64, 10, 40, 20, 30, 5, 45, 15, 35, 25, 60];
    let mut source: Vec<u64> = Vec::new(&env);
    for v in scrambled.iter() {
        source.push_back(*v);
    }

    let mut collected = std::vec::Vec::new();
    let mut offset = 0u32;
    // Chunk size 4 does not divide 11 -> exercises the uneven final chunk.
    loop {
        let (chunk, next) = vec_chunks(&env, &source, offset, 4);
        if chunk.is_empty() {
            break;
        }
        for i in 0..chunk.len() {
            collected.push(chunk.get(i).unwrap());
        }
        match next {
            Some(o) => offset = o,
            None => break,
        }
    }

    // Concatenation is exactly the source order (deterministic, order-preserving).
    let expected: std::vec::Vec<u64> = scrambled.to_vec();
    assert_eq!(
        collected, expected,
        "chunks must preserve source order exactly"
    );

    // And as a multiset, every element appears exactly once (no dup, no omit).
    let mut dedup = collected.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), collected.len(), "no element chunked twice");
    assert_eq!(dedup.len(), scrambled.len(), "no element omitted");
}
