//! # Gas / Cost-Estimate Benchmark Tests — Dispute Resolution Contract
//!
//! Measures CPU-instruction and memory-byte budgets for every public function.
//! Run with `-- --nocapture` to see the benchmark table.
//!
//! ```bash
//! cargo test -p dispute_resolution -- gas --nocapture
//! ```
//!
//! See `docs/dispute_resolution_gas_benchmarks.md` for recorded numbers.

#![cfg(test)]

// The parent crate is `#![no_std]`, but test binaries always link `std`.
extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Register the dispute contract + a token, mint `mint_amount` to `recipient`.
/// Returns `(contract_id, token_id, token_client)`.
fn setup<'a>(
    env: &'a Env,
    admin: &Address,
    recipient: &Address,
    mint_amount: i128,
) -> (Address, Address, soroban_sdk::token::Client<'a>) {
    let contract_id = env.register(DisputeContract, ());
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    soroban_sdk::token::StellarAssetClient::new(env, &token_id).mint(recipient, &mint_amount);
    let token_client = soroban_sdk::token::Client::new(env, &token_id);
    (contract_id, token_id, token_client)
}

/// Approve + create a single dispute and return its ID.
fn create_one(
    env: &Env,
    client: &DisputeContractClient,
    disputer: &Address,
    contract_id: &Address,
    token_id: &Address,
    token_client: &soroban_sdk::token::Client,
    stake: i128,
    deadline: u64,
) -> u64 {
    token_client.approve(disputer, contract_id, &stake, &10_000);
    client.create_dispute(disputer, &1, &stake, token_id, &deadline)
}

/// Print a labelled budget line captured via `env.cost_estimate().budget()`.
fn print_budget(label: &str, env: &Env) {
    let b = env.cost_estimate().budget();
    let cpu = b.cpu_instruction_cost();
    let mem = b.memory_bytes_cost();
    std::println!(
        "[GAS] {:<60} | cpu_instructions: {:>12} | memory_bytes: {:>12}",
        label,
        cpu,
        mem,
    );
}

// ─── Baseline ────────────────────────────────────────────────────────────────

/// Empty test — records budget overhead of observation itself.
#[test]
fn gas_baseline_empty() {
    let env = Env::default();
    env.cost_estimate().budget().reset_default();
    print_budget("baseline (no-op)", &env);
}

// ─── create_dispute ───────────────────────────────────────────────────────────

#[test]
fn gas_create_dispute_single() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 10_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    token_client.approve(&disputer, &contract_id, &500, &10_000);

    env.cost_estimate().budget().reset_default();
    client.create_dispute(&disputer, &1, &500, &token_id, &3600);
    print_budget("create_dispute (1st, fresh counter)", &env);
}

#[test]
fn gas_create_dispute_subsequent() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 10_000);
    let client = DisputeContractClient::new(&env, &contract_id);

    // Prime the counter.
    token_client.approve(&disputer, &contract_id, &1000, &10_000);
    client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    env.cost_estimate().budget().reset_default();
    client.create_dispute(&disputer, &2, &500, &token_id, &3600);
    print_budget("create_dispute (2nd, counter already set)", &env);
}

// ─── get_dispute ──────────────────────────────────────────────────────────────

#[test]
fn gas_get_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    let _ = client.get_dispute(&id);
    print_budget("get_dispute", &env);
}

// ─── cast_vote ────────────────────────────────────────────────────────────────

#[test]
fn gas_cast_vote_first() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let arbitrator = Address::generate(&env);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    client.cast_vote(&arbitrator, &id, &true);
    print_budget("cast_vote (first vote on dispute)", &env);
}

#[test]
fn gas_cast_vote_nth() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    for _ in 0..4 {
        client.cast_vote(&Address::generate(&env), &id, &true);
    }

    let nth_arb = Address::generate(&env);
    env.cost_estimate().budget().reset_default();
    client.cast_vote(&nth_arb, &id, &false);
    print_budget("cast_vote (5th vote, dispute has 4 existing)", &env);
}

// ─── resolve_dispute ──────────────────────────────────────────────────────────

#[test]
fn gas_resolve_dispute_favor_disputer() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        100,
    );

    client.cast_vote(&Address::generate(&env), &id, &true);
    client.cast_vote(&Address::generate(&env), &id, &false);
    client.cast_vote(&Address::generate(&env), &id, &true);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    env.cost_estimate().budget().reset_default();
    client.resolve_dispute(&id);
    print_budget("resolve_dispute (FavorDisputer — stake returned)", &env);
}

#[test]
fn gas_resolve_dispute_favor_slasher() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        100,
    );

    client.cast_vote(&Address::generate(&env), &id, &false);
    client.cast_vote(&Address::generate(&env), &id, &false);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    env.cost_estimate().budget().reset_default();
    client.resolve_dispute(&id);
    print_budget("resolve_dispute (FavorSlasher — stake forfeited)", &env);
}

// ─── expire_dispute ───────────────────────────────────────────────────────────

#[test]
fn gas_expire_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        100,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    env.cost_estimate().budget().reset_default();
    client.expire_dispute(&id);
    print_budget("expire_dispute", &env);
}

// ─── has_voted ────────────────────────────────────────────────────────────────

#[test]
fn gas_has_voted_false() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let arbitrator = Address::generate(&env);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    let _ = client.has_voted(&id, &arbitrator);
    print_budget("has_voted (false — key absent)", &env);
}

#[test]
fn gas_has_voted_true() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let arbitrator = Address::generate(&env);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );
    client.cast_vote(&arbitrator, &id, &true);

    env.cost_estimate().budget().reset_default();
    let _ = client.has_voted(&id, &arbitrator);
    print_budget("has_voted (true — key present)", &env);
}

// ─── get_dispute_count ────────────────────────────────────────────────────────

#[test]
fn gas_get_dispute_count_empty() {
    let env = Env::default();
    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    env.cost_estimate().budget().reset_default();
    let _ = client.get_dispute_count();
    print_budget("get_dispute_count (counter == 0)", &env);
}

#[test]
fn gas_get_dispute_count_nonzero() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    let _ = client.get_dispute_count();
    print_budget("get_dispute_count (counter == 1)", &env);
}

// ─── Batch vs individual ──────────────────────────────────────────────────────

/// Cumulative cost of creating N disputes sequentially.
fn batch_create_cost(n: u32) -> (u64, u64) {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let mint: i128 = (n as i128) * 600;
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, mint);
    let client = DisputeContractClient::new(&env, &contract_id);
    token_client.approve(&disputer, &contract_id, &mint, &(n + 10_000));

    env.cost_estimate().budget().reset_default();
    for i in 0..n {
        client.create_dispute(&disputer, &(i as u64 + 1), &500, &token_id, &3600);
    }
    let b = env.cost_estimate().budget();
    (b.cpu_instruction_cost(), b.memory_bytes_cost())
}

#[test]
fn gas_batch_vs_individual_create_dispute() {
    std::println!("\n--- Batch vs Individual: create_dispute ---");
    for n in [1u32, 5, 10, 20] {
        let (cpu, mem) = batch_create_cost(n);
        std::println!(
            "  {:>3} dispute(s) | cpu: {:>12} | mem: {:>12} | cpu/op: {:>10}",
            n,
            cpu,
            mem,
            cpu / n as u64,
        );
    }
}

/// Cumulative cost of N cast_vote calls on the same dispute.
fn batch_vote_cost(n: u32) -> (u64, u64) {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    for _ in 0..n {
        client.cast_vote(&Address::generate(&env), &id, &true);
    }
    let b = env.cost_estimate().budget();
    (b.cpu_instruction_cost(), b.memory_bytes_cost())
}

#[test]
fn gas_batch_vs_individual_cast_vote() {
    std::println!("\n--- Batch vs Individual: cast_vote ---");
    for n in [1u32, 5, 10, 20] {
        let (cpu, mem) = batch_vote_cost(n);
        std::println!(
            "  {:>3} vote(s)     | cpu: {:>12} | mem: {:>12} | cpu/op: {:>10}",
            n,
            cpu,
            mem,
            cpu / n as u64,
        );
    }
}

// ─── Optimization probes ──────────────────────────────────────────────────────

/// Repeated `get_dispute` calls should have near-identical cost (TTL extension
/// should not compound on successive reads).
#[test]
fn gas_get_dispute_repeated_read_ttl_stability() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    let _ = client.get_dispute(&id);
    let first_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_default();
    let _ = client.get_dispute(&id);
    let second_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    std::println!(
        "[GAS] get_dispute repeated reads: 1st={} 2nd={} delta={}",
        first_cpu,
        second_cpu,
        (second_cpu as i64 - first_cpu as i64).abs(),
    );

    // Second call should cost within ±10% of first.
    let threshold = first_cpu / 10;
    assert!(
        (second_cpu as i64 - first_cpu as i64).unsigned_abs() <= threshold,
        "Repeated get_dispute cost too variable: 1st={first_cpu}, 2nd={second_cpu}",
    );
}

/// `cast_vote` (write path) must be more expensive than `has_voted` (read-only).
#[test]
fn gas_cast_vote_vs_has_voted_ratio() {
    let env = Env::default();
    env.mock_all_auths();
    let disputer = Address::generate(&env);
    let admin = Address::generate(&env);
    let (contract_id, token_id, token_client) = setup(&env, &admin, &disputer, 5_000);
    let client = DisputeContractClient::new(&env, &contract_id);
    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let id = create_one(
        &env,
        &client,
        &disputer,
        &contract_id,
        &token_id,
        &token_client,
        500,
        3600,
    );

    env.cost_estimate().budget().reset_default();
    client.cast_vote(&arb1, &id, &true);
    let vote_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_default();
    let _ = client.has_voted(&id, &arb2);
    let read_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    std::println!(
        "[GAS] cast_vote cpu={vote_cpu} vs has_voted cpu={read_cpu} (ratio: {:.2}x)",
        vote_cpu as f64 / read_cpu as f64,
    );

    assert!(
        vote_cpu > read_cpu,
        "cast_vote should cost more CPU than has_voted"
    );
}
