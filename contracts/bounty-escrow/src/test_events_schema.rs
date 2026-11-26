use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token, Address, Env, Error, Symbol, TryFromVal, Val,
};

fn decode<T: TryFromVal<Env, Val>>(env: &Env, val: Val, what: &str) -> T {
    T::try_from_val(env, &val).unwrap_or_else(|_| panic!("{what} has the wrong type"))
}

fn contract_events(
    env: &Env,
    contract_id: &Address,
) -> soroban_sdk::Vec<(Address, soroban_sdk::Vec<Val>, Val)> {
    let all = env.events().all();
    let mut events = soroban_sdk::Vec::new(env);
    for event in all.iter() {
        if event.0 == *contract_id {
            events.push_back(event);
        }
    }
    events
}

fn only_contract_event(env: &Env, contract_id: &Address) -> (soroban_sdk::Vec<Val>, Val) {
    let events = contract_events(env, contract_id);
    assert_eq!(events.len(), 1, "expected exactly one bounty-escrow event");
    let (_, topics, data) = events.get(0).unwrap();
    (topics, data)
}

fn assert_no_contract_events(env: &Env, contract_id: &Address) {
    let events = contract_events(env, contract_id);
    assert!(
        events.is_empty(),
        "rejected calls must not emit bounty-escrow events"
    );
    assert!(
        env.events().all().is_empty(),
        "rejected calls must not emit events"
    );
}

fn contract_error(error: ContractError) -> Error {
    Error::from_contract_error(error as u32)
}

struct Fixture {
    env: Env,
    contract_id: Address,
    client: BountyEscrowClient<'static>,
    depositor: Address,
    beneficiary: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_addr = token_contract.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&depositor, &100_000);

    let contract_id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);
    let _ = env.events().all();

    Fixture {
        env,
        contract_id,
        client,
        depositor,
        beneficiary,
    }
}

#[test]
fn lock_event_schema_matches() {
    let f = setup();
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);

    assert_eq!(id, 1);
    let (topics, data) = only_contract_event(&f.env, &f.contract_id);
    assert_eq!(topics.len(), 2, "escrow_locked must have 2 topics");
    assert_eq!(
        decode::<Symbol>(&f.env, topics.get(0).unwrap(), "topic[0]"),
        Symbol::new(&f.env, "escrow_locked")
    );
    assert_eq!(decode::<u64>(&f.env, topics.get(1).unwrap(), "topic[1]"), 1);

    let (depositor, beneficiary, amount, expires_at): (Address, Address, i128, u64) = decode(
        &f.env,
        data,
        "escrow_locked data (Address, Address, i128, u64)",
    );
    assert_eq!(depositor, f.depositor);
    assert_eq!(beneficiary, f.beneficiary);
    assert_eq!(amount, 1_000);
    assert_eq!(expires_at, 2_000);
}

#[test]
fn release_event_schema_matches() {
    let f = setup();
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
    let _ = f.env.events().all();

    f.client.release(&f.depositor, &id);

    let (topics, data) = only_contract_event(&f.env, &f.contract_id);
    assert_eq!(topics.len(), 2, "escrow_released must have 2 topics");
    assert_eq!(
        decode::<Symbol>(&f.env, topics.get(0).unwrap(), "topic[0]"),
        Symbol::new(&f.env, "escrow_released")
    );
    assert_eq!(
        decode::<u64>(&f.env, topics.get(1).unwrap(), "topic[1]"),
        id
    );

    let (beneficiary, payout): (Address, i128) =
        decode(&f.env, data, "escrow_released data (Address, i128)");
    assert_eq!(beneficiary, f.beneficiary);
    assert_eq!(payout, 1_000);
}

#[test]
fn refund_event_schema_matches() {
    let f = setup();
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
    f.env.ledger().with_mut(|li| li.timestamp = 2_000);
    let _ = f.env.events().all();

    f.client.refund(&id, &300);

    let (topics, data) = only_contract_event(&f.env, &f.contract_id);
    assert_eq!(topics.len(), 2, "escrow_refunded must have 2 topics");
    assert_eq!(
        decode::<Symbol>(&f.env, topics.get(0).unwrap(), "topic[0]"),
        Symbol::new(&f.env, "escrow_refunded")
    );
    assert_eq!(
        decode::<u64>(&f.env, topics.get(1).unwrap(), "topic[1]"),
        id
    );

    let (depositor, amount, remaining_amount): (Address, i128, i128) =
        decode(&f.env, data, "escrow_refunded data (Address, i128, i128)");
    assert_eq!(depositor, f.depositor);
    assert_eq!(amount, 300);
    assert_eq!(remaining_amount, 700);
    assert_eq!(
        f.client.get_escrow(&id).unwrap().status,
        EscrowStatus::PartiallyRefunded
    );

    let _ = f.env.events().all();
    f.client.refund(&id, &700);

    let (topics, data) = only_contract_event(&f.env, &f.contract_id);
    assert_eq!(topics.len(), 2, "escrow_refunded must have 2 topics");
    assert_eq!(
        decode::<Symbol>(&f.env, topics.get(0).unwrap(), "topic[0]"),
        Symbol::new(&f.env, "escrow_refunded")
    );
    assert_eq!(
        decode::<u64>(&f.env, topics.get(1).unwrap(), "topic[1]"),
        id
    );

    let (depositor, amount, remaining_amount): (Address, i128, i128) =
        decode(&f.env, data, "escrow_refunded data (Address, i128, i128)");
    assert_eq!(depositor, f.depositor);
    assert_eq!(amount, 700);
    assert_eq!(remaining_amount, 0);
    assert_eq!(
        f.client.get_escrow(&id).unwrap().status,
        EscrowStatus::Refunded
    );
}

#[test]
fn rejected_calls_emit_no_events() {
    let f = setup();
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
    let _ = f.env.events().all();

    assert_eq!(
        f.client.try_lock(&f.depositor, &f.beneficiary, &0, &2_000),
        Err(Ok(contract_error(ContractError::AmountMustBePositive)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    assert_eq!(
        f.client
            .try_lock(&f.depositor, &f.beneficiary, &1_000, &999),
        Err(Ok(contract_error(ContractError::InvalidEscrowExpiry)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.client.try_release(&stranger, &id),
        Err(Ok(contract_error(ContractError::NotEscrowParty)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    assert_eq!(
        f.client.try_refund(&id, &500),
        Err(Ok(contract_error(ContractError::EscrowNotExpired)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    f.env.ledger().with_mut(|li| li.timestamp = 2_000);
    assert_eq!(
        f.client.try_refund(&id, &1_001),
        Err(Ok(contract_error(ContractError::RefundExceedsRemaining)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    f.client.refund(&id, &1_000);
    let _ = f.env.events().all();

    assert_eq!(
        f.client.try_refund(&id, &1),
        Err(Ok(contract_error(ContractError::EscrowNotRefundable)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);

    assert_eq!(
        f.client.try_release(&f.depositor, &id),
        Err(Ok(contract_error(ContractError::EscrowNotReleasable)))
    );
    assert_no_contract_events(&f.env, &f.contract_id);
}
