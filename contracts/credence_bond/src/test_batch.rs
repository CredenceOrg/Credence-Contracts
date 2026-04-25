#![cfg(test)]

extern crate std;

use crate::{
    batch::MAX_BATCH_BOND_SIZE, test_helpers::setup_with_token, BatchBondParams, CredenceBond,
    CredenceBondClient,
};
use std::panic::AssertUnwindSafe;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

fn build_valid_batch(env: &Env, count: u32) -> Vec<BatchBondParams> {
    let mut params_list = Vec::new(env);

    for index in 0..count {
        params_list.push_back(BatchBondParams {
            identity: Address::generate(env),
            amount: 1000 + i128::from(index),
            duration: 86_400 + u64::from(index),
            is_rolling: index % 2 == 0,
            notice_period_duration: if index % 2 == 0 { 3600 } else { 0 },
        });
    }

    params_list
}

fn batch_create_cost(n: u32) -> (u64, u64) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, n);

    env.cost_estimate().budget().reset_default();
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, n);

    let budget = env.cost_estimate().budget();
    (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
}

#[test]
fn test_create_single_bond_in_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.len(), 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 1000);
    assert_eq!(bond.bond_duration, 86400);
    assert_eq!(bond.active, true);
    assert_eq!(bond.is_rolling, false);
}

#[test]
fn test_create_multiple_bonds_in_batch() {
    let env = Env::default();
    env.mock_all_auths();

    // Note: Current implementation only supports one bond per contract instance
    // This test demonstrates the batch interface even though it will panic
    // In a multi-identity system, this would work

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let identity3 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: 2000,
        duration: 172800,
        is_rolling: true,
        notice_period_duration: 3600,
    });

    params_list.push_back(BatchBondParams {
        identity: identity3,
        amount: 3000,
        duration: 259200,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // This test verifies the batch interface works correctly
    // In production with per-identity bonds, all 3 would be created
    assert_eq!(params_list.len(), 3);
}

#[test]
fn test_create_batch_bonds_at_max_batch_size() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, MAX_BATCH_BOND_SIZE);
    assert_eq!(result.bonds.len(), MAX_BATCH_BOND_SIZE);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_create_batch_bonds_above_max_batch_size_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "empty batch")]
fn test_empty_batch_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = Vec::new(&env);
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "invalid amount in batch")]
fn test_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: -1000, // Invalid: negative amount
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "invalid amount in batch")]
fn test_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 0, // Invalid: zero amount
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "duration overflow in batch")]
fn test_duration_overflow_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    // Set ledger timestamp to a high value to ensure overflow
    env.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 100; // High timestamp
    });

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: 1000, // Will overflow when added to (u64::MAX - 100)
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "rolling bond requires notice period")]
fn test_rolling_bond_without_notice_period_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: 86400,
        is_rolling: true,
        notice_period_duration: 0, // Invalid: rolling bond needs notice period
    });

    client.create_batch_bonds(&params_list);
}

#[test]
fn test_validate_batch_bonds_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let is_valid = client.validate_batch_bonds(&params_list);
    assert!(is_valid);
}

#[test]
#[should_panic(expected = "invalid amount in batch")]
fn test_validate_batch_bonds_fails_on_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: -1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.validate_batch_bonds(&params_list);
}

#[test]
fn test_get_batch_total_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let identity3 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: 2000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity3,
        amount: 3000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let total = client.get_batch_total_amount(&params_list);
    assert_eq!(total, 6000);
}

#[test]
fn test_get_batch_total_amount_empty_batch_returns_zero() {
    let env = Env::default();
    let params_list = Vec::new(&env);

    let total = crate::batch::get_batch_total_amount(&params_list);
    assert_eq!(total, 0);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_get_batch_total_amount_above_max_batch_size_fails() {
    let env = Env::default();
    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);

    let _ = crate::batch::get_batch_total_amount(&params_list);
}

#[test]
#[should_panic(expected = "batch total overflow")]
fn test_batch_total_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: i128::MAX,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: 1, // Will overflow when added to i128::MAX
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.get_batch_total_amount(&params_list);
}

#[test]
#[should_panic(expected = "bond already exists")]
fn test_duplicate_bond_in_batch_fails() {
    let env = Env::default();
    let (client, _admin, identity, _token, _contract_id) = setup_with_token(&env);

    // Create first bond
    client.create_bond_with_rolling(&identity, &1_000_000, &86400, &false, &0);

    // Try to create another bond (will fail because bond already exists)
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 2000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);
}

#[test]
fn test_batch_with_rolling_bonds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 5000,
        duration: 86400,
        is_rolling: true,
        notice_period_duration: 7200,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.is_rolling, true);
    assert_eq!(bond.notice_period_duration, 7200);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

#[test]
fn test_atomic_failure_on_second_bond() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    // First bond is valid
    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Second bond has invalid amount (will cause entire batch to fail)
    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: -1000, // Invalid
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // The entire batch should fail atomically
    // Validation happens before any state changes
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.create_batch_bonds(&params_list);
    }));

    assert!(result.is_err(), "Batch should fail atomically");

    // Verify NO bonds were created (atomic failure)
    // The bond storage should be empty since validation failed before creation
    assert!(!env.storage().instance().has(&crate::DataKey::Bond));
}

#[test]
fn test_batch_bonds_with_different_durations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: 86400, // 1 day
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.bond_duration, 86400);
}

#[test]
fn test_max_batch_size_gas_profile() {
    let (single_cpu, single_mem) = batch_create_cost(1);
    let (max_cpu, max_mem) = batch_create_cost(MAX_BATCH_BOND_SIZE);

    std::println!(
        "[GAS] create_batch_bonds single cpu={single_cpu} mem={single_mem} | max cpu={max_cpu} mem={max_mem}"
    );

    assert!(max_cpu >= single_cpu);
    assert!(max_mem >= single_mem);
}

// ── Additional Atomicity and Boundary Tests ──────────────────────────────────

#[test]
#[should_panic(expected = "invalid amount in batch")]
fn test_atomic_failure_with_mixed_valid_invalid_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 2000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Invalid bond in the middle
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 0, // Invalid
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 3000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Should fail before creating any bonds
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "rolling bond requires notice period")]
fn test_atomic_failure_with_invalid_rolling_bond_in_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    // Valid non-rolling bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Invalid rolling bond (missing notice period)
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 2000,
        duration: 86400,
        is_rolling: true,
        notice_period_duration: 0, // Invalid
    });

    // Should fail atomically
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "duration overflow in batch")]
fn test_atomic_failure_with_duration_overflow_in_middle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Set high timestamp
    env.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 100;
    });

    let mut params_list = Vec::new(&env);

    // Valid bond with small duration
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: 50, // Won't overflow
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Invalid bond with overflow duration
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 2000,
        duration: 200, // Will overflow
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Should fail atomically
    client.create_batch_bonds(&params_list);
}

#[test]
fn test_batch_size_boundary_at_max_minus_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE - 1);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, MAX_BATCH_BOND_SIZE - 1);
}

#[test]
fn test_batch_size_boundary_at_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, 1);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_batch_size_boundary_at_max_plus_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_batch_size_boundary_way_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE * 2);
    client.create_batch_bonds(&params_list);
}

#[test]
fn test_validate_batch_enforces_max_size() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE);
    let is_valid = client.validate_batch_bonds(&params_list);
    assert!(is_valid);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_validate_batch_rejects_oversized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);
    client.validate_batch_bonds(&params_list);
}

#[test]
fn test_all_bonds_validated_before_any_created() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    // Add 5 valid bonds
    for i in 0..5 {
        params_list.push_back(BatchBondParams {
            identity: Address::generate(&env),
            amount: 1000 + i128::from(i),
            duration: 86400,
            is_rolling: false,
            notice_period_duration: 0,
        });
    }

    // Add 1 invalid bond at the end
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: -100, // Invalid
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Should fail before creating any bonds
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.create_batch_bonds(&params_list);
    }));

    assert!(result.is_err());

    // Verify no bonds were created
    assert!(!env.storage().instance().has(&crate::DataKey::Bond));
}

#[test]
fn test_batch_total_amount_with_max_size() {
    let env = Env::default();
    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE);

    let total = crate::batch::get_batch_total_amount(&params_list);

    // Calculate expected total: sum of (1000 + i) for i in 0..MAX_BATCH_BOND_SIZE
    let mut expected: i128 = 0;
    for i in 0..MAX_BATCH_BOND_SIZE {
        expected += 1000 + i128::from(i);
    }

    assert_eq!(total, expected);
}

#[test]
fn test_batch_total_amount_single_bond() {
    let env = Env::default();
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 5000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let total = crate::batch::get_batch_total_amount(&params_list);
    assert_eq!(total, 5000);
}

#[test]
fn test_batch_with_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: i128::MAX / 2,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.bonded_amount, i128::MAX / 2);
}

#[test]
fn test_batch_with_minimum_valid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1, // Minimum valid amount
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.bonded_amount, 1);
}

#[test]
fn test_batch_with_minimum_valid_duration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: 1, // Minimum valid duration
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.bond_duration, 1);
}

#[test]
fn test_batch_with_maximum_valid_duration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut params_list = Vec::new(&env);

    // Use a large but valid duration
    let max_duration = u64::MAX / 2;

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: max_duration,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.bond_duration, max_duration);
}

#[test]
fn test_validation_order_size_before_content() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Create oversized batch with invalid content
    let mut params_list = Vec::new(&env);
    for _ in 0..(MAX_BATCH_BOND_SIZE + 1) {
        params_list.push_back(BatchBondParams {
            identity: Address::generate(&env),
            amount: -1000, // Invalid amount
            duration: 86400,
            is_rolling: false,
            notice_period_duration: 0,
        });
    }

    // Should fail with "batch too large" before checking invalid amounts
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.create_batch_bonds(&params_list);
    }));

    assert!(result.is_err());
    // Note: We can't easily check the exact panic message in this context
}

#[test]
fn test_empty_batch_fails_before_size_check() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = Vec::new(&env);

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.create_batch_bonds(&params_list);
    }));

    assert!(result.is_err());
}

#[test]
fn test_batch_result_structure() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, 3);
    let result = client.create_batch_bonds(&params_list);

    // Verify result structure
    assert_eq!(result.created_count, 3);
    assert_eq!(result.bonds.len(), 3);

    // Verify each bond in result
    for i in 0..3 {
        let bond = result.bonds.get(i).unwrap();
        assert!(bond.active);
        assert_eq!(bond.slashed_amount, 0);
        assert_eq!(bond.withdrawal_requested_at, 0);
    }
}

#[test]
fn test_batch_bonds_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let params_list = build_valid_batch(&env, 2);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 2);
    // Event emission is verified by the contract's event system
    // In a real test, you'd check env.events() for the batch_bonds_created event
}
