#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::Address as _,
    Address, BytesN, Env,
};

use crate::idempotency::{Idempotency, IdempotencyError};

#[test]
fn first_execution_stores_result() {
    let env = Env::default();
    let caller = Address::generate(&env);
    let tx_id = BytesN::from_array(&env, &[1u8; 32]);

    let result = Idempotency::handle(&env, tx_id.clone(), caller.clone(), || {
        vec![10, 20, 30]
    }).unwrap();

    assert_eq!(result, vec![10, 20, 30]);
}

#[test]
fn duplicate_returns_same_result() {
    let env = Env::default();
    let caller = Address::generate(&env);
    let tx_id = BytesN::from_array(&env, &[2u8; 32]);

    let _ = Idempotency::handle(&env, tx_id.clone(), caller.clone(), || {
        vec![1, 2, 3]
    }).unwrap();

    let second = Idempotency::handle(&env, tx_id.clone(), caller.clone(), || {
        vec![9, 9, 9]
    }).unwrap();

    assert_eq!(second, vec![1, 2, 3]);
}

#[test]
fn duplicate_different_caller_fails() {
    let env = Env::default();
    let caller1 = Address::generate(&env);
    let caller2 = Address::generate(&env);
    let tx_id = BytesN::from_array(&env, &[3u8; 32]);

    let _ = Idempotency::handle(&env, tx_id.clone(), caller1.clone(), || {
        vec![5]
    }).unwrap();

    let result = Idempotency::handle(&env, tx_id.clone(), caller2.clone(), || {
        vec![6]
    });

    assert_eq!(result, Err(IdempotencyError::DuplicateDifferentCaller));
}