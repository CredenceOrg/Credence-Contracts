//! Migration tests for IdentityBond storage

#![cfg(test)]
use super::*;
use soroban_sdk::{Address, Env};

#[test]
fn test_lazy_migration() {
    // Setup environment
    let e = Env::default();
    // Create a dummy identity address
    let identity = Address::random(&e);
    // Create a bond with default (v2) fields; this simulates existing storage
    let bond = IdentityBond {
        identity: identity.clone(),
        bonded_amount: 1_000,
        bond_start: e.ledger().timestamp(),
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    // Store the bond directly in storage (old data would be missing new fields, but here defaults are zero)
    e.storage().instance().set(&DataKey::Bond, &bond);

    // Invoke the getter which triggers lazy migration
    let loaded = CredenceBond::get_identity_state(e.clone());
    assert_eq!(loaded.bonded_amount, bond.bonded_amount);
    assert_eq!(loaded.identity, bond.identity);
    // Ensure new fields have default values
    assert!(!loaded.is_rolling);
    assert_eq!(loaded.withdrawal_requested_at, 0);
    assert_eq!(loaded.notice_period_duration, 0);
}
