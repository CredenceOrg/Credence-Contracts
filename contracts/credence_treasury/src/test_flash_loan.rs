#![cfg(test)]

use super::*;
use crate::receiver::FLASH_LOAN_SUCCESS;
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{token, Address, Bytes, Env, IntoVal};

fn stored_treasury(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&soroban_sdk::Symbol::new(e, "treasury"))
        .unwrap()
}

fn set_treasury(e: &Env, receiver: &Address, treasury: &Address) {
    e.as_contract(receiver, || {
        e.storage()
            .instance()
            .set(&soroban_sdk::Symbol::new(e, "treasury"), treasury);
    });
}

mod valid_receiver {
    use super::stored_treasury;
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
            let treasury = stored_treasury(&e);
            treasury.require_auth();
            token::TokenClient::new(&e, &token).transfer(
                &e.current_contract_address(),
                &treasury,
                &(amount + fee),
            );
            Symbol::new(&e, FLASH_LOAN_SUCCESS)
        }
    }
}

mod wrong_magic_receiver {
    use super::stored_treasury;
    use crate::receiver::{FlashLoanReceiver, FLASH_LOAN_SUCCESS};
    use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, Symbol};

    #[contract]
    pub struct WrongMagicReceiver;

    #[contractimpl]
    impl FlashLoanReceiver for WrongMagicReceiver {
        fn on_flash_loan(
            e: Env,
            _initiator: Address,
            token: Address,
            amount: i128,
            fee: i128,
            _data: Bytes,
        ) -> Symbol {
            let treasury = stored_treasury(&e);
            treasury.require_auth();
            token::TokenClient::new(&e, &token).transfer(
                &e.current_contract_address(),
                &treasury,
                &(amount + fee),
            );
            let _ = FLASH_LOAN_SUCCESS;
            Symbol::new(&e, "WRONG_MAGIC")
        }
    }
}

mod defaulter_receiver {
    use super::stored_treasury;
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
            stored_treasury(&e).require_auth();
            Symbol::new(&e, FLASH_LOAN_SUCCESS)
        }
    }
}

mod panicking_receiver {
    use super::stored_treasury;
    use crate::receiver::FlashLoanReceiver;
    use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, Symbol};

    #[contract]
    pub struct PanickingReceiver;

    #[contractimpl]
    impl FlashLoanReceiver for PanickingReceiver {
        fn on_flash_loan(
            e: Env,
            _initiator: Address,
            _token: Address,
            _amount: i128,
            _fee: i128,
            _data: Bytes,
        ) -> Symbol {
            stored_treasury(&e).require_auth();
            panic!("receiver panicked");
        }
    }
}

use defaulter_receiver::{DefaulterReceiver, DefaulterReceiverClient};
use panicking_receiver::{PanickingReceiver, PanickingReceiverClient};
use valid_receiver::{ValidReceiver, ValidReceiverClient};
use wrong_magic_receiver::{WrongMagicReceiver, WrongMagicReceiverClient};

struct Setup<'a> {
    treasury: CredenceTreasuryClient<'a>,
    token_admin: token::StellarAssetClient<'a>,
    admin: Address,
    token: Address,
}

fn setup_test(e: &Env) -> Setup<'_> {
    let admin = Address::generate(e);
    let treasury_id = e.register(CredenceTreasury, ());
    let treasury = CredenceTreasuryClient::new(e, &treasury_id);
    let token_admin_addr = Address::generate(e);
    let token = e.register_stellar_asset_contract(token_admin_addr);
    let token_admin = token::StellarAssetClient::new(e, &token);
    e.mock_all_auths();
    treasury.initialize(&admin, &token);
    token_admin.mint(&treasury_id, &1_000_000_i128);
    Setup {
        treasury,
        token_admin,
        admin,
        token,
    }
}

fn mock_flash_loan_auth(
    e: &Env,
    treasury: &Address,
    initiator: &Address,
    receiver: &Address,
    amount: i128,
    data: &Bytes,
    copies: usize,
) {
    let invoke = MockAuthInvoke {
        contract: treasury,
        fn_name: "flash_loan",
        args: (initiator.clone(), receiver.clone(), amount, data.clone()).into_val(e),
        sub_invokes: &[],
    };
    let auth = MockAuth {
        address: initiator,
        invoke: &invoke,
    };
    let auths = [auth.clone(), auth];
    e.mock_auths(&auths[..copies]);
}

fn assert_contract_error(
    result: Result<
        Result<(), soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    code: u32,
) {
    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(code)))
    );
}

#[test]
fn test_flash_loan_success() {
    let e = Env::default();
    let setup = setup_test(&e);
    setup.treasury.set_flash_loan_fee(&50);

    let receiver = e.register(ValidReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);
    setup.token_admin.mint(&receiver, &1_000_i128);

    let user = Address::generate(&e);
    let amount = 100_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );

    let token_client = token::TokenClient::new(&e, &setup.token);
    let token_before = token_client.balance(&setup.treasury.address);
    let balance_before = setup.treasury.get_balance();

    setup.treasury.flash_loan(&user, &receiver, &amount, &data);

    assert_eq!(
        token_client.balance(&setup.treasury.address),
        token_before + 500
    );
    assert_eq!(setup.treasury.get_balance(), balance_before + 500);
    assert_eq!(
        setup
            .treasury
            .get_balance_by_source(&FundSource::ProtocolFee),
        500
    );
    assert_eq!(setup.treasury.get_flash_loan_fee(), 50);
}

#[test]
fn test_flash_loan_wrong_symbol_reverts() {
    let e = Env::default();
    let setup = setup_test(&e);
    setup.treasury.set_flash_loan_fee(&100);

    let receiver = e.register(WrongMagicReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);
    setup.token_admin.mint(&receiver, &1_000_i128);

    let user = Address::generate(&e);
    let amount = 1_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );

    let token_client = token::TokenClient::new(&e, &setup.token);
    let token_before = token_client.balance(&setup.treasury.address);
    let balance_before = setup.treasury.get_balance();

    assert_contract_error(
        setup
            .treasury
            .try_flash_loan(&user, &receiver, &amount, &data),
        ContractError::InvalidFlashLoanCallback as u32,
    );
    assert_eq!(token_client.balance(&setup.treasury.address), token_before);
    assert_eq!(setup.treasury.get_balance(), balance_before);
    assert_eq!(token_client.balance(&receiver), 1_000);
}

#[test]
fn test_flash_loan_insufficient_repayment_reverts() {
    let e = Env::default();
    let setup = setup_test(&e);
    setup.treasury.set_flash_loan_fee(&100);

    let receiver = e.register(DefaulterReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);

    let user = Address::generate(&e);
    let amount = 1_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );

    let token_client = token::TokenClient::new(&e, &setup.token);
    let token_before = token_client.balance(&setup.treasury.address);
    let balance_before = setup.treasury.get_balance();

    assert_contract_error(
        setup
            .treasury
            .try_flash_loan(&user, &receiver, &amount, &data),
        ContractError::FlashLoanRepaymentFailed as u32,
    );
    assert_eq!(token_client.balance(&setup.treasury.address), token_before);
    assert_eq!(setup.treasury.get_balance(), balance_before);
    assert_eq!(token_client.balance(&receiver), 0);
}

#[test]
fn test_flash_loan_receiver_panic_reverts() {
    let e = Env::default();
    let setup = setup_test(&e);
    setup.treasury.set_flash_loan_fee(&50);

    let receiver = e.register(PanickingReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);

    let user = Address::generate(&e);
    let amount = 1_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );

    let token_client = token::TokenClient::new(&e, &setup.token);
    let token_before = token_client.balance(&setup.treasury.address);
    let balance_before = setup.treasury.get_balance();

    assert!(setup
        .treasury
        .try_flash_loan(&user, &receiver, &amount, &data)
        .is_err());
    assert_eq!(token_client.balance(&setup.treasury.address), token_before);
    assert_eq!(setup.treasury.get_balance(), balance_before);
    assert_eq!(token_client.balance(&receiver), 0);
}

#[test]
fn test_flash_loan_reentrancy_blocked() {
    let e = Env::default();
    let setup = setup_test(&e);

    e.as_contract(&setup.treasury.address, || {
        e.storage().instance().set(&DataKey::WithdrawalLock, &true);
    });

    let receiver = e.register(ValidReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);

    let user = Address::generate(&e);
    let amount = 1_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );

    let token_client = token::TokenClient::new(&e, &setup.token);
    let token_before = token_client.balance(&setup.treasury.address);
    let balance_before = setup.treasury.get_balance();

    assert_contract_error(
        setup
            .treasury
            .try_flash_loan(&user, &receiver, &amount, &data),
        ContractError::ReentrancyDetected as u32,
    );
    assert_eq!(token_client.balance(&setup.treasury.address), token_before);
    assert_eq!(setup.treasury.get_balance(), balance_before);
}

#[test]
fn test_flash_loan_paused_reverts() {
    let e = Env::default();
    let setup = setup_test(&e);
    let receiver = e.register(DefaulterReceiver, ());
    set_treasury(&e, &receiver, &setup.treasury.address);

    e.mock_all_auths();
    setup.treasury.pause(&setup.admin);

    let user = Address::generate(&e);
    let amount = 1_000_i128;
    let data = Bytes::new(&e);
    mock_flash_loan_auth(
        &e,
        &setup.treasury.address,
        &user,
        &receiver,
        amount,
        &data,
        1,
    );
    assert!(setup
        .treasury
        .try_flash_loan(&user, &receiver, &amount, &data)
        .is_err());
}

#[test]
fn test_set_flash_loan_fee_rejects_out_of_range() {
    let e = Env::default();
    let setup = setup_test(&e);
    assert!(setup.treasury.try_set_flash_loan_fee(&10_001).is_err());
    assert_eq!(setup.treasury.get_flash_loan_fee(), 0);
}
