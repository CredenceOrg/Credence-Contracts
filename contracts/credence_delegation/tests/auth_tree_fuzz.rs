#![cfg(test)]

use credence_bond::{CredenceBond, CredenceBondClient};
use credence_delegation::{CredenceDelegation, CredenceDelegationClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec, Address, Env, IntoVal, String,
};

/// A proxy contract to simulate the cross-contract auth tree.
#[soroban_sdk::contract]
pub struct AuthProxy;

#[soroban_sdk::contractimpl]
impl AuthProxy {
    pub fn delegated_action(
        e: Env,
        bond_id: Address,
        owner: Address,
        subject: Address,
        nonce: u64,
    ) {
        owner.require_auth();
        let bond_client = CredenceBondClient::new(&e, &bond_id);
        bond_client.add_attestation(&owner, &subject, &String::from_str(&e, "fuzz_data"), &nonce);
    }
}

fn setup(e: &Env) -> (Address, Address, Address, Address) {
    // Broadly authorize the one-time administrative setup calls below
    // (initialize / register_attester). The individual tests then narrow the
    // auth context with `e.mock_auths(..)` to exercise the precise auth tree.
    e.mock_all_auths();

    let admin = Address::generate(e);

    let delegation_id = e.register(CredenceDelegation, ());
    let delegation_client = CredenceDelegationClient::new(e, &delegation_id);
    delegation_client.initialize(&admin);

    let bond_id = e.register(CredenceBond, ());
    let bond_client = CredenceBondClient::new(e, &bond_id);
    bond_client.initialize(&admin, &None);

    let owner = Address::generate(e);
    let subject = Address::generate(e);
    let proxy_id = e.register(AuthProxy, ());

    // Owner must be an authorized attester for `add_attestation` to succeed.
    bond_client.register_attester(&owner);

    (bond_id, proxy_id, owner, subject)
}

#[test]
fn test_auth_tree_valid() {
    let e = Env::default();
    let (bond_id, proxy_id, owner, subject) = setup(&e);

    // Leaf invoke: CredenceBond::add_attestation, authorized by `owner`.
    let leaf_invoke = MockAuthInvoke {
        contract: &bond_id,
        fn_name: "add_attestation",
        args: vec![
            &e,
            owner.to_val(),
            subject.to_val(),
            String::from_str(&e, "fuzz_data").to_val(),
            0_u64.into_val(&e),
        ],
        sub_invokes: &[],
    };

    // Root invoke: AuthProxy::delegated_action, with the bond call as a sub-invoke.
    let root_invoke = MockAuthInvoke {
        contract: &proxy_id,
        fn_name: "delegated_action",
        args: vec![
            &e,
            bond_id.to_val(),
            owner.to_val(),
            subject.to_val(),
            0_u64.into_val(&e),
        ],
        sub_invokes: core::slice::from_ref(&leaf_invoke),
    };

    e.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &root_invoke,
    }]);

    let proxy_client = AuthProxyClient::new(&e, &proxy_id);
    proxy_client.delegated_action(&bond_id, &owner, &subject, &0);
}

#[test]
#[should_panic]
fn test_auth_tree_missing_leaf() {
    let e = Env::default();
    let (bond_id, proxy_id, owner, subject) = setup(&e);

    // Root invoke authorized, but the required leaf (bond call) sub-invoke is
    // omitted, so the nested `require_auth` inside the bond contract fails.
    let root_invoke = MockAuthInvoke {
        contract: &proxy_id,
        fn_name: "delegated_action",
        args: vec![
            &e,
            bond_id.to_val(),
            owner.to_val(),
            subject.to_val(),
            0_u64.into_val(&e),
        ],
        sub_invokes: &[],
    };

    e.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &root_invoke,
    }]);

    let proxy_client = AuthProxyClient::new(&e, &proxy_id);
    proxy_client.delegated_action(&bond_id, &owner, &subject, &0);
}
