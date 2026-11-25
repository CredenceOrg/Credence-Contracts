#![allow(clippy::disallowed_macros)]

//! Cost regression test for `timelock`.
//!
//! Guards CPU, memory, and ledger read/write footprint of core timelock
//! entrypoints against `cost_baseline.json` with a 10% tolerance.
//!
//! To refresh the baseline after an intentional cost change:
//! `UPDATE_COST_BASELINE=1 cargo test -p timelock --locked cost`

use std::collections::BTreeMap;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN,
};
use testutils::cost_harness::{
    assert_cost_baseline, diff, fresh_env, measure, Baseline, EntryCost, TOLERANCE_PCT,
};
use timelock::{min_delay_seconds, TimelockContract, TimelockContractClient};

const ENTRYPOINTS: &[&str] = &[
    "queue_operation",
    "execute_operation",
    "cancel_operation",
    "get_operations_page",
];

const UPDATE_CMD: &str = "UPDATE_COST_BASELINE=1 cargo test -p timelock --locked cost";

fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();
    let delay = min_delay_seconds();

    // queue_operation
    {
        let env = fresh_env();
        let client = TimelockContractClient::new(&env, &env.register(TimelockContract, ()));
        let admin = Address::generate(&env);
        env.ledger().set_timestamp(1_000);
        client.initialize(&admin);
        let op_hash = BytesN::from_array(&env, &[1u8; 32]);
        client.queue_operation(&admin, &op_hash, &delay);
        out.insert("queue_operation".into(), measure(&env));
    }

    // execute_operation
    {
        let env = fresh_env();
        let client = TimelockContractClient::new(&env, &env.register(TimelockContract, ()));
        let admin = Address::generate(&env);
        env.ledger().set_timestamp(1_000);
        client.initialize(&admin);
        let op_hash = BytesN::from_array(&env, &[2u8; 32]);
        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        env.ledger().set_timestamp(1_000 + delay + 1);
        client.execute_operation(&op_id);
        out.insert("execute_operation".into(), measure(&env));
    }

    // cancel_operation
    {
        let env = fresh_env();
        let client = TimelockContractClient::new(&env, &env.register(TimelockContract, ()));
        let admin = Address::generate(&env);
        env.ledger().set_timestamp(1_000);
        client.initialize(&admin);
        let op_hash = BytesN::from_array(&env, &[3u8; 32]);
        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        client.cancel_operation(&admin, &op_id);
        out.insert("cancel_operation".into(), measure(&env));
    }

    // get_operations_page
    {
        let env = fresh_env();
        let client = TimelockContractClient::new(&env, &env.register(TimelockContract, ()));
        let admin = Address::generate(&env);
        env.ledger().set_timestamp(1_000);
        client.initialize(&admin);
        for i in 0u8..5 {
            let op_hash = BytesN::from_array(&env, &[10 + i; 32]);
            client.queue_operation(&admin, &op_hash, &delay);
        }
        client.get_operations_page(&0_u64, &10_u32);
        out.insert("get_operations_page".into(), measure(&env));
    }

    out
}

#[test]
fn test_cost_regression() {
    let current = measure_all();
    assert_cost_baseline(env!("CARGO_MANIFEST_DIR"), ENTRYPOINTS, &current, UPDATE_CMD);
}

#[test]
fn test_cost_regression_detects_exceeded_tolerance() {
    let current = measure_all();
    let baseline = Baseline {
        tolerance_pct: TOLERANCE_PCT,
        costs: current.clone(),
    };
    assert!(diff(ENTRYPOINTS, &baseline, &current).is_empty());

    let mut regressed = current;
    let exec = regressed
        .get_mut("execute_operation")
        .expect("execute_operation measured");
    exec.cpu_insns = (exec.cpu_insns as f64 * 1.15) as i64 + 1;

    let regressions = diff(ENTRYPOINTS, &baseline, &regressed);
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].entrypoint, "execute_operation");
    assert_eq!(regressions[0].metric, "cpu_insns");
}
