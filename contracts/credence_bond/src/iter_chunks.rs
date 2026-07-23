//! Fixed-size chunk iteration over a [`soroban_sdk::Vec`] for gas budgeting.
//!
//! Provides [`vec_chunks`], a utility for processing a `Vec` in fixed-size
//! windows. Each call returns one contiguous slice of elements and the offset
//! to pass into the next call, making it straightforward to spread work across
//! multiple transactions without exceeding the Soroban instruction budget.
//!
//! ## Why chunks?
//!
//! A Soroban transaction has a finite CPU-instruction and memory budget.
//! Naively iterating an unbounded `Vec` inside a single invocation risks
//! hitting that budget as the collection grows. The chunk pattern lets callers
//! commit a known upper bound of work per transaction and resume from where
//! they left off in the next call.
//!
//! ## Default chunk size
//!
//! [`crate::parameters::DEFAULT_CHUNK_SIZE`] (50) is the recommended value for
//! `chunk_size` when no custom sizing is required. Import it rather than
//! hard-coding the number so the entire codebase stays in sync if the default
//! ever changes.
//!
//! ## Example
//!
//! ```ignore
//! use credence_bond::iter_chunks::vec_chunks;
//! use credence_bond::parameters::DEFAULT_CHUNK_SIZE;
//! use soroban_sdk::{Env, Vec};
//!
//! let e = Env::default();
//! let mut items: Vec<u64> = Vec::new(&e);
//! for i in 0..130_u64 {
//!     items.push_back(i);
//! }
//!
//! let mut offset = 0u32;
//! loop {
//!     let (chunk, next) = vec_chunks(&e, &items, offset, DEFAULT_CHUNK_SIZE);
//!     if chunk.is_empty() {
//!         break;
//!     }
//!     // process chunk ...
//!     match next {
//!         Some(n) => offset = n,
//!         None => break,
//!     }
//! }
//! ```

use soroban_sdk::{Env, Vec};

/// Return a fixed-size chunk of `source` starting at `offset`, plus the
/// offset to use for the **next** call (or `None` when the end is reached).
///
/// # Arguments
///
/// * `e`          - Soroban [`Env`] (required by `soroban_sdk::Vec`).
/// * `source`     - The vector to iterate over. Not mutated.
/// * `offset`     - Zero-based start index of this chunk.
/// * `chunk_size` - Maximum number of items to include. Pass `0` to use
///   [`crate::parameters::DEFAULT_CHUNK_SIZE`] so callers never accidentally
///   request an empty chunk.
///
/// # Returns
///
/// A tuple `(chunk, next_offset)` where:
///
/// * `chunk` - A `Vec<T>` of at most `chunk_size` elements beginning at
///   `offset`. An **empty** vec is returned when `offset >= source.len()`.
/// * `next_offset` - `Some(offset + chunk.len())` when more elements remain;
///   `None` when the chunk reached the end of `source`.
///
/// # Panics
///
/// Never panics. An out-of-bounds `offset` produces an empty chunk.
///
/// # Gas budgeting
///
/// Each call performs at most `chunk_size` index accesses on `source`.
/// Callers can treat `chunk_size` as their per-call work unit and pick a
/// value that keeps the transaction inside the Soroban CPU budget.
pub fn vec_chunks<T>(
    e: &Env,
    source: &Vec<T>,
    offset: u32,
    chunk_size: u32,
) -> (Vec<T>, Option<u32>)
where
    T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    let effective_size = if chunk_size == 0 {
        crate::parameters::DEFAULT_CHUNK_SIZE
    } else {
        chunk_size
    };

    let total = source.len();
    let mut chunk: Vec<T> = Vec::new(e);

    if offset >= total {
        return (chunk, None);
    }

    let end = (offset + effective_size).min(total);

    for i in offset..end {
        chunk.push_back(source.get(i).unwrap());
    }

    let next = if end >= total { None } else { Some(end) };
    (chunk, next)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::vec_chunks;
    use crate::parameters::DEFAULT_CHUNK_SIZE;
    use soroban_sdk::{Env, Vec};

    fn make_vec(e: &Env, n: u32) -> Vec<u32> {
        let mut v = Vec::new(e);
        for i in 0..n {
            v.push_back(i);
        }
        v
    }

    #[test]
    fn empty_source_returns_empty_chunk_and_no_next() {
        let e = Env::default();
        let source: Vec<u32> = Vec::new(&e);
        let (chunk, next) = vec_chunks(&e, &source, 0, 10);
        assert_eq!(chunk.len(), 0);
        assert!(next.is_none());
    }

    #[test]
    fn offset_beyond_end_returns_empty_chunk_and_no_next() {
        let e = Env::default();
        let source = make_vec(&e, 5);
        let (chunk, next) = vec_chunks(&e, &source, 10, 3);
        assert_eq!(chunk.len(), 0);
        assert!(next.is_none());
    }

    #[test]
    fn first_chunk_correct_values_and_has_next() {
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
    fn last_chunk_smaller_than_chunk_size_and_no_next() {
        let e = Env::default();
        let source = make_vec(&e, 10);
        let (chunk, next) = vec_chunks(&e, &source, 9, 3);
        assert_eq!(chunk.len(), 1);
        assert_eq!(chunk.get(0).unwrap(), 9u32);
        assert!(next.is_none());
    }

    #[test]
    fn exact_divisor_final_chunk_returns_no_next() {
        let e = Env::default();
        let source = make_vec(&e, 9);
        let (chunk, next) = vec_chunks(&e, &source, 6, 3);
        assert_eq!(chunk.len(), 3);
        assert!(next.is_none());
    }

    #[test]
    fn full_iteration_visits_all_elements() {
        let e = Env::default();
        let n = 13u32;
        let source = make_vec(&e, n);
        let chunk_size = 4u32;

        let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
        let mut offset = 0u32;

        loop {
            let (chunk, next) = vec_chunks(&e, &source, offset, chunk_size);
            if chunk.is_empty() {
                break;
            }
            for i in 0..chunk.len() {
                collected.push(chunk.get(i).unwrap());
            }
            match next {
                Some(n) => offset = n,
                None => break,
            }
        }

        assert_eq!(collected.len() as u32, n);
        for (i, v) in collected.iter().enumerate() {
            assert_eq!(*v, i as u32);
        }
    }

    #[test]
    fn full_iteration_single_element_vec() {
        let e = Env::default();
        let source = make_vec(&e, 1);
        let (chunk, next) = vec_chunks(&e, &source, 0, 10);
        assert_eq!(chunk.len(), 1);
        assert_eq!(chunk.get(0).unwrap(), 0u32);
        assert!(next.is_none());
    }

    #[test]
    fn full_iteration_chunk_larger_than_source() {
        let e = Env::default();
        let source = make_vec(&e, 3);
        let (chunk, next) = vec_chunks(&e, &source, 0, 100);
        assert_eq!(chunk.len(), 3);
        assert!(next.is_none());
    }

    #[test]
    fn zero_chunk_size_uses_default() {
        let e = Env::default();
        let n = DEFAULT_CHUNK_SIZE + 10;
        let source = make_vec(&e, n);
        let (chunk, next) = vec_chunks(&e, &source, 0, 0);
        assert_eq!(chunk.len(), DEFAULT_CHUNK_SIZE);
        assert_eq!(next, Some(DEFAULT_CHUNK_SIZE));
    }

    #[test]
    fn next_offset_equals_offset_plus_chunk_len() {
        let e = Env::default();
        let source = make_vec(&e, 20);
        let (chunk, next) = vec_chunks(&e, &source, 7, 5);
        assert_eq!(next, Some(7 + chunk.len()));
    }

    #[test]
    fn chained_calls_produce_contiguous_non_overlapping_chunks() {
        let e = Env::default();
        let source = make_vec(&e, 15);
        let chunk_size = 4u32;

        let (c0, n0) = vec_chunks(&e, &source, 0, chunk_size);
        let (c1, n1) = vec_chunks(&e, &source, n0.unwrap(), chunk_size);
        let (c2, n2) = vec_chunks(&e, &source, n1.unwrap(), chunk_size);
        let (c3, n3) = vec_chunks(&e, &source, n2.unwrap(), chunk_size);

        assert_eq!(c0.len(), 4);
        assert_eq!(c1.len(), 4);
        assert_eq!(c2.len(), 4);
        assert_eq!(c3.len(), 3);
        assert!(n3.is_none());

        assert_eq!(c1.get(0).unwrap(), c0.get(c0.len() - 1).unwrap() + 1);
        assert_eq!(c2.get(0).unwrap(), c1.get(c1.len() - 1).unwrap() + 1);
        assert_eq!(c3.get(0).unwrap(), c2.get(c2.len() - 1).unwrap() + 1);
    }
}
