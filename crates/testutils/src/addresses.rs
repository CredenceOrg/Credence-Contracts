use soroban_sdk::{Address, Env};

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
