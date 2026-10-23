#![cfg(test)]

//! Tests verifying that the `Governable` trait implementation on
//! `CredenceMultiSig` is consistent with the inherent `get_admin` /
//! `transfer_admin` state.
//!
//! Acceptance criterion from issue #1605:
//! - After `initialize(admin, ...)`, `client.get_admin() == admin`.
//! - After `client.set_admin(&new_admin)` (with auth mocked),
//!   `client.get_admin() == new_admin`.

use crate::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (CredenceMultiSigClient<'_>, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let signer = Address::generate(env);
    let mut signers = Vec::new(env);
    signers.push_back(signer);

    client.initialize(&admin, &signers, &1);

    (client, admin)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// After `initialize`, `get_admin` must return the admin that was passed in.
#[test]
fn governable_get_admin_matches_inherent_state() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert_eq!(
        client.get_admin(),
        admin,
        "get_admin() must equal the admin set during initialize()"
    );
}

/// After `set_admin` (i.e. `transfer_admin`), `get_admin` must return the new
/// admin and no longer the old one.
#[test]
fn governable_set_admin_updates_inherent_state() {
    let env = Env::default();
    let (client, _old_admin) = setup(&env);

    let new_admin = Address::generate(&env);
    // mock_all_auths() was called in setup; transfer_admin requires auth from
    // the current admin — the mock satisfies that automatically.
    client.transfer_admin(&new_admin);

    assert_eq!(
        client.get_admin(),
        new_admin,
        "get_admin() must equal new_admin after transfer_admin()"
    );
}

/// `get_admin` is idempotent: calling it twice in a row must return the same
/// address without mutating any state.
#[test]
fn governable_get_admin_is_idempotent() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let first = client.get_admin();
    let second = client.get_admin();

    assert_eq!(first, admin);
    assert_eq!(second, admin);
    assert_eq!(first, second);
}
