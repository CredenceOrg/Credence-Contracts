use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol};

mod hostile_token {
    use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};

    const TARGET: &str = "target";
    const METHOD: &str = "method";
    const ADMIN: &str = "admin";
    const DESTINATION: &str = "destination";
    const PROPOSAL_ID: &str = "proposal_id";
    const AMOUNT: &str = "amount";
    const ARMED: &str = "armed";
    const ATTEMPTED: &str = "attempted";
    const REJECTED: &str = "rejected";

    #[contract]
    pub struct HostileToken;

    #[contractimpl]
    impl HostileToken {
        pub fn decimals(_e: Env) -> u32 {
            7
        }

        pub fn balance(e: Env, id: Address) -> i128 {
            e.storage().instance().get(&id).unwrap_or(0)
        }

        pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
            Self::maybe_reenter(e.clone());
            let from_balance = Self::balance(e.clone(), from.clone());
            let to_balance = Self::balance(e.clone(), to.clone());
            e.storage()
                .instance()
                .set(&from, &(from_balance - amount));
            e.storage().instance().set(&to, &(to_balance + amount));
        }

        pub fn transfer_from(e: Env, _spender: Address, from: Address, to: Address, amount: i128) {
            Self::transfer(e, from, to, amount);
        }

        pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
            i128::MAX
        }

        pub fn approve(_e: Env, _from: Address, _spender: Address, _amount: i128, _expiration: u32) {}

        pub fn mint(e: Env, to: Address, amount: i128) {
            let balance = Self::balance(e.clone(), to.clone());
            e.storage().instance().set(&to, &(balance + amount));
        }

        pub fn set_authorized(_e: Env, _id: Address, _auth: bool) {}

        pub fn arm(
            e: Env,
            target: Address,
            method: Symbol,
            admin: Address,
            destination: Address,
            proposal_id: u64,
            amount: i128,
        ) {
            e.storage().instance().set(&Symbol::new(&e, TARGET), &target);
            e.storage().instance().set(&Symbol::new(&e, METHOD), &method);
            e.storage().instance().set(&Symbol::new(&e, ADMIN), &admin);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, DESTINATION), &destination);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, PROPOSAL_ID), &proposal_id);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, AMOUNT), &amount);
            e.storage().instance().set(&Symbol::new(&e, ARMED), &true);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, ATTEMPTED), &false);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, REJECTED), &false);
        }

        pub fn attack_attempted(e: Env) -> bool {
            e.storage()
                .instance()
                .get(&Symbol::new(&e, ATTEMPTED))
                .unwrap_or(false)
        }

        pub fn attack_rejected(e: Env) -> bool {
            e.storage()
                .instance()
                .get(&Symbol::new(&e, REJECTED))
                .unwrap_or(false)
        }

        fn maybe_reenter(e: Env) {
            let armed_key = Symbol::new(&e, ARMED);
            if !e.storage().instance().get(&armed_key).unwrap_or(false) {
                return;
            }
            e.storage().instance().set(&armed_key, &false);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, ATTEMPTED), &true);

            let target: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, TARGET))
                .unwrap();
            let method: Symbol = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, METHOD))
                .unwrap();
            let admin: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, ADMIN))
                .unwrap();
            let destination: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, DESTINATION))
                .unwrap();
            let proposal_id: u64 = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, PROPOSAL_ID))
                .unwrap();
            let amount: i128 = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, AMOUNT))
                .unwrap();
            let args = if method == Symbol::new(&e, "execute_withdrawal") {
                Vec::from_array(&e, [proposal_id.into_val(&e), 0_i128.into_val(&e)])
            } else {
                Vec::from_array(&e, [
                    admin.into_val(&e),
                    destination.into_val(&e),
                    amount.into_val(&e),
                ])
            };
            let rejected = !matches!(
                e.try_invoke_contract::<Val, soroban_sdk::Error>(&target, &method, args),
                Ok(Ok(_))
            );
            e.storage()
                .instance()
                .set(&Symbol::new(&e, REJECTED), &rejected);
        }
    }
}

use hostile_token::{HostileToken, HostileTokenClient};

struct Setup<'a> {
    env: Env,
    client: CredenceTreasuryClient<'a>,
    token: HostileTokenClient<'a>,
    contract: Address,
    admin: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(&env, &contract);
    let token_id = env.register(HostileToken, ());
    let token = HostileTokenClient::new(&env, &token_id);
    let admin = Address::generate(&env);
    token.mint(&admin, &20_000);
    client.initialize(&admin, &token_id);
    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    Setup {
        env,
        client,
        token,
        contract,
        admin,
    }
}

fn arm(
    setup: &Setup<'_>,
    method: &str,
    destination: &Address,
    proposal_id: u64,
    amount: i128,
) {
    setup.token.arm(
        &setup.contract,
        &Symbol::new(&setup.env, method),
        &setup.admin,
        destination,
        &proposal_id,
        &amount,
    );
}

fn assert_reentry_rejected(token: &HostileTokenClient<'_>) {
    assert!(token.attack_attempted(), "token did not attempt re-entry");
    assert!(token.attack_rejected(), "reentrant call was not rejected");
}

#[test]
fn hostile_token_reentry_into_execute_withdrawal_is_rejected() {
    let setup = setup();
    let signer = Address::generate(&setup.env);
    let recipient = Address::generate(&setup.env);
    setup.client.add_signer(&signer);
    setup.client.set_threshold(&1);
    let proposal_id = setup
        .client
        .propose_withdrawal(&signer, &recipient, &1_000);
    setup.client.approve_withdrawal(&signer, &proposal_id);
    arm(&setup, "execute_withdrawal", &recipient, proposal_id, 0);

    setup.client.execute_withdrawal(&proposal_id, &0);

    assert_reentry_rejected(&setup.token);
    assert_eq!(setup.client.get_balance(), 9_000);
    assert_eq!(setup.token.balance(&setup.contract), 9_000);
    assert_eq!(setup.token.balance(&recipient), 1_000);
    assert!(setup.client.get_proposal(&proposal_id).executed);
}

#[test]
fn hostile_token_reentry_into_settle_is_rejected() {
    let setup = setup();
    let destination = Address::generate(&setup.env);
    setup.client.register_corridor(&setup.admin, &destination);
    arm(&setup, "settle", &destination, 0, 1_000);

    assert_eq!(setup.client.settle(&setup.admin, &destination, &1_000), 1_000);

    assert_reentry_rejected(&setup.token);
    assert_eq!(setup.client.get_balance(), 9_000);
    assert_eq!(setup.token.balance(&setup.contract), 9_000);
    assert_eq!(setup.token.balance(&destination), 1_000);
}

#[test]
fn hostile_token_reentry_into_rescue_native_is_rejected() {
    let setup = setup();
    let destination = Address::generate(&setup.env);
    setup.token.mint(&setup.contract, &1_000);
    arm(&setup, "rescue_native", &destination, 0, 500);

    setup
        .client
        .rescue_native(&setup.admin, &destination, &500);

    assert_reentry_rejected(&setup.token);
    assert_eq!(setup.client.get_balance(), 10_000);
    assert_eq!(setup.token.balance(&setup.contract), 10_500);
    assert_eq!(setup.token.balance(&destination), 500);
}