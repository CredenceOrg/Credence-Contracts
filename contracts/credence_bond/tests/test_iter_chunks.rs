//! Integration tests for `iter_chunks::vec_chunks`.
//!
//! Covers three boundary classes — **empty**, **single**, **many** — with
//! both happy-path and sad-path (offset-out-of-range) cases.

use credence_bond::iter_chunks::vec_chunks;
use credence_bond::parameters::DEFAULT_CHUNK_SIZE;
use credence_bond::soroban_sdk::{Env, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_vec(e: &Env, n: u32) -> Vec<u32> {
    let mut v = Vec::new(e);
    for i in 0..n {
        v.push_back(i);
    }
    v
}

// ---------------------------------------------------------------------------
// EMPTY source (boundary: len == 0)
// ---------------------------------------------------------------------------

#[test]
fn empty_source_at_offset_zero_returns_empty_chunk_and_no_next() {
    let e = Env::default();
    let source: Vec<u32> = Vec::new(&e);
    let (chunk, next) = vec_chunks(&e, &source, 0, 10);
    assert_eq!(chunk.len(), 0);
    assert!(next.is_none());
}

#[test]
fn empty_source_at_nonzero_offset_returns_empty_chunk_and_no_next() {
    let e = Env::default();
    let source: Vec<u32> = Vec::new(&e);
    let (chunk, next) = vec_chunks(&e, &source, 5, 10);
    assert_eq!(chunk.len(), 0);
    assert!(next.is_none());
}

// ---------------------------------------------------------------------------
// SINGLE element (boundary: len == 1)
// ---------------------------------------------------------------------------

#[test]
fn single_element_at_offset_zero_returns_element_and_no_next() {
    let e = Env::default();
    let source = make_vec(&e, 1);
    let (chunk, next) = vec_chunks(&e, &source, 0, 10);
    assert_eq!(chunk.len(), 1);
    assert_eq!(chunk.get(0).unwrap(), 0u32);
    assert!(next.is_none());
}

#[test]
fn single_element_offset_one_is_out_of_range_returns_empty_and_no_next() {
    // sad path: offset exactly equals len
    let e = Env::default();
    let source = make_vec(&e, 1);
    let (chunk, next) = vec_chunks(&e, &source, 1, 10);
    assert_eq!(chunk.len(), 0);
    assert!(next.is_none());
}

#[test]
fn single_element_chunk_size_larger_than_source_returns_full_vec() {
    let e = Env::default();
    let source = make_vec(&e, 1);
    let (chunk, next) = vec_chunks(&e, &source, 0, 100);
    assert_eq!(chunk.len(), 1);
    assert!(next.is_none());
}

// ---------------------------------------------------------------------------
// MANY elements (boundary: len > 1)
// ---------------------------------------------------------------------------

#[test]
fn many_elements_first_chunk_has_correct_values_and_next_offset() {
    let e = Env::default();
    let source = make_vec(&e, 10);
    let (chunk, next) = vec_chunks(&e, &source, 0, 3);
    assert_eq!(chunk.len(), 3);
    assert_eq!(chunk.get(0).unwrap(), 0u32);
    assert_eq!(chunk.get(1).unwrap(), 1u32);
    assert_eq!(chunk.get(2).unwrap(), 2u32);
    assert_eq!(next, Some(3));
}

#[test]
fn many_elements_last_chunk_is_smaller_than_chunk_size_and_has_no_next() {
    // 10 items, chunk_size 3: last chunk starts at offset 9, contains 1 item
    let e = Env::default();
    let source = make_vec(&e, 10);
    let (chunk, next) = vec_chunks(&e, &source, 9, 3);
    assert_eq!(chunk.len(), 1);
    assert_eq!(chunk.get(0).unwrap(), 9u32);
    assert!(next.is_none());
}

#[test]
fn many_elements_exact_multiple_final_chunk_has_no_next() {
    // 9 items, chunk_size 3: chunks at 0, 3, 6 — last is full and done
    let e = Env::default();
    let source = make_vec(&e, 9);
    let (chunk, next) = vec_chunks(&e, &source, 6, 3);
    assert_eq!(chunk.len(), 3);
    assert!(next.is_none());
}

#[test]
fn many_elements_offset_beyond_end_returns_empty_chunk_and_no_next() {
    // sad path: offset well past the end
    let e = Env::default();
    let source = make_vec(&e, 5);
    let (chunk, next) = vec_chunks(&e, &source, 100, 3);
    assert_eq!(chunk.len(), 0);
    assert!(next.is_none());
}

#[test]
fn many_elements_full_loop_visits_every_element_exactly_once() {
    let e = Env::default();
    let n = 17u32;
    let source = make_vec(&e, n);
    let chunk_size = 5u32;

    let mut count = 0u32;
    let mut offset = 0u32;
    let mut prev_last: Option<u32> = None;

    loop {
        let (chunk, next) = vec_chunks(&e, &source, offset, chunk_size);
        if chunk.is_empty() {
            break;
        }

        // Each chunk must be contiguous with the previous one
        if let Some(last) = prev_last {
            assert_eq!(chunk.get(0).unwrap(), last + 1);
        }

        count += chunk.len();
        prev_last = Some(chunk.get(chunk.len() - 1).unwrap());

        match next {
            Some(n) => offset = n,
            None => break,
        }
    }

    assert_eq!(count, n);
}

#[test]
fn many_elements_zero_chunk_size_falls_back_to_default() {
    let e = Env::default();
    let n = DEFAULT_CHUNK_SIZE + 5;
    let source = make_vec(&e, n);
    let (chunk, next) = vec_chunks(&e, &source, 0, 0);
    assert_eq!(chunk.len(), DEFAULT_CHUNK_SIZE);
    assert_eq!(next, Some(DEFAULT_CHUNK_SIZE));
}

#[test]
fn many_elements_next_offset_equals_offset_plus_chunk_length() {
    let e = Env::default();
    let source = make_vec(&e, 20);
    let (chunk, next) = vec_chunks(&e, &source, 3, 7);
    assert_eq!(next, Some(3 + chunk.len()));
}

#[test]
fn many_elements_chunk_larger_than_source_returns_whole_vec_and_no_next() {
    let e = Env::default();
    let source = make_vec(&e, 4);
    let (chunk, next) = vec_chunks(&e, &source, 0, 1000);
    assert_eq!(chunk.len(), 4);
    assert!(next.is_none());
}
