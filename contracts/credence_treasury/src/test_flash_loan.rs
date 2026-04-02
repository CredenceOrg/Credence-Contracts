#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env};

// Each receiver lives in its own module. Multiple `#[contractimpl] impl Trait for T`
// in a single module cause symbol conflicts (`__on_flash_loan` defined multiple times)
// because the macro emits module-level dispatch symbols named after the method, not the type.

mod valid_receiver {
    use crate::receiver::{FlashLoanReceiver, FLASH_LOAN_SUCCESS};
    use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, Symbol};

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
            let treasury: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "treasury"))
                .unwrap();
            let token_client = token::TokenClient::new(&e, &token);
            token_client.transfer(&e.current_contract_address(), &treasury, &(amount + fee));
            Symbol::new(&e, FLASH_LOAN_SUCCESS)
        }
    }

    #[contractimpl]
    impl ValidReceiver {
        pub fn set_treasury(e: Env, treasury: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "treasury"), &treasury);
        }
    }
}

mod malicious_magic_receiver {
    use crate::receiver::FlashLoanReceiver;
    use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, Symbol};

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
            let token_client = token::TokenClient::new(&e, &token);
            token_client.transfer(&e.current_contract_address(), &treasury, &(amount + fee));
            Symbol::new(&e, "WRONG_MAGIC")
        }
    }

    #[contractimpl]
    impl MaliciousMagicReceiver {
        pub fn set_treasury(e: Env, treasury: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "treasury"), &treasury);
        }
    }
}

mod defaulter_receiver {
    use crate::receiver::{FlashLoanReceiver, FLASH_LOAN_SUCCESS};
    use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Symbol};

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
}

mod reentrant_receiver {
    use crate::receiver::{FlashLoanReceiver, FLASH_LOAN_SUCCESS};
    use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Symbol};

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
            let treasury = crate::CredenceTreasuryClient::new(&e, &treasury_id);
            // Attempt re-entry: must be blocked by the flash loan reentrancy guard.
            treasury.flash_loan(
                &initiator,
                &e.current_contract_address(),
                &amount,
                &Bytes::new(&e),
            );
            Symbol::new(&e, FLASH_LOAN_SUCCESS)
        }
    }

    #[contractimpl]
    impl ReentrantReceiver {
        pub fn set_treasury(e: Env, treasury: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "treasury"), &treasury);
        }
    }
}

use defaulter_receiver::DefaulterReceiver;
use malicious_magic_receiver::{MaliciousMagicReceiver, MaliciousMagicReceiverClient};
use reentrant_receiver::{ReentrantReceiver, ReentrantReceiverClient};
use valid_receiver::{ValidReceiver, ValidReceiverClient};

// --- Test Suite ---

fn setup_test(
    e: &Env,
) -> (
    CredenceTreasuryClient<'_>,
    soroban_sdk::token::StellarAssetClient<'_>,
    Address,
    Address,
) {
    let admin = Address::generate(e);
    let treasury_id = e.register(CredenceTreasury, ());
    let treasury = CredenceTreasuryClient::new(e, &treasury_id);
    treasury.initialize(&admin);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);

    treasury.set_token(&token_id);

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

    // Give the receiver tokens to cover the fee
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    token_admin.mint(&receiver_id, &1_000_i128);

    let balance_before = treasury.get_balance();

    treasury.flash_loan(&user, &receiver_id, &amount, &Bytes::new(&e));

    let balance_after = treasury.get_balance();
    assert_eq!(balance_after, balance_before + 500_i128);

    let source_balance = treasury.get_balance_by_source(&FundSource::ProtocolFee);
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

    let receiver_id = e.register(ReentrantReceiver, ());
    let receiver_client = ReentrantReceiverClient::new(&e, &receiver_id);
    receiver_client.set_treasury(&treasury.address);

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &1000, &Bytes::new(&e));
}

#[test]
#[should_panic(expected = "HostError")] // ContractError::AmountMustBePositive
fn test_flash_loan_zero_amount_reverts() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _, _) = setup_test(&e);

    // Zero amount must be rejected before any external callback is invoked.
    // ValidReceiver is used as receiver: if the callback were reached it would
    // succeed, so a HostError here confirms the guard fired before the callback.
    let receiver_id = e.register(ValidReceiver, ());
    let receiver_client = ValidReceiverClient::new(&e, &receiver_id);
    receiver_client.set_treasury(&treasury.address);

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &0, &Bytes::new(&e));
}

#[test]
#[should_panic(expected = "HostError")] // ContractError::AmountMustBePositive
fn test_flash_loan_negative_amount_reverts() {
    let e = Env::default();
    e.mock_all_auths();
    let (treasury, _, _, _) = setup_test(&e);

    let receiver_id = e.register(ValidReceiver, ());
    let receiver_client = ValidReceiverClient::new(&e, &receiver_id);
    receiver_client.set_treasury(&treasury.address);

    let user = Address::generate(&e);
    treasury.flash_loan(&user, &receiver_id, &-1, &Bytes::new(&e));
}
