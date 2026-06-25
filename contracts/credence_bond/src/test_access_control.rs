#![cfg(test)]
extern crate alloc;
extern crate std;
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
    let args: Vec<Val> = (caller.clone(), new_admin.clone()).into_val(env);
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
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "slash_bond",
                        args: (caller, 100_i128).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.slash_bond(caller, &100_i128);
            },
        },
        PrivilegedCase {
            name: "collect_fees",
            invoke: |env, client, caller| {
                env.mock_auths(&[soroban_sdk::testutils::MockAuth {
                    address: caller,
                    invoke: &soroban_sdk::testutils::MockAuthInvoke {
                        contract: &client.address,
                        fn_name: "collect_fees",
                        args: (caller,).into_val(env),
                        sub_invokes: &[],
                    },
                }]);
                client.collect_fees(caller);
            },
        },
        PrivilegedCase {
            name: "transfer_admin",
            invoke: invoke_transfer_admin,
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

    let args: Vec<Val> = (admin.clone(), new_admin.clone()).into_val(&env);
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

    let config = client.describe_config();
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
    let config = client.describe_config();
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
