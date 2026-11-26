use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SorobanVec};

/// Generates an admin address for tests.
pub fn admin(e: &Env) -> Address {
    Address::generate(e)
}

/// Generates a user address for tests.
pub fn user(e: &Env) -> Address {
    Address::generate(e)
}

/// Generates an attacker address for tests.
pub fn attacker(e: &Env) -> Address {
    Address::generate(e)
}

/// Generates `n` distinct addresses for tests.
///
/// Returns an empty vector when `n` is zero. Candidates that repeat an
/// already-generated address are discarded, so every address in the result
/// is unique.
pub fn generate_many(e: &Env, n: u32) -> SorobanVec<Address> {
    let mut addresses: SorobanVec<Address> = SorobanVec::new(e);
    while addresses.len() < n {
        let address = Address::generate(e);
        if !addresses.contains(&address) {
            addresses.push_back(address);
        }
    }
    addresses
}

/// Generates two distinct addresses for tests.
pub fn distinct_pair(e: &Env) -> (Address, Address) {
    let addresses = generate_many(e, 2);
    (addresses.get(0).unwrap(), addresses.get(1).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_many_returns_requested_length() {
        let e = Env::default();
        assert_eq!(generate_many(&e, 1).len(), 1);
        assert_eq!(generate_many(&e, 8).len(), 8);
    }

    #[test]
    fn generate_many_zero_returns_empty() {
        let e = Env::default();
        assert!(generate_many(&e, 0).is_empty());
    }

    #[test]
    fn generate_many_returns_distinct_addresses() {
        let e = Env::default();
        let addresses = generate_many(&e, 8);
        let len = addresses.len();
        for i in 0..len {
            for j in (i + 1)..len {
                assert_ne!(addresses.get(i).unwrap(), addresses.get(j).unwrap());
            }
        }
    }

    #[test]
    fn distinct_pair_returns_two_different_addresses() {
        let e = Env::default();
        let (first, second) = distinct_pair(&e);
        assert_ne!(first, second);
    }
}
