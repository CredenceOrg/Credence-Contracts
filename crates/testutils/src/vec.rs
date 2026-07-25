use soroban_sdk::{Env, Vec as SorobanVec};
use alloc::vec::Vec as StdVec;

pub fn deduplicate_stable<T: PartialEq + Clone>(e: &Env, v: &SorobanVec<T>) -> SorobanVec<T> {
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
