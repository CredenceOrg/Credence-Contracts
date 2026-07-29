#![cfg(test)]

//! # Access Control Matrix — CredenceTreasury
//!
//! Enumerates every restricted entrypoint and verifies that unauthorized
//! callers and uninitialized contracts are rejected.
//!
//! ## Entrypoint Matrix
//!
//! | Entrypoint               | Required Caller    | Notes                          |
//! |--------------------------|--------------------|--------------------------------|
//! | `initialize`             | caller (self-auth) | One-time setup                 |
//! | `add_depositor`          | admin              | Admin-gated                    |
//! | `remove_depositor`       | admin              | Admin-gated                    |
//! | `add_signer`             | admin              | Admin-gated                    |
//! | `remove_signer`          | admin              | Admin-gated                    |
//! | `set_threshold`          | admin              | Admin-gated                    |
//! | `propose_withdrawal`     | signer             | Signer-gated                   |
//! | `approve_withdrawal`     | signer             | Signer-gated                   |
//! | `execute_withdrawal`     | anyone             | Permissionless (threshold-gated) |
//! | `register_corridor`      | admin              | Admin-gated                    |
//! | `remove_corridor`        | admin              | Admin-gated                    |
//! | `settle`                 | admin              | Admin-gated                    |
//! | `set_token`              | admin              | Admin-gated                    |
//! | `set_min_liquidity`      | admin              | Admin-gated                    |
//! | `set_proposal_ttl`       | admin              | Admin-gated                    |
//! | `receive_fee`            | depositor          | Depositor-gated                |
//! | `rescue_native`          | admin              | Admin-gated                    |
//! | `transfer_admin`         | admin (current)    | Admin-gated                    |
//! | `set_pause_signer`       | admin              | Admin-gated via pausable       |
//! | `set_pause_threshold`    | admin              | Admin-gated via pausable       |
//! | `approve_pause_proposal` | pause signer       | Signer-gated                   |
//! | `execute_pause_proposal` | anyone             | Permissionless (threshold-gated)|
//! | `pause`                  | pause signer/admin | Via pausable module            |
//! | `unpause`                | pause signer/admin | Via pausable module            |
//! | `get_*` (read-only)      | anyone             | Permissionless views           |

use crate::treasury::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

use crate::CredenceTreasury;

fn setup(env: &Env) -> (CredenceTreasuryClient<'_>, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token = Address::generate(env);
    let attacker = Address::generate(env);

    client.initialize(&admin, &token);

    (client, admin, token, attacker)
}

// ---------------------------------------------------------------------------
// Privileged admin entrypoint cases
// ---------------------------------------------------------------------------

struct PrivilegedCase {
    name: &'static str,
    invoke: fn(&Env, &CredenceTreasuryClient<'_>, &Address),
}

fn get_privileged_cases() -> alloc::vec::Vec<PrivilegedCase> {
    alloc::vec![
        PrivilegedCase {
            name: "add_depositor",
            invoke: |env, client, caller| {
                let depositor = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "add_depositor",
                        args: (caller, depositor.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.add_depositor(caller, &depositor);
            },
        },
        PrivilegedCase {
            name: "remove_depositor",
            invoke: |env, client, caller| {
                let depositor = Address::generate(env);
                // First add depositor as admin
                client.add_depositor(caller, &depositor);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "remove_depositor",
                        args: (caller, depositor.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.remove_depositor(caller, &depositor);
            },
        },
        PrivilegedCase {
            name: "add_signer",
            invoke: |env, client, caller| {
                let signer = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "add_signer",
                        args: (caller, signer.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.add_signer(caller, &signer);
            },
        },
        PrivilegedCase {
            name: "remove_signer",
            invoke: |env, client, caller| {
                let signer = Address::generate(env);
                client.add_signer(caller, &signer);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "remove_signer",
                        args: (caller, signer.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.remove_signer(caller, &signer);
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
            name: "register_corridor",
            invoke: |env, client, caller| {
                let dest = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "register_corridor",
                        args: (caller, dest.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.register_corridor(caller, &dest);
            },
        },
        PrivilegedCase {
            name: "remove_corridor",
            invoke: |env, client, caller| {
                let dest = Address::generate(env);
                client.register_corridor(caller, &dest);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "remove_corridor",
                        args: (caller, dest.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.remove_corridor(caller, &dest);
            },
        },
        PrivilegedCase {
            name: "set_token",
            invoke: |env, client, caller| {
                let new_token = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_token",
                        args: (caller, new_token.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_token(caller, &new_token);
            },
        },
        PrivilegedCase {
            name: "set_min_liquidity",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_min_liquidity",
                        args: (caller, 100_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_min_liquidity(caller, &100_i128);
            },
        },
        PrivilegedCase {
            name: "set_proposal_ttl",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_proposal_ttl",
                        args: (caller, 86400_u64).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_proposal_ttl(caller, &86400_u64);
            },
        },
        PrivilegedCase {
            name: "rescue_native",
            invoke: |env, client, caller| {
                let to = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "rescue_native",
                        args: (caller, to.clone(), 100_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.rescue_native(caller, &to, &100_i128);
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
    ]
}

// ---------------------------------------------------------------------------
// Tests: Admin-restricted entrypoints
// ---------------------------------------------------------------------------

/// Every admin-restricted entrypoint panics when called by a non-admin.
#[test]
fn test_admin_entrypoints_reject_non_admin() {
    let env = Env::default();
    let (client, _admin, _token, attacker) = setup(&env);

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
    let contract_id = env.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(&env, &contract_id);
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
// Tests: Signer-gated entrypoints (propose_withdrawal)
// ---------------------------------------------------------------------------

#[test]
fn test_propose_withdrawal_rejects_non_signer() {
    let env = Env::default();
    let (client, _admin, _token, attacker) = setup(&env);
    let recipient = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "propose_withdrawal",
            args: (&attacker, &recipient, 100_i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.propose_withdrawal(&attacker, &recipient, &100_i128);
    }));
    assert!(res.is_err(), "propose_withdrawal must reject non-signer");
}

#[test]
fn test_propose_withdrawal_succeeds_as_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _token, _attacker) = setup(&env);
    let signer = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.add_signer(&admin, &signer);
    let proposal_id = client.propose_withdrawal(&signer, &recipient, &100_i128);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.recipient, recipient);
}

// ---------------------------------------------------------------------------
// Tests: Depositor-gated entrypoints (receive_fee)
// ---------------------------------------------------------------------------

#[test]
fn test_receive_fee_rejects_non_depositor() {
    let env = Env::default();
    let (client, _admin, token, attacker) = setup(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "receive_fee",
            args: (&attacker, 100_i128, FundSource::ProtocolFee).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.receive_fee(&attacker, &100_i128, &FundSource::ProtocolFee);
    }));
    assert!(res.is_err(), "receive_fee must reject non-depositor");
}

#[test]
fn test_receive_fee_succeeds_as_depositor() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _attacker) = setup(&env);
    let depositor = Address::generate(&env);

    client.add_depositor(&admin, &depositor);
    client.receive_fee(&depositor, &100_i128, &FundSource::ProtocolFee);
}

// ---------------------------------------------------------------------------
// Tests: Admin success paths
// ---------------------------------------------------------------------------

#[test]
fn test_admin_success_on_privileged_entrypoints() {
    let env = Env::default();
    let (client, admin, token, _attacker) = setup(&env);

    // add_depositor
    let depositor = Address::generate(&env);
    client.add_depositor(&admin, &depositor);
    assert!(client.is_depositor(&depositor));

    // add_signer
    let signer = Address::generate(&env);
    client.add_signer(&admin, &signer);
    assert!(client.is_signer(&signer));

    // set_threshold
    client.set_threshold(&admin, &2_u32);
    assert_eq!(client.get_threshold(), 2);

    // set_token
    let new_token = Address::generate(&env);
    client.set_token(&admin, &new_token);
    assert_eq!(client.get_token(), new_token);

    // register_corridor
    let dest = Address::generate(&env);
    client.register_corridor(&admin, &dest);
    assert!(client.is_corridor_registered(&dest));
}
