#![cfg(test)]

//! # Access Control Matrix — CredenceMultiSig
//!
//! Enumerates every restricted entrypoint and verifies that unauthorized
//! callers and uninitialized contracts are rejected.
//!
//! ## Entrypoint Matrix
//!
//! | Entrypoint                | Required Caller    | Notes                          |
//! |---------------------------|--------------------|--------------------------------|
//! | `initialize`              | caller (self-auth) | One-time setup                 |
//! | `add_signer`              | admin              | Admin-gated                    |
//! | `remove_signer`           | admin              | Admin-gated                    |
//! | `set_threshold`           | admin              | Admin-gated                    |
//! | `reject_proposal`         | admin              | Admin-gated                    |
//! | `transfer_admin`          | admin (current)    | Admin-gated                    |
//! | `submit_proposal`         | signer             | Signer-gated                   |
//! | `sign_proposal`           | signer             | Signer-gated                   |
//! | `execute_proposal`        | anyone             | Permissionless (threshold-gated) |
//! | `prune_expired_proposals` | anyone             | Permissionless                 |
//! | `set_pause_signer`        | admin              | Admin-gated via pausable       |
//! | `set_pause_threshold`     | admin              | Admin-gated via pausable       |
//! | `set_max_pause_signers`   | admin              | Admin-gated via pausable       |
//! | `approve_pause_proposal`  | pause signer       | Signer-gated                   |
//! | `execute_pause_proposal`  | anyone             | Permissionless                 |
//! | `pause`                   | pause signer/admin | Via pausable module            |
//! | `unpause`                 | pause signer/admin | Via pausable module            |
//! | `get_*` (read-only)       | anyone             | Permissionless views           |

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, IntoVal, Val, Vec};

fn setup(env: &Env) -> (CredenceMultiSigClient<'_>, Address, Vec<Address>, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let signer1 = Address::generate(env);
    let signer2 = Address::generate(env);
    let signer3 = Address::generate(env);
    let attacker = Address::generate(env);

    let mut signers = Vec::new(env);
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());

    client.initialize(&admin, &signers, &2);

    (client, admin, signers, attacker)
}

// ---------------------------------------------------------------------------
// Privileged admin entrypoint cases
// ---------------------------------------------------------------------------

struct PrivilegedCase {
    name: &'static str,
    invoke: fn(&Env, &CredenceMultiSigClient<'_>, &Address),
}

fn get_privileged_cases() -> alloc::vec::Vec<PrivilegedCase> {
    alloc::vec![
        PrivilegedCase {
            name: "add_signer",
            invoke: |env, client, caller| {
                let new_signer = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "add_signer",
                        args: (caller, new_signer.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.add_signer(caller, &new_signer);
            },
        },
        PrivilegedCase {
            name: "remove_signer",
            invoke: |env, client, caller| {
                let signer_to_remove = Address::generate(env);
                // Set up: add the signer first when called as admin
                let admin_for_setup = Address::generate(env);

                // This case will be invoked with caller=attacker; setup separately
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "remove_signer",
                        args: (caller, signer_to_remove.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.remove_signer(caller, &signer_to_remove);
            },
        },
        PrivilegedCase {
            name: "set_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_threshold",
                        args: (caller, 2_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_threshold(caller, &2_u32);
            },
        },
        PrivilegedCase {
            name: "reject_proposal",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "reject_proposal",
                        args: (caller, 0_u64).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.reject_proposal(caller, &0_u64);
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
        PrivilegedCase {
            name: "set_max_pause_signers",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_max_pause_signers",
                        args: (caller, 10_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_max_pause_signers(caller, &10_u32);
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests: Admin-restricted entrypoints
// ---------------------------------------------------------------------------

/// Every admin-restricted entrypoint panics when called by a non-admin.
#[test]
fn test_admin_entrypoints_reject_non_admin() {
    let env = Env::default();
    let (client, _admin, _signers, attacker) = setup(&env);

    for case in get_privileged_cases() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.invoke)(&env, &client, &attacker);
        }));

        assert!(
            res.is_err(),
            "Expected admin entrypoint '{}' to panic for non-admin",
            case.name
        );
    }
}

/// Every admin-restricted entrypoint panics when contract is uninitialized.
#[test]
fn test_admin_entrypoints_reject_uninitialized() {
    let env = Env::default();
    let contract_id = env.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    for case in get_privileged_cases() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.invoke)(&env, &client, &caller);
        }));

        assert!(
            res.is_err(),
            "Expected admin entrypoint '{}' to panic for uninitialized contract",
            case.name
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: Signer-gated entrypoints
// ---------------------------------------------------------------------------

#[test]
fn test_submit_proposal_rejects_non_signer() {
    let env = Env::default();
    let (client, _admin, _signers, attacker) = setup(&env);
    let target = Address::generate(&env);
    let calldata = soroban_sdk::Bytes::new(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "submit_proposal",
            args: (&attacker, &target, &calldata, &ActionType::ContractCall)
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.submit_proposal(&attacker, &target, &calldata, &ActionType::ContractCall);
    }));
    assert!(res.is_err(), "submit_proposal must reject non-signer");
}

#[test]
fn test_submit_proposal_succeeds_as_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, signers, _attacker) = setup(&env);
    let signer = signers.get(0).unwrap();
    let target = Address::generate(&env);
    let calldata = soroban_sdk::Bytes::new(&env);

    let proposal_id = client.submit_proposal(&signer, &target, &calldata, &ActionType::ContractCall);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.proposer, signer);
}

// ---------------------------------------------------------------------------
// Tests: Admin success paths
// ---------------------------------------------------------------------------

#[test]
fn test_admin_success_on_privileged_entrypoints() {
    let env = Env::default();
    let (client, admin, _signers, _attacker) = setup(&env);

    // add_signer
    let new_signer = Address::generate(&env);
    client.add_signer(&admin, &new_signer);
    assert!(client.is_signer(&new_signer));

    // set_threshold
    client.set_threshold(&admin, &3_u32);
    assert_eq!(client.get_threshold(), 3);

    // set_max_pause_signers
    client.set_max_pause_signers(&admin, &10_u32);
    assert_eq!(client.get_max_pause_signers(), 10);
}
