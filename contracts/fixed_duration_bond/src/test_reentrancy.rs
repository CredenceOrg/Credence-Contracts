use crate::{FixedDurationBond, FixedDurationBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

mod hostile_token {
    use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};

    const TARGET: &str = "target";
    const METHOD: &str = "method";
    const ACCOUNT: &str = "account";
    const RECIPIENT: &str = "recipient";
    const ARMED: &str = "armed";
    const ATTEMPTED: &str = "attempted";
    const REJECTED: &str = "rejected";

    #[contract]
    pub struct HostileToken;

    #[contractimpl]
    impl HostileToken {
        pub fn decimals(_e: Env) -> u32 {
            6
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

        pub fn arm(e: Env, target: Address, method: Symbol, account: Address, recipient: Address) {
            e.storage().instance().set(&Symbol::new(&e, TARGET), &target);
            e.storage().instance().set(&Symbol::new(&e, METHOD), &method);
            e.storage().instance().set(&Symbol::new(&e, ACCOUNT), &account);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, RECIPIENT), &recipient);
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
            let account: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, ACCOUNT))
                .unwrap();
            let recipient: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, RECIPIENT))
                .unwrap();
            let args = if method == Symbol::new(&e, "collect_fees") {
                Vec::from_array(&e, [account.into_val(&e), recipient.into_val(&e)])
            } else {
                Vec::from_array(&e, [account.into_val(&e)])
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
    client: FixedDurationBondClient<'a>,
    token: HostileTokenClient<'a>,
    contract: Address,
    admin: Address,
    owner: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(FixedDurationBond, ());
    let client = FixedDurationBondClient::new(&env, &contract);
    let token_id = env.register(HostileToken, ());
    let token = HostileTokenClient::new(&env, &token_id);
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    token.mint(&owner, &10_000);
    client.initialize(&admin, &token_id);
    Setup {
        env,
        client,
        token,
        contract,
        admin,
        owner,
    }
}

fn arm(setup: &Setup<'_>, method: &str, recipient: &Address) {
    setup.token.arm(
        &setup.contract,
        &Symbol::new(&setup.env, method),
        &setup.owner,
        recipient,
    );
}

fn assert_reentry_rejected(token: &HostileTokenClient<'_>) {
    assert!(token.attack_attempted(), "token did not attempt re-entry");
    assert!(token.attack_rejected(), "reentrant call was not rejected");
}

#[test]
fn hostile_token_reentry_into_withdraw_is_rejected() {
    let setup = setup();
    let bond = setup.client.create_bond(&setup.owner, &1_000, &100);
    setup.env.ledger().with_mut(|ledger| ledger.timestamp = bond.bond_expiry);
    let contract_before = setup.token.balance(&setup.contract);
    arm(&setup, "withdraw", &setup.owner);

    let withdrawn = setup.client.withdraw(&setup.owner);

    assert_reentry_rejected(&setup.token);
    assert!(!withdrawn.active);
    assert_eq!(setup.token.balance(&setup.contract), contract_before - 1_000);
    assert_eq!(setup.token.balance(&setup.owner), 10_000);
}

#[test]
fn hostile_token_reentry_into_withdraw_early_is_rejected() {
    let setup = setup();
    let penalty_treasury = Address::generate(&setup.env);
    setup
        .client
        .set_penalty_config(&setup.admin, &penalty_treasury, &1_000);
    let bond = setup.client.create_bond(&setup.owner, &1_000, &100);
    let contract_before = setup.token.balance(&setup.contract);
    arm(&setup, "withdraw_early", &setup.owner);

    let withdrawn = setup.client.withdraw_early(&setup.owner);

    assert_reentry_rejected(&setup.token);
    assert!(!withdrawn.active);
    assert_eq!(setup.token.balance(&setup.owner), 9_900);
    assert_eq!(setup.token.balance(&penalty_treasury), 100);
    assert_eq!(setup.token.balance(&setup.contract), contract_before - bond.amount);
}

#[test]
fn hostile_token_reentry_into_collect_fees_is_rejected() {
    let setup = setup();
    let fee_treasury = Address::generate(&setup.env);
    let recipient = Address::generate(&setup.env);
    setup
        .client
        .set_fee_config(&setup.admin, &fee_treasury, &1_000);
    setup.client.create_bond(&setup.owner, &1_000, &100);
    let contract_before = setup.token.balance(&setup.contract);
    arm(&setup, "collect_fees", &recipient);

    let fees = setup.client.collect_fees(&setup.admin, &recipient);

    assert_reentry_rejected(&setup.token);
    assert_eq!(fees, 100);
    assert_eq!(setup.token.balance(&recipient), fees);
    assert_eq!(setup.token.balance(&setup.contract), contract_before - fees);
    assert!(setup
        .client
        .try_collect_fees(&setup.admin, &recipient)
        .is_err());
}