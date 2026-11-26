#![allow(clippy::disallowed_macros)]

//! Cost regression test for `credence_arbitration`.
//!
//! Guards CPU, memory, and ledger read/write footprint of core arbitration
//! entrypoints against `cost_baseline.json` with a 10% tolerance.
//!
//! To refresh the baseline after an intentional cost change:
//! `UPDATE_COST_BASELINE=1 cargo test -p credence_arbitration --locked cost`

use std::collections::BTreeMap;

use credence_arbitration::{CredenceArbitration, CredenceArbitrationClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, String as SorobanString,
};
use testutils::cost_harness::{
    assert_cost_baseline, diff, fresh_env, measure, Baseline, EntryCost, TOLERANCE_PCT,
};

const ENTRYPOINTS: &[&str] = &[
    "register_arbitrator",
    "create_dispute",
    "vote",
    "resolve_dispute",
    "cancel_dispute",
    "get_arbitrators_page",
];

const UPDATE_CMD: &str = "UPDATE_COST_BASELINE=1 cargo test -p credence_arbitration --locked cost";

fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();

    // register_arbitrator
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        let arb = Address::generate(&env);
        client.initialize(&admin);
        client.register_arbitrator(&arb, &100_i128);
        out.insert("register_arbitrator".into(), measure(&env));
    }

    // create_dispute
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        let arb = Address::generate(&env);
        let creator = Address::generate(&env);
        client.initialize(&admin);
        client.register_arbitrator(&arb, &100_i128);
        let desc = SorobanString::from_str(&env, "bond slash dispute");
        client.create_dispute(&creator, &desc, &1_000_u64);
        out.insert("create_dispute".into(), measure(&env));
    }

    // vote
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        let arb = Address::generate(&env);
        let creator = Address::generate(&env);
        env.ledger().set_timestamp(100);
        client.initialize(&admin);
        client.register_arbitrator(&arb, &100_i128);
        let desc = SorobanString::from_str(&env, "bond slash dispute");
        let dispute_id = client.create_dispute(&creator, &desc, &1_000_u64);
        client.vote(&arb, &dispute_id, &1_u32);
        out.insert("vote".into(), measure(&env));
    }

    // resolve_dispute
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        let arb = Address::generate(&env);
        let creator = Address::generate(&env);
        env.ledger().set_timestamp(100);
        client.initialize(&admin);
        client.register_arbitrator(&arb, &100_i128);
        let desc = SorobanString::from_str(&env, "bond slash dispute");
        let dispute_id = client.create_dispute(&creator, &desc, &1_000_u64);
        client.vote(&arb, &dispute_id, &1_u32);
        env.ledger().set_timestamp(1_200);
        client.resolve_dispute(&dispute_id);
        out.insert("resolve_dispute".into(), measure(&env));
    }

    // cancel_dispute
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        let arb = Address::generate(&env);
        let creator = Address::generate(&env);
        client.initialize(&admin);
        client.register_arbitrator(&arb, &100_i128);
        let desc = SorobanString::from_str(&env, "bond slash dispute");
        let dispute_id = client.create_dispute(&creator, &desc, &1_000_u64);
        let reason = SorobanString::from_str(&env, "withdrawn by creator");
        client.cancel_dispute(&creator, &dispute_id, &reason);
        out.insert("cancel_dispute".into(), measure(&env));
    }

    // get_arbitrators_page
    {
        let env = fresh_env();
        let client = CredenceArbitrationClient::new(&env, &env.register(CredenceArbitration, ()));
        let admin = Address::generate(&env);
        client.initialize(&admin);
        for _ in 0..5 {
            let arb = Address::generate(&env);
            client.register_arbitrator(&arb, &100_i128);
        }
        client.get_arbitrators_page(&0_u32, &10_u32);
        out.insert("get_arbitrators_page".into(), measure(&env));
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
    let vote = regressed.get_mut("vote").expect("vote measured");
    vote.cpu_insns = (vote.cpu_insns as f64 * 1.15) as i64 + 1;

    let regressions = diff(ENTRYPOINTS, &baseline, &regressed);
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].entrypoint, "vote");
    assert_eq!(regressions[0].metric, "cpu_insns");
}
