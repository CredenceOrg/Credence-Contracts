//! Shared cost-measurement harness for the `credence_bond` contract.
//!
//! Delegates generic measurement, JSON serialization/parsing, and regression
//! diffing to [`testutils::cost_harness`].
#![allow(dead_code)]
#![allow(clippy::disallowed_macros)]

use std::collections::BTreeMap;

use credence_bond::{CredenceBond, CredenceBondClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, String as SorobanString,
};

pub use testutils::cost_harness::{
    fresh_env, measure, parse_baseline, try_parse_baseline, Baseline, BaselineError, EntryCost,
    Regression, TOLERANCE_PCT,
};

/// The entrypoints we gate, in a stable, deterministic order. Adding an
/// entrypoint here makes it part of the baseline on the next refresh.
pub const ENTRYPOINTS: &[&str] = &[
    "create_bond",
    "top_up",
    "withdraw",
    "withdraw_early",
    "slash_bond",
    "add_attestation",
];

/// Drive every tracked entrypoint and return its cost, keyed by name.
pub fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();

    // Use realistic bond amounts (minimum is 1e18)
    let bond_amount = 1_000_000_000_000_000_000i128; // 1e18
    let duration = 1_000_u64;

    // create_bond — the bare happy path: one identity bonds.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        out.insert("create_bond".into(), measure(&env));
    }

    // top_up — adds to an existing bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        client.top_up(&identity, &(bond_amount / 2));
        out.insert("top_up".into(), measure(&env));
    }

    // withdraw — non-rolling bond, after the lock-up has elapsed.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(2_000);
        client.withdraw(&identity, &(bond_amount / 10));
        out.insert("withdraw".into(), measure(&env));
    }

    // withdraw_early — bond exited before lock-up end, charging the penalty.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        // With a bond token configured, amounts are in the token's own units
        // (7 decimals here) and the bond is funded by a real transfer.
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let mut accepted = soroban_sdk::Vec::new(&env);
        accepted.push_back(token_id.clone());
        client.set_accepted_tokens(&admin, &accepted);
        client.set_token(&admin, &token_id);
        let token_amount = 1_000_000_000_i128; // 100 tokens
        soroban_sdk::token::StellarAssetClient::new(&env, &token_id).mint(&identity, &token_amount);
        soroban_sdk::token::TokenClient::new(&env, &token_id).approve(
            &identity,
            &client.address,
            &token_amount,
            &1_000_u32,
        );
        client.set_early_exit_config(&admin, &treasury, &500_u32);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &token_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(100);
        client.withdraw_early(&identity, &(token_amount / 10));
        out.insert("withdraw_early".into(), measure(&env));
    }

    // slash_bond — admin slashes part of an active bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        let salt = soroban_sdk::Bytes::new(&env);
        client.slash_bond(&admin, &identity, &(bond_amount / 10), &salt);
        out.insert("slash_bond".into(), measure(&env));
    }

    // add_attestation — a registered attester attests to a subject.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let attester = Address::generate(&env);
        let subject = Address::generate(&env);
        client.initialize(&admin, &None);
        client.register_attester(&attester);
        let data = SorobanString::from_str(&env, "kyc:passed");
        client.add_attestation(&attester, &subject, &data, &client.address, &0_u64, &0_u64);
        out.insert("add_attestation".into(), measure(&env));
    }

    out
}

/// Serialize a baseline snapshot to deterministic, pretty-printed JSON.
pub fn to_json(costs: &BTreeMap<String, EntryCost>) -> String {
    testutils::cost_harness::to_json(ENTRYPOINTS, costs, TOLERANCE_PCT)
}

/// Compare a fresh measurement against the baseline and return every metric that
/// regressed by more than `baseline.tolerance_pct`.
pub fn diff(baseline: &Baseline, current: &BTreeMap<String, EntryCost>) -> Vec<Regression> {
    testutils::cost_harness::diff(ENTRYPOINTS, baseline, current)
}
