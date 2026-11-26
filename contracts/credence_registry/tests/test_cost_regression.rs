#![allow(clippy::disallowed_macros)]

//! Cost regression test for `credence_registry`.
//!
//! Guards CPU, memory, and ledger read/write footprint of core registry
//! entrypoints (including bounded pagination) against `cost_baseline.json`
//! with a 10% tolerance.
//!
//! To refresh the baseline after an intentional cost change:
//! `UPDATE_COST_BASELINE=1 cargo test -p credence_registry --locked cost`

use std::collections::BTreeMap;

use credence_registry::{CredenceRegistry, CredenceRegistryClient};
use soroban_sdk::{testutils::Address as _, Address};
use testutils::cost_harness::{
    assert_cost_baseline, diff, fresh_env, measure, Baseline, EntryCost, TOLERANCE_PCT,
};

const ENTRYPOINTS: &[&str] = &[
    "register",
    "get_bond_contract",
    "get_identity",
    "is_registered",
    "deactivate",
    "get_identities_page",
];

const UPDATE_CMD: &str = "UPDATE_COST_BASELINE=1 cargo test -p credence_registry --locked cost";

fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();

    // register
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        let bond = Address::generate(&env);
        client.initialize(&admin);
        client.register(&identity, &bond, &true);
        out.insert("register".into(), measure(&env));
    }

    // get_bond_contract
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        let bond = Address::generate(&env);
        client.initialize(&admin);
        client.register(&identity, &bond, &true);
        client.get_bond_contract(&identity);
        out.insert("get_bond_contract".into(), measure(&env));
    }

    // get_identity
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        let bond = Address::generate(&env);
        client.initialize(&admin);
        client.register(&identity, &bond, &true);
        client.get_identity(&bond);
        out.insert("get_identity".into(), measure(&env));
    }

    // is_registered
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        let bond = Address::generate(&env);
        client.initialize(&admin);
        client.register(&identity, &bond, &true);
        client.is_registered(&identity);
        out.insert("is_registered".into(), measure(&env));
    }

    // deactivate
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        let bond = Address::generate(&env);
        client.initialize(&admin);
        client.register(&identity, &bond, &true);
        client.deactivate(&identity);
        out.insert("deactivate".into(), measure(&env));
    }

    // get_identities_page
    {
        let env = fresh_env();
        let client = CredenceRegistryClient::new(&env, &env.register(CredenceRegistry, ()));
        let admin = Address::generate(&env);
        client.initialize(&admin);
        for _ in 0..5 {
            let identity = Address::generate(&env);
            let bond = Address::generate(&env);
            client.register(&identity, &bond, &true);
        }
        client.get_identities_page(&0_u32, &10_u32);
        out.insert("get_identities_page".into(), measure(&env));
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
    let page = regressed
        .get_mut("get_identities_page")
        .expect("get_identities_page measured");
    page.cpu_insns = (page.cpu_insns as f64 * 1.15) as i64 + 1;

    let regressions = diff(ENTRYPOINTS, &baseline, &regressed);
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].entrypoint, "get_identities_page");
    assert_eq!(regressions[0].metric, "cpu_insns");
}
