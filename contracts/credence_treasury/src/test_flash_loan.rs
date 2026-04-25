#![cfg(test)]

use super::*;
use crate::receiver::{FlashLoanReceiver, FLASH_LOAN_SUCCESS};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, Symbol};

// --- Mock Receivers ---

#[contract]
pub struct ValidReceiver;

#[contractimpl]
impl FlashLoanReceiver for ValidReceiver {
    fn on_flash_loan(
        e: Env,
        _initiator: Address,
        token: Address,
        amount: i128,
        fee: i128,
        _data: Bytes,
    ) -> Symbol {
        // Mandatory Security Check: Verify caller is the trusted treasury
        let treasury: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, "treasury"))
            .unwrap();
        if e.caller() != treasury {
            panic!("unauthorized caller");
        }

        let token_client = token::TokenClient::new(&e, &token);
        // Repay principal + fee
        token_client.transfer(&e.current_contract_address(), &treasury, &(amount + fee));

        Symbol::new(&e, FLASH_LOAN_SUCCESS)
    }
}

impl ValidReceiver {
    pub fn set_treasury(e: Env, treasury: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "treasury"), &treasury);
    }
}

#[contract]
pub struct MaliciousMagicReceiver;

#[contractimpl]
impl FlashLoanReceiver for MaliciousMagicReceiver {
    fn on_flash_loan(
        e: Env,
        _initiator: Address,
        token: Address,
        amount: i128,
        fee: i128,
        _data: Bytes,
    ) -> Symbol {
        let treasury: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, "treasury"))
            .unwrap();
        if e.caller() != treasury {
            panic!("unauthorized caller");
        }
        let token_client = token::TokenClient::new(&e, &token);
        token_client.transfer(&e.current_contract_address(), &treasury, &(amount + fee));

        Symbol::new(&e, "WRONG_MAGIC")
    }
}

impl MaliciousMagicReceiver {
    pub fn set_treasury(e: Env, treasury: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "treasury"), &treasury);
    }
}

#[contract]
pub struct DefaulterReceiver;

#[contractimpl]
impl FlashLoanReceiver for DefaulterReceiver {
    fn on_flash_loan(
        e: Env,
        _initiator: Address,
        _token: Address,
        _amount: i128,
        _fee: i128,
        _data: Bytes,
    ) -> Symbol {
        // Do nothing, don't repay
        Symbol::new(&e, FLASH_LOAN_SUCCESS)
    }
}

// --- Test Suite ---

fn setup_test(
    e: &Env,
) -> (
    CredenceTreasuryClient<'_>,
    token::StellarAssetClient<'_>,
    Address,
    Address,
) {
    let admin = Address::generate(e);
    let treasury_id = e.register(CredenceTreasury, ());
    let treasury = CredenceTreasuryClient::new(e, &treasury_id);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(e, &token_id);

    e.mock_all_auths();
    treasury.initialize(&admin, &token_id);

    // Seed treasury with funds
    token_admin_client.mint(&treasury_id, &1_000_000_i128);

    (treasury, token_admin_client, admin, token_id)
}

#[test]
fn test_flash_loan_success() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _admin, token_id) = setup_test(&e);

    // Set 0.5% fee (50 bps)
    treasury.set_flash_loan_fee(&50);

    let receiver_id = e.register(ValidReceiver, ());
    let receiver_client = ValidReceiverClient::new(&e, &receiver_id);
    receiver_client.set_treasury(&treasury.address);

    let user = Address::generate(&e);
    let amount = 100_000_i128;
    // Expected fee = 100,000 * 50 / 10,000 = 500

    // We need to give the receiver some tokens to pay the fee if they don't have enough
    let token_admin = token::StellarAssetClient::new(&e, &token_id);
    token_admin.mint(&receiver_id, &1_000_i128);

    let balance_before = treasury.get_balance();

    treasury.flash_loan(&user, &receiver_id, &amount, &Bytes::new(&e));

    let balance_after = treasury.get_balance();
    assert_eq!(balance_after, balance_before + 500_i128);

    let source_balance = treasury.get_balance_by_source(FundSource::ProtocolFee);
    assert_eq!(source_balance, 500_i128);
}

#[test]
#[should_panic(expected = "HostError")] // ContractError::InvalidFlashLoanCallback
fn test_flash_loan_wrong_magic_reverts() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _, _) = setup_test(&e);

    let receiver_id = e.register(MaliciousMagicReceiver, ());
    let receiver_client = MaliciousMagicReceiverClient::new(&e, &receiver_id);
    receiver_client.set_treasury(&treasury.address);

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &1000, &Bytes::new(&e));
}

#[test]
#[should_panic(expected = "HostError")] // ContractError::FlashLoanRepaymentFailed
fn test_flash_loan_insufficient_repayment_reverts() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _, _) = setup_test(&e);
    treasury.set_flash_loan_fee(&100); // 1%

    let receiver_id = e.register(DefaulterReceiver, ());

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &1000, &Bytes::new(&e));
}

#[test]
#[should_panic(expected = "HostError")] // ContractError::ReentrancyDetected
fn test_flash_loan_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _, _) = setup_test(&e);

    #[contract]
    pub struct ReentrantReceiver;

    #[contractimpl]
    impl FlashLoanReceiver for ReentrantReceiver {
        fn on_flash_loan(
            e: Env,
            initiator: Address,
            _token: Address,
            amount: i128,
            _fee: i128,
            _data: Bytes,
        ) -> Symbol {
            let treasury_id = e
                .storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "treasury"))
                .unwrap();
            let treasury = CredenceTreasuryClient::new(&e, &treasury_id);
            // Re-enter
            treasury.flash_loan(
                &initiator,
                &e.current_contract_address(),
                &amount,
                &Bytes::new(&e),
            );
            Symbol::new(&e, FLASH_LOAN_SUCCESS)
        }
    }

    let receiver_id = e.register(ReentrantReceiver, ());
    e.as_contract(&receiver_id, || {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "treasury"), &treasury.address);
    });

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &1000, &Bytes::new(&e));
}
