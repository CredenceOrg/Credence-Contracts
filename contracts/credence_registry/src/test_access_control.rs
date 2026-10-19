#![cfg(test)]

//! # Access Control Matrix — CredenceRegistry
//!
//! Enumerates every admin-restricted entrypoint and verifies that
//! non-admin callers and uninitialized contracts are rejected.
//!
//! ## Entrypoint Matrix
//!
//! | Entrypoint              | Required Caller     | Notes                          |
//! |-------------------------|---------------------|--------------------------------|
//! | `initialize`            | caller (self-auth)  | One-time setup                 |
//! | `register`              | admin               | Requires `require_admin_auth`  |
//! | `deactivate`            | admin               | Requires `require_admin_auth`  |
//! | `remove`                | admin               | Requires `require_admin_auth`  |
//! | `reactivate`            | admin               | Requires `require_admin_auth`  |
//! | `transfer_admin`        | admin (current)     | Requires `require_admin_auth`  |
//! | `set_bond_code_hash`    | admin               | Requires `require_admin_auth`  |
//! | `set_pause_signer`      | admin               | Admin-gated via pausable       |
//! | `set_pause_threshold`   | admin               | Admin-gated via pausable       |
//! | `approve_pause_proposal`| pause signer        | Signer-gated                   |
//! | `execute_pause_proposal`| anyone              | Permissionless (threshold-gated)|
//! | `pause`                 | pause signer / admin| Via pausable module            |
//! | `unpause`               | pause signer / admin| Via pausable module            |
//! | `register_trustless`    | bond contract (self)| Code-hash verification         |
//! | `get_*` (read-only)     | anyone              | Permissionless views           |

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

fn setup(env: &Env) -> (CredenceRegistryClient<'_>, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let attacker = Address::generate(env);

    client.initialize(&admin);

    (client, admin, attacker)
}

// ---------------------------------------------------------------------------
// Privileged entrypoint cases
// ---------------------------------------------------------------------------

struct PrivilegedCase {
    name: &'static str,
    invoke: fn(&Env, &CredenceRegistryClient<'_>, &Address),
}

fn get_privileged_cases() -> alloc::vec::Vec<PrivilegedCase> {
    alloc::vec![
        PrivilegedCase {
            name: "register",
            invoke: |env, client, caller| {
                let identity = Address::generate(env);
                let bond = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "register",
                        args: (identity.clone(), bond.clone(), true).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.register(&identity, &bond, &true);
            },
        },
        PrivilegedCase {
            name: "deactivate",
            invoke: |env, client, caller| {
                let identity = Address::generate(env);
                let bond = Address::generate(env);
                // First register via admin
                client.register(&identity, &bond, &true);
                // Then try to deactivate as caller
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "deactivate",
                        args: (identity.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.deactivate(&identity);
            },
        },
        PrivilegedCase {
            name: "remove",
            invoke: |env, client, caller| {
                let identity = Address::generate(env);
                let bond = Address::generate(env);
                // First register via admin
                client.register(&identity, &bond, &true);
                // Then try to remove as caller
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "remove",
                        args: (identity.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.remove(&identity);
            },
        },
        PrivilegedCase {
            name: "reactivate",
            invoke: |env, client, caller| {
                let identity = Address::generate(env);
                let bond = Address::generate(env);
                // Register, deactivate, then try reactivate as caller
                client.register(&identity, &bond, &true);
                client.deactivate(&identity);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "reactivate",
                        args: (identity.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.reactivate(&identity);
            },
        },
        PrivilegedCase {
            name: "transfer_admin",
            invoke: |env, client, caller| {
                let new_admin = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "transfer_admin",
                        args: (new_admin.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.transfer_admin(&new_admin);
            },
        },
        PrivilegedCase {
            name: "set_bond_code_hash",
            invoke: |env, client, caller| {
                let hash = soroban_sdk::Bytes::from_array(env, &[0u8; 32]);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_bond_code_hash",
                        args: (hash.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_bond_code_hash(&hash);
            },
        },
        PrivilegedCase {
            name: "set_pause_signer",
            invoke: |env, client, caller| {
                let signer = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_pause_signer",
                        args: (caller, signer.clone(), true).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_pause_signer(caller, &signer, &true);
            },
        },
        PrivilegedCase {
            name: "set_pause_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_pause_threshold",
                        args: (caller, 2_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_pause_threshold(caller, &2_u32);
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Every admin-restricted entrypoint panics when called by a non-admin.
#[test]
fn test_exhaustive_non_admin_rejected() {
    let env = Env::default();
    let (client, _admin, attacker) = setup(&env);

    for case in get_privileged_cases() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.invoke)(&env, &client, &attacker);
        }));

        assert!(
            res.is_err(),
            "Expected entrypoint '{}' to panic for non-admin",
            case.name
        );
    }
}

/// Every admin-restricted entrypoint panics when the contract is uninitialized.
#[test]
fn test_exhaustive_uninitialized_rejected() {
    let env = Env::default();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    for case in get_privileged_cases() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.invoke)(&env, &client, &caller);
        }));

        assert!(
            res.is_err(),
            "Expected entrypoint '{}' to panic for uninitialized contract",
            case.name
        );
    }
}

/// Admin can successfully call all admin-restricted entrypoints.
#[test]
fn test_admin_success_on_all_privileged_entrypoints() {
    let env = Env::default();
    let (client, admin, _attacker) = setup(&env);

    // register (admin)
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    let entry = client.register(&identity, &bond, &true);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond);
    assert!(entry.active);

    // deactivate (admin)
    client.deactivate(&identity);
    let entry = client.get_bond_contract(&identity);
    assert!(!entry.active);

    // reactivate (admin)
    client.reactivate(&identity);
    let entry = client.get_bond_contract(&identity);
    assert!(entry.active);

    // transfer_admin (admin)
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);

    // set_bond_code_hash (admin)
    let hash = soroban_sdk::Bytes::from_array(&env, &[1u8; 32]);
    client.set_bond_code_hash(&hash);
    let stored = client.get_bond_code_hash();
    assert_eq!(stored, hash);
}
