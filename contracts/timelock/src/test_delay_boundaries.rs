use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

const NOW: u64 = 1_000;

fn setup() -> (Env, TimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = NOW);
    let admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
}

fn assert_pending(client: &TimelockContractClient<'_>, id: u64, delay: u64) {
    let operation = client.get_operation(&id).unwrap();
    assert_eq!(operation.id, id);
    assert_eq!(operation.status, OperationStatus::Pending);
    assert_eq!(operation.eta, NOW + delay);
    assert_eq!(operation.expires_at, NOW + delay + GRACE_PERIOD);
}

#[test]
fn minimum_delay_is_86400_seconds() {
    assert_eq!(min_delay_seconds(), 86_400);
}

#[test]
fn queue_operation_rejects_minimum_minus_one() {
    let (env, client, admin) = setup();
    let hash = BytesN::from_array(&env, &[1; 32]);
    assert_eq!(
        client.try_queue_operation(&admin, &hash, &(min_delay_seconds() - 1)),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::TimelockNotReady as u32
        )))
    );
    assert_eq!(client.get_operation_count(), 0);
}

fn assert_queue_operation_accepts(delay: u64) {
    let (env, client, admin) = setup();
    let hash = BytesN::from_array(&env, &[1; 32]);
    let id = client.queue_operation(&admin, &hash, &delay);
    assert_pending(&client, id, delay);
}

#[test]
fn queue_operation_accepts_exact_minimum() {
    assert_queue_operation_accepts(min_delay_seconds());
}

#[test]
fn queue_operation_accepts_minimum_plus_one() {
    assert_queue_operation_accepts(min_delay_seconds() + 1);
}

#[test]
fn queue_call_rejects_minimum_minus_one() {
    let (env, client, admin) = setup();
    assert_eq!(
        client.try_queue_call(
            &admin,
            &Address::generate(&env),
            &Symbol::new(&env, "update"),
            &Vec::new(&env),
            &BytesN::from_array(&env, &[1; 32]),
            &(min_delay_seconds() - 1),
        ),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::TimelockNotReady as u32
        )))
    );
    assert_eq!(client.get_operation_count(), 0);
}

fn assert_queue_call_accepts(delay: u64) {
    let (env, client, admin) = setup();
    let id = client.queue_call(
        &admin,
        &Address::generate(&env),
        &Symbol::new(&env, "update"),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[1; 32]),
        &delay,
    );
    assert_pending(&client, id, delay);
    assert!(client.get_call(&id).is_some());
}

#[test]
fn queue_call_accepts_exact_minimum() {
    assert_queue_call_accepts(min_delay_seconds());
}

#[test]
fn queue_call_accepts_minimum_plus_one() {
    assert_queue_call_accepts(min_delay_seconds() + 1);
}

#[test]
fn is_ready_at_eta_but_not_one_second_before() {
    let eta = NOW + min_delay_seconds();
    assert!(!is_ready(eta, eta - 1));
    assert!(is_ready(eta, eta));
}
