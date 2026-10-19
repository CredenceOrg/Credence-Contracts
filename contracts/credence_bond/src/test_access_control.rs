#![cfg(test)]
extern crate alloc;
extern crate std;
use crate::access_control::{
    add_verifier_role, get_admin, is_admin, is_verifier, remove_verifier_role,
    require_admin, require_admin_or_verifier, require_identity_owner, require_verifier,
};
use crate::{CredenceBond, CredenceBondClient, DataKey};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

fn setup(env: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let user = Address::generate(env);
    let attacker = Address::generate(env);

    client.initialize(&admin, &None);

    (client, admin, user, attacker)
}

struct PrivilegedCase {
    name: &'static str,
    invoke: fn(&Env, &CredenceBondClient<'_>, &Address),
}

fn invoke_transfer_admin(env: &Env, client: &CredenceBondClient<'_>, caller: &Address) {
    let new_admin = Address::generate(env);
    let args: soroban_sdk::Vec<soroban_sdk::Val> =
        (caller.clone(), new_admin.clone()).into_val(env);
    env.mock_auths(&[
        soroban_sdk::testutils::MockAuth {
            address: caller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "transfer_admin",
                args: args.clone(),
                sub_invokes: &[],
            },
        },
        soroban_sdk::testutils::MockAuth {
            address: &new_admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "transfer_admin",
                args,
                sub_invokes: &[],
            },
        },
    ]);
    client.transfer_admin(caller, &new_admin);
}

fn get_privileged_cases() -> alloc::vec::Vec<PrivilegedCase> {
    alloc::vec![
        PrivilegedCase {
            name: "set_early_exit_config",
            invoke: |env, client, caller| {
                let treasury = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_early_exit_config",
                        args: (caller, treasury.clone(), 500_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_early_exit_config(caller, &treasury, &500_u32);
            },
        },
        PrivilegedCase {
            name: "register_attester",
            invoke: |env, client, caller| {
                let attester = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "register_attester",
                        args: (attester.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.register_attester(&attester);
            },
        },
        PrivilegedCase {
            name: "unregister_attester",
            invoke: |env, client, caller| {
                let attester = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "unregister_attester",
                        args: (attester.clone(),).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.unregister_attester(&attester);
            },
        },
        PrivilegedCase {
            name: "set_attester_stake",
            invoke: |env, client, caller| {
                let attester = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_attester_stake",
                        args: (caller, attester.clone(), 100_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_attester_stake(caller, &attester, &100_i128);
            },
        },
        PrivilegedCase {
            name: "set_weight_config",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_weight_config",
                        args: (caller, 100_u32, 1000_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_weight_config(caller, &100_u32, &1000_u32);
            },
        },
        PrivilegedCase {
            name: "slash",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "slash",
                        args: (caller, 100_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.slash(caller, &100_i128);
            },
        },
        PrivilegedCase {
            name: "slash_bond",
            invoke: |env, client, caller| {
                let salt = soroban_sdk::Bytes::new(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "slash_bond",
                        args: (caller, 100_i128, salt.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.slash_bond(caller, &100_i128, &salt);
            },
        },
        PrivilegedCase {
            name: "collect_fees",
            invoke: |env, client, caller| {
                let salt = soroban_sdk::Bytes::new(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "collect_fees",
                        args: (caller, salt.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.collect_fees(caller, &salt);
            },
        },
        PrivilegedCase {
            name: "transfer_admin",
            invoke: invoke_transfer_admin,
        },
        PrivilegedCase {
            name: "set_accepted_tokens",
            invoke: |env, client, caller| {
                let token = Address::generate(env);
                let tokens = soroban_sdk::vec![env, token];
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_accepted_tokens",
                        args: (caller, tokens.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_accepted_tokens(caller, &tokens);
            },
        },
        PrivilegedCase {
            name: "set_token",
            invoke: |env, client, caller| {
                let token = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_token",
                        args: (caller, token.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_token(caller, &token);
            },
        },
        PrivilegedCase {
            name: "set_borrow_frozen",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_borrow_frozen",
                        args: (caller, true).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_borrow_frozen(caller, &true);
            },
        },
        PrivilegedCase {
            name: "set_protocol_fee_bps",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_protocol_fee_bps",
                        args: (caller, 300_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_protocol_fee_bps(caller, &300_u32);
            },
        },
        PrivilegedCase {
            name: "set_attestation_fee_bps",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_attestation_fee_bps",
                        args: (caller, 200_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_attestation_fee_bps(caller, &200_u32);
            },
        },
        PrivilegedCase {
            name: "set_withdrawal_cooldown_secs",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_withdrawal_cooldown_secs",
                        args: (caller, 3600_u64).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_withdrawal_cooldown_secs(caller, &3600_u64);
            },
        },
        PrivilegedCase {
            name: "set_slash_cooldown_secs",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_slash_cooldown_secs",
                        args: (caller, 3600_u64).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_slash_cooldown_secs(caller, &3600_u64);
            },
        },
        PrivilegedCase {
            name: "set_bronze_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_bronze_threshold",
                        args: (caller, 1000_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_bronze_threshold(caller, &1000_i128);
            },
        },
        PrivilegedCase {
            name: "set_silver_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_silver_threshold",
                        args: (caller, 5000_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_silver_threshold(caller, &5000_i128);
            },
        },
        PrivilegedCase {
            name: "set_gold_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_gold_threshold",
                        args: (caller, 10000_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_gold_threshold(caller, &10000_i128);
            },
        },
        PrivilegedCase {
            name: "set_platinum_threshold",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_platinum_threshold",
                        args: (caller, 50000_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_platinum_threshold(caller, &50000_i128);
            },
        },
        PrivilegedCase {
            name: "set_max_leverage",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_max_leverage",
                        args: (caller, 10_u32).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_max_leverage(caller, &10_u32);
            },
        },
        PrivilegedCase {
            name: "set_liquidation_treasury",
            invoke: |env, client, caller| {
                let treasury = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_liquidation_treasury",
                        args: (caller, treasury.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_liquidation_treasury(caller, &treasury);
            },
        },
        PrivilegedCase {
            name: "set_slash_treasury",
            invoke: |env, client, caller| {
                let treasury = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "set_slash_treasury",
                        args: (caller, treasury.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.set_slash_treasury(caller, &treasury);
            },
        },
        PrivilegedCase {
            name: "pause",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "pause",
                        args: (caller,).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.pause(caller);
            },
        },
        PrivilegedCase {
            name: "unpause",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "unpause",
                        args: (caller,).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.unpause(caller);
            },
        },
        PrivilegedCase {
            name: "schedule_emergency_drain",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "schedule_emergency_drain",
                        args: (caller, 86400_u64).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.schedule_emergency_drain(caller, &86400_u64);
            },
        },
        PrivilegedCase {
            name: "cancel_emergency_drain",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "cancel_emergency_drain",
                        args: (caller,).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.cancel_emergency_drain(caller);
            },
        },
        PrivilegedCase {
            name: "emergency_drain_to_treasury",
            invoke: |env, client, caller| {
                let recipient = Address::generate(env);
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "emergency_drain_to_treasury",
                        args: (caller, 1000_i128, recipient.clone()).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.emergency_drain_to_treasury(caller, &1000_i128, &recipient);
            },
        },
    ]
}

#[test]
fn test_exhaustive_non_admin_rejected() {
    let env = Env::default();
    let (client, _admin, _user, attacker) = setup(&env);

    for case in get_privileged_cases() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.invoke)(&env, &client, &attacker);
        }));

        assert!(
            res.is_err(),
            "Expected entrypoint '{}' to panic for non-admin",
            case.name
        );
        let err = res.unwrap_err();
        if let Some(err_msg) = err.downcast_ref::<soroban_sdk::Error>() {
            assert!(
                err_msg.is_type(soroban_sdk::xdr::ScErrorType::Context)
                    || err_msg.is_type(soroban_sdk::xdr::ScErrorType::WasmVm)
                    || err_msg.is_type(soroban_sdk::xdr::ScErrorType::Contract),
                "Entrypoint '{}' returned unexpected SDK error: {:?}",
                case.name,
                err_msg
            );
        } else if let Some(err_msg) = err.downcast_ref::<std::string::String>() {
            assert!(
                err_msg.contains("not admin")
                    || err_msg.contains("NotAdmin")
                    || err_msg.contains("Context")
                    || err_msg.contains("Contract")
                    || err_msg.contains("escalating error"),
                "Entrypoint '{}' returned unexpected error: {}",
                case.name,
                err_msg
            );
        }
    }
}

#[test]
fn test_exhaustive_uninitialized_rejected() {
    let env = Env::default();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
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

#[test]
fn test_genuine_require_auth_enforcement() {
    let env = Env::default();

    // Register but DO NOT mock_all_auths
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    // Provide auth explicitly for initialize
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &None::<Address>).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.initialize(&admin, &None);

    let treasury = Address::generate(&env);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_early_exit_config(&admin, &treasury, &500_u32);
    }));

    assert!(res.is_err(), "Call should have failed due to missing auth");
}

#[test]
fn test_transfer_admin_rotates_admin_and_rejects_old_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _user, _attacker) = setup(&env);
    let new_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let args: soroban_sdk::Vec<soroban_sdk::Val> =
        (admin.clone(), new_admin.clone()).into_val(&env);
    env.mock_auths(&[
        soroban_sdk::testutils::MockAuth {
            address: &admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "transfer_admin",
                args: args.clone(),
                sub_invokes: &[],
            },
        },
        soroban_sdk::testutils::MockAuth {
            address: &new_admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "transfer_admin",
                args,
                sub_invokes: &[],
            },
        },
    ]);
    client.transfer_admin(&admin, &new_admin);

    let stored_admin: Address = env.as_contract(&client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, new_admin);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &new_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_early_exit_config",
            args: (&new_admin, &treasury, 500_u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_early_exit_config(&new_admin, &treasury, &500_u32);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_early_exit_config",
            args: (&admin, &treasury, 500_u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_early_exit_config(&admin, &treasury, &500_u32);
    }));
    assert!(result.is_err(), "old admin should no longer be authorized");
}

#[test]
fn test_admin_success() {
    let env = Env::default();
    let (client, admin, _user, _attacker) = setup(&env);

    let treasury = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_early_exit_config",
            args: (&admin, &treasury, 500_u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.set_early_exit_config(&admin, &treasury, &500_u32);

    let config = client.describe_config().unwrap();
    assert_eq!(config.early_exit_penalty_bps, Some(500));
}

#[test]
fn test_admin_as_attester_edge_case() {
    let env = Env::default();
    let (client, admin, _user, _attacker) = setup(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "register_attester",
            args: (&admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.register_attester(&admin);
    assert!(client.is_attester(&admin));

    let treasury = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_early_exit_config",
            args: (&admin, &treasury, 600_u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.set_early_exit_config(&admin, &treasury, &600_u32);
    let config = client.describe_config().unwrap();
    assert_eq!(config.early_exit_penalty_bps, Some(600));

    let non_admin_attester = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "register_attester",
            args: (&non_admin_attester,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.register_attester(&non_admin_attester);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &non_admin_attester,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &client.address,
                fn_name: "set_early_exit_config",
                args: (&non_admin_attester, &treasury, 700_u32).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.set_early_exit_config(&non_admin_attester, &treasury, &700_u32);
    }));
    assert!(res.is_err());
}

#[test]
fn test_access_control_module_direct_checks() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CredenceBond, ());
    let _client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let verifier = Address::generate(&env);
    let user = Address::generate(&env);
    let non_admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Before initialize, get_admin panics and is_admin returns Role::User
        assert_eq!(is_admin(&env, &admin), credence_errors::Role::User);

        // Store admin
        env.storage().instance().set(&DataKey::Admin, &admin);

        // get_admin and is_admin check
        assert_eq!(get_admin(&env), admin);
        assert_eq!(is_admin(&env, &admin), credence_errors::Role::Admin);
        assert_eq!(is_admin(&env, &non_admin), credence_errors::Role::User);

        // require_admin check
        require_admin(&env, &admin);
        let res_admin = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_admin(&env, &non_admin);
        }));
        assert!(res_admin.is_err());

        // require_identity_owner check
        require_identity_owner(&env, &user, &user);
        let res_owner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_identity_owner(&env, &non_admin, &user);
        }));
        assert!(res_owner.is_err());

        // verifier roles
        assert!(!is_verifier(&env, &verifier));
        add_verifier_role(&env, &admin, &verifier);
        assert!(is_verifier(&env, &verifier));

        require_verifier(&env, &verifier);
        let res_ver = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_verifier(&env, &non_admin);
        }));
        assert!(res_ver.is_err());

        // composite admin or verifier
        require_admin_or_verifier(&env, &admin);
        require_admin_or_verifier(&env, &verifier);
        let res_comp = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_admin_or_verifier(&env, &non_admin);
        }));
        assert!(res_comp.is_err());

        // remove verifier role
        remove_verifier_role(&env, &admin, &verifier);
        assert!(!is_verifier(&env, &verifier));
    });
}
