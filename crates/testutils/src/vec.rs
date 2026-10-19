use alloc::vec::Vec as StdVec;
use soroban_sdk::{Env, TryFromVal, Vec as SorobanVec};

pub fn deduplicate_stable<T>(e: &Env, v: &SorobanVec<T>) -> SorobanVec<T>
where
    T: PartialEq + Clone + TryFromVal<Env, soroban_sdk::Val>,
    soroban_sdk::Val: TryFromVal<Env, T>,
{
    let mut out: SorobanVec<T> = SorobanVec::new(e);
    let len = v.len();
    let mut i = 0u32;
    while i < len {
        let val = v.get(i).unwrap();
        let mut seen = false;
        let out_len = out.len();
        let mut j = 0u32;
        while j < out_len {
            if out.get(j).unwrap() == val {
                seen = true;
                break;
            }
            j += 1;
        }
        if !seen {
            out.push_back(val);
        }
        i += 1;
    }
    out
}

pub fn deduplicate_stable_alloc<T: PartialEq + Clone>(v: &StdVec<T>) -> StdVec<T> {
    let mut out: StdVec<T> = StdVec::new();
    for item in v.iter() {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out
}

/// Returns every consecutive `(v[i], v[i+1])` pair from a Soroban [`Vec`].
///
/// For a vector of length `n` this produces `n − 1` pairs.
/// Returns an empty [`alloc::vec::Vec`] for vectors with fewer than two
/// elements.
///
/// # Example
///
/// ```rust,ignore
/// use soroban_sdk::{Env, Vec};
/// use testutils::pair_iter;
///
/// let env = Env::default();
/// let mut v: Vec<u32> = Vec::new(&env);
/// v.push_back(10);
/// v.push_back(20);
/// v.push_back(30);
///
/// let pairs = pair_iter(&env, &v);
/// assert_eq!(pairs, [(10, 20), (20, 30)]);
/// ```
pub fn pair_iter<T>(_e: &Env, v: &SorobanVec<T>) -> StdVec<(T, T)>
where
    T: Clone + TryFromVal<Env, soroban_sdk::Val>,
    soroban_sdk::Val: TryFromVal<Env, T>,
{
    let len = v.len();
    let mut out: StdVec<(T, T)> = StdVec::new();
    if len < 2 {
        return out;
    }
    let mut i = 0u32;
    while i < len - 1 {
        let a = v.get(i).unwrap();
        let b = v.get(i + 1).unwrap();
        out.push((a, b));
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn make_vec(e: &Env, items: &[u32]) -> SorobanVec<u32> {
        let mut v: SorobanVec<u32> = SorobanVec::new(e);
        for &item in items {
            v.push_back(item);
        }
        v
    }

    // -----------------------------------------------------------------------
    // pair_iter — empty input
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_empty_returns_empty() {
        let e = Env::default();
        let v: SorobanVec<u32> = SorobanVec::new(&e);
        let pairs = pair_iter(&e, &v);
        assert!(pairs.is_empty());
    }

    // -----------------------------------------------------------------------
    // pair_iter — single element produces no pairs
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_single_element_returns_empty() {
        let e = Env::default();
        let v = make_vec(&e, &[42]);
        let pairs = pair_iter(&e, &v);
        assert!(pairs.is_empty());
    }

    // -----------------------------------------------------------------------
    // pair_iter — two elements produce one pair
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_two_elements_returns_one_pair() {
        let e = Env::default();
        let v = make_vec(&e, &[10, 20]);
        let pairs = pair_iter(&e, &v);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (10u32, 20u32));
    }

    // -----------------------------------------------------------------------
    // pair_iter — three elements produce two overlapping pairs
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_three_elements_returns_two_overlapping_pairs() {
        let e = Env::default();
        let v = make_vec(&e, &[1, 2, 3]);
        let pairs = pair_iter(&e, &v);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (1u32, 2u32));
        assert_eq!(pairs[1], (2u32, 3u32));
    }

    // -----------------------------------------------------------------------
    // pair_iter — n elements produce n-1 pairs
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_length_is_n_minus_one() {
        let e = Env::default();
        let items: &[u32] = &[5, 10, 15, 20, 25];
        let v = make_vec(&e, items);
        let pairs = pair_iter(&e, &v);
        assert_eq!(pairs.len(), items.len() - 1);
    }

    // -----------------------------------------------------------------------
    // pair_iter — consecutive pairs share the middle element (sliding window)
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_consecutive_pairs_overlap_correctly() {
        let e = Env::default();
        let v = make_vec(&e, &[10, 20, 30, 40]);
        let pairs = pair_iter(&e, &v);
        // Each pair's second element is the next pair's first element
        assert_eq!(pairs[0], (10u32, 20u32));
        assert_eq!(pairs[1], (20u32, 30u32));
        assert_eq!(pairs[2], (30u32, 40u32));
        // Adjacent pairs share the middle value
        for i in 0..pairs.len() - 1 {
            assert_eq!(pairs[i].1, pairs[i + 1].0);
        }
    }

    // -----------------------------------------------------------------------
    // pair_iter — duplicate values are preserved as-is
    // -----------------------------------------------------------------------

    #[test]
    fn pair_iter_duplicate_values_preserved() {
        let e = Env::default();
        let v = make_vec(&e, &[7, 7, 7]);
        let pairs = pair_iter(&e, &v);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (7u32, 7u32));
        assert_eq!(pairs[1], (7u32, 7u32));
    }
}
