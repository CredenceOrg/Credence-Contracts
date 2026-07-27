//! # Guards
//!
//! Reusable pre-condition helpers that replace repeated inline `panic_with_error!`
//! patterns throughout the contract entry-points.
//!
//! ## Design notes
//! - Every function is a pure guard — it either returns successfully or panics
//!   with a typed [`ContractError`]; no side-effects beyond the panic path.
//! - `#![no_std]` discipline is preserved: only `soroban_sdk` primitives are used.
//! - These helpers are intentionally kept in a separate module so reviewers have
//!   a single, auditable location for all pre-condition logic.

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Env};

use crate::{bump_instance_ttl, DataKey, IdentityBond};

/// Verify that `admin` matches the stored admin address.
///
/// Reads `DataKey::Admin` from instance storage and checks it against
/// the supplied `admin` address.
///
/// # Errors
/// - Panics with [`ContractError::NotInitialized`] when the contract has not
///   been initialized (no admin stored).
/// - Panics with [`ContractError::NotAdmin`] when `admin` does not match the
///   stored admin.
///
/// # Example
/// ```ignore
/// pub fn admin_only_fn(e: Env, admin: Address) {
///     admin.require_auth();
///     guards::require_admin(&e, &admin);
///     // … admin-only logic …
/// }
/// ```
pub fn require_admin(e: &Env, admin: &Address) {
    credence_errors::require_admin!(e, admin, DataKey::Admin);
}

/// Load the bond from instance storage and bump the TTL.
///
/// Reads `DataKey::Bond` from instance storage, bumps the instance TTL via
/// [`bump_instance_ttl`], and returns the bond.
///
/// # Errors
/// - Panics with [`ContractError::BondNotFound`] when no bond has been stored.
///
/// # Example
/// ```ignore
/// pub fn read_bond_fn(e: Env) -> IdentityBond {
///     let bond = guards::load_bond(&e);
///     bond
/// }
/// ```
pub fn load_bond(e: &Env) -> IdentityBond {
    let bond: IdentityBond = e
        .storage()
        .instance()
        .get(&DataKey::Bond)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
    bump_instance_ttl(e);
    bond
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    use crate::CredenceBond;

    // ── require_admin ──────────────────────────────────────────────────────

    #[test]
    fn require_admin_succeeds_for_stored_admin() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let admin = Address::generate(&e);

        // Initialize sets DataKey::Admin.
        let client = crate::CredenceBondClient::new(&e, &contract_id);
        client.initialize(&admin, &None);

        // Guard must not panic when the correct admin is supplied.
        e.as_contract(&contract_id, || {
            require_admin(&e, &admin);
        });
    }

    #[test]
    #[should_panic]
    fn require_admin_panics_for_wrong_caller() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let admin = Address::generate(&e);
        let impostor = Address::generate(&e);

        let client = crate::CredenceBondClient::new(&e, &contract_id);
        client.initialize(&admin, &None);

        // Supplying the wrong address must panic (NotAdmin).
        e.as_contract(&contract_id, || {
            require_admin(&e, &impostor);
        });
    }

    #[test]
    #[should_panic]
    fn require_admin_panics_when_not_initialized() {
        let e = Env::default();
        let contract_id = e.register(CredenceBond, ());
        let any = Address::generate(&e);

        // No initialize call — must panic (NotInitialized).
        e.as_contract(&contract_id, || {
            require_admin(&e, &any);
        });
    }

    // ── load_bond ──────────────────────────────────────────────────────────

    #[test]
    fn load_bond_returns_stored_bond() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register(CredenceBond, ());
        let client = crate::CredenceBondClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let identity = Address::generate(&e);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

        e.as_contract(&contract_id, || {
            let bond = load_bond(&e);
            assert_eq!(bond.bonded_amount, 1000);
            assert_eq!(bond.identity, identity);
        });
    }

    #[test]
    #[should_panic]
    fn load_bond_panics_when_no_bond_exists() {
        let e = Env::default();
        let contract_id = e.register(CredenceBond, ());

        // No bond stored — must panic (BondNotFound).
        e.as_contract(&contract_id, || {
            load_bond(&e);
        });
    }
}
