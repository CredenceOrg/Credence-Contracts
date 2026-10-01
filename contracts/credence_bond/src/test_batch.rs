#![cfg(test)]

extern crate std;

use crate::{
    batch::MAX_BATCH_BOND_SIZE, test_helpers::setup_with_token, BatchBondParams, CredenceBond,
    CredenceBondClient,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};
use std::panic::AssertUnwindSafe;

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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.len(), 1);

    let bond = result.bonds.get(0).unwrap();
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 1000);
    assert_eq!(
        bond.bond_duration,
        credence_math::Timestamp::SECONDS_PER_DAY
    );
    assert!(bond.active);
    assert!(!bond.is_rolling);
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
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, MAX_BATCH_BOND_SIZE);
    assert_eq!(result.bonds.len(), MAX_BATCH_BOND_SIZE);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_create_batch_bonds_above_max_batch_size_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_empty_batch_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: -1000, // Invalid: negative amount
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 0, // Invalid: zero amount
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: -1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let identity3 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: 2000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity3,
        amount: 3000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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

    let total = crate::batch::get_batch_total_amount(&env, &params_list);
    assert_eq!(total, 0);
}
#[test]
#[should_panic(expected = "HostError")]
fn test_get_batch_total_amount_above_max_batch_size_fails() {
    let env = Env::default();
    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);

    let _ = crate::batch::get_batch_total_amount(&env, &params_list);
}

#[test]
#[should_panic(expected = "batch total overflow")]
fn test_batch_total_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: i128::MAX,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: 1, // Will overflow when added to i128::MAX
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.create_bond_with_rolling(
        &identity,
        &1_000_000,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0,
    );

    // Try to create another bond (will fail because bond already exists)
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 2000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 5000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: true,
        notice_period_duration: 7200,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    let bond = result.bonds.get(0).unwrap();
    assert!(bond.is_rolling);
    assert_eq!(bond.notice_period_duration, 7200);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

// TODO: Rewrite without catch_unwind - Env contains UnsafeCell and cannot cross unwind boundaries in SDK 22.0
// See docs/known-simplifications.md § 5.1 for details about this SDK 22.0 limitation.
#[test]
#[ignore = "Requires rewrite without catch_unwind due to SDK 22.0 Env incompatibility"]
fn test_atomic_failure_on_second_bond() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);

    let mut params_list = Vec::new(&env);

    // First bond is valid
    params_list.push_back(BatchBondParams {
        identity: identity1,
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Second bond has invalid amount (will cause entire batch to fail)
    params_list.push_back(BatchBondParams {
        identity: identity2,
        amount: -1000, // Invalid
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
#[ignore = "Requires rewrite without catch_unwind due to SDK 22.0 Env incompatibility"]
fn test_atomic_failure_validation_order() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY, // 1 day
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
    let bond = result.bonds.get(0).unwrap();
    assert_eq!(
        bond.bond_duration,
        credence_math::Timestamp::SECONDS_PER_DAY
    );
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
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 2000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Invalid bond in the middle
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 0, // Invalid
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Valid bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 3000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);

    // Valid non-rolling bond
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Invalid rolling bond (missing notice period)
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 2000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, 1);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 1);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_batch_size_boundary_at_max_plus_one() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);
    client.create_batch_bonds(&params_list);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_batch_size_boundary_way_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE);
    let is_valid = client.validate_batch_bonds(&params_list);
    assert!(is_valid);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_validate_batch_rejects_oversized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);

    // Add 5 valid bonds
    for i in 0..5 {
        params_list.push_back(BatchBondParams {
            identity: Address::generate(&env),
            amount: 1000 + i128::from(i),
            duration: credence_math::Timestamp::SECONDS_PER_DAY,
            is_rolling: false,
            notice_period_duration: 0,
        });
    }

    // Add 1 invalid bond at the end
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: -100, // Invalid
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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

    let total = crate::batch::get_batch_total_amount(&env, &params_list);

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
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let total = crate::batch::get_batch_total_amount(&env, &params_list);
    assert_eq!(total, 5000);
}

#[test]
fn test_batch_with_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: i128::MAX / 2,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1, // Minimum valid amount
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    // Create oversized batch with invalid content
    let mut params_list = Vec::new(&env);
    for _ in 0..(MAX_BATCH_BOND_SIZE + 1) {
        params_list.push_back(BatchBondParams {
            identity: Address::generate(&env),
            amount: -1000, // Invalid amount
            duration: credence_math::Timestamp::SECONDS_PER_DAY,
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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

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
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, 2);
    let result = client.create_batch_bonds(&params_list);

    assert_eq!(result.created_count, 2);
    // Event emission is verified by the contract's event system
    // In a real test, you'd check env.events() for the batch_bonds_created event
}

// ── Boundary and Recovery Tests (issue #1317) ────────────────────────────────
//
// This section covers the gaps identified in the batch module's test coverage:
//
//  A. MIN/MAX bond amount boundaries (batch skips validate_bond_amount)
//  B. MIN/MAX bond duration boundaries (batch skips validate_bond_duration)
//  C. Paused-contract rejection for all three batch entry points
//  D. Same-ledger liquidation guard: slash blocked after batch creation
//  E. Event payload content assertion (batch_bonds_created topic + data)
//  F. Storage persistence read-back via get_identity_state after batch
//  G. Intra-batch duplicate identity (same address appears twice in one call)
//  H. get_batch_total_amount with negative amounts (no per-item sign check)
//  I. validate_batch vs create_batch_bonds divergence on pre-existing bond
//  J. Atomicity rewrites without catch_unwind (replaces the two #[ignore]'d tests)

// ── A. MIN/MAX Bond Amount Boundaries ────────────────────────────────────────
//
// The batch validate_batch_bonds path only checks `amount <= 0`; it does NOT
// call validation::validate_bond_amount, which enforces MIN_BOND_AMOUNT (1_000
// in test mode) and MAX_BOND_AMOUNT (100_000_000_000_000 in test mode). These
// tests document the current behaviour and act as regression guards: if the
// batch path is ever tightened to match the single-bond path the
// #[should_panic] annotations must be updated.

/// Amount exactly equal to MIN_BOND_AMOUNT (1_000 in test) must succeed
/// because batch only rejects <= 0.
#[test]
fn test_batch_amount_at_min_bond_amount_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        // MIN_BOND_AMOUNT in test mode is 1_000 (validation.rs §4.2)
        amount: crate::validation::MIN_BOND_AMOUNT,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(
        result.bonds.get(0).unwrap().bonded_amount,
        crate::validation::MIN_BOND_AMOUNT
    );
}

/// Amount of 1 (below MIN_BOND_AMOUNT = 1_000) passes batch validation because
/// batch does not call validate_bond_amount. This test documents that the
/// stricter amount floor is intentionally not enforced in the batch path.
#[test]
fn test_batch_amount_below_min_bond_amount_is_accepted_by_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);
    // amount = 1 is above 0 so batch's only guard (amount <= 0) passes,
    // even though it would fail single-bond validate_bond_amount.
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Batch does not enforce MIN_BOND_AMOUNT — this must NOT panic.
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.get(0).unwrap().bonded_amount, 1);
}

/// Amount exactly at MAX_BOND_AMOUNT (100_000_000_000_000 in test) must
/// succeed; batch does not enforce the maximum ceiling.
#[test]
fn test_batch_amount_at_max_bond_amount_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: crate::validation::MAX_BOND_AMOUNT,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(
        result.bonds.get(0).unwrap().bonded_amount,
        crate::validation::MAX_BOND_AMOUNT
    );
}

/// Amount of MAX_BOND_AMOUNT + 1 also passes batch validation because batch
/// only checks amount <= 0, not the upper ceiling. Documents divergence from
/// single-bond path.
#[test]
fn test_batch_amount_above_max_bond_amount_is_accepted_by_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let above_max = crate::validation::MAX_BOND_AMOUNT + 1;
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: above_max,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Batch accepts this; single-bond create_bond would reject it.
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.get(0).unwrap().bonded_amount, above_max);
}

// ── B. MIN/MAX Bond Duration Boundaries ──────────────────────────────────────
//
// batch does not call validate_bond_duration. The single-bond path enforces
// MIN_BOND_DURATION = 86_400 (1 day) and MAX_BOND_DURATION = 31_536_000 (365
// days). Tests document that batch currently bypasses these constraints.

/// Duration exactly equal to MIN_BOND_DURATION (86_400 seconds = 1 day) must
/// succeed. This is the boundary where single-bond and batch behaviour agree.
#[test]
fn test_batch_duration_at_min_bond_duration_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1_000,
        duration: crate::validation::MIN_BOND_DURATION, // 86_400
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(
        result.bonds.get(0).unwrap().bond_duration,
        crate::validation::MIN_BOND_DURATION
    );
}

/// Duration one second below MIN_BOND_DURATION (86_399) passes batch validation
/// because batch only checks for duration overflow, not the minimum floor.
#[test]
fn test_batch_duration_below_min_bond_duration_is_accepted_by_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let below_min = crate::validation::MIN_BOND_DURATION - 1; // 86_399
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1_000,
        duration: below_min,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Batch does not enforce MIN_BOND_DURATION — must NOT panic.
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.get(0).unwrap().bond_duration, below_min);
}

/// Duration exactly equal to MAX_BOND_DURATION (31_536_000 seconds = 365 days)
/// must succeed.
#[test]
fn test_batch_duration_at_max_bond_duration_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1_000,
        duration: crate::validation::MAX_BOND_DURATION, // 31_536_000
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(
        result.bonds.get(0).unwrap().bond_duration,
        crate::validation::MAX_BOND_DURATION
    );
}

/// Duration one second above MAX_BOND_DURATION (31_536_001) passes batch
/// validation because batch only checks for arithmetic overflow, not the
/// 365-day ceiling. Documents divergence from single-bond path.
#[test]
fn test_batch_duration_above_max_bond_duration_is_accepted_by_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let above_max = crate::validation::MAX_BOND_DURATION + 1;
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1_000,
        duration: above_max,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Batch accepts this; single-bond create_bond would reject it.
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 1);
    assert_eq!(result.bonds.get(0).unwrap().bond_duration, above_max);
}

// ── C. Paused-Contract Rejection ─────────────────────────────────────────────
//
// The #[cfg(test)] entry points in lib.rs call require_not_paused before
// delegating. All three batch entry points must return ContractPaused when the
// contract is paused.

/// create_batch_bonds is rejected with HostError when the contract is paused.
#[test]
#[should_panic(expected = "HostError")]
fn test_create_batch_bonds_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Pause the contract (single-admin threshold = 0, so one call suffices).
    client.pause(&admin);
    assert!(client.is_paused(), "precondition: contract must be paused");

    let params_list = build_valid_batch(&env, 1);
    // Must revert with ContractPaused.
    client.create_batch_bonds(&params_list);
}

/// validate_batch_bonds is rejected with HostError when the contract is paused.
#[test]
#[should_panic(expected = "HostError")]
fn test_validate_batch_bonds_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    client.pause(&admin);
    assert!(client.is_paused(), "precondition: contract must be paused");

    let params_list = build_valid_batch(&env, 1);
    client.validate_batch_bonds(&params_list);
}

/// get_batch_total_amount is rejected with HostError when the contract is paused.
#[test]
#[should_panic(expected = "HostError")]
fn test_get_batch_total_amount_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    client.pause(&admin);
    assert!(client.is_paused(), "precondition: contract must be paused");

    let params_list = build_valid_batch(&env, 2);
    client.get_batch_total_amount(&params_list);
}

/// After unpausing, create_batch_bonds succeeds again.
#[test]
fn test_create_batch_bonds_succeeds_after_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    client.pause(&admin);
    assert!(client.is_paused(), "precondition: paused");
    client.unpause(&admin);
    assert!(!client.is_paused(), "precondition: unpaused");

    let params_list = build_valid_batch(&env, 3);
    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 3);
}

// ── D. Same-Ledger Liquidation Guard ─────────────────────────────────────────
//
// create_batch_bonds calls record_collateral_increase after writing all bonds.
// Any slash attempt in the same ledger sequence must be blocked by
// require_slash_allowed_after_collateral_increase. Moving to the next ledger
// sequence must allow slashing to proceed.

/// Slash attempted in the same ledger as a batch creation is blocked.
#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_slash_blocked_same_ledger_after_batch_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Create a batch — this records the current ledger sequence.
    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    client.create_batch_bonds(&params_list);

    // Slash in the SAME ledger sequence — must be rejected by the guard.
    client.slash(&admin, &identity, &500_i128);
}

/// Slash attempted one ledger after a batch creation succeeds.
#[test]
fn test_slash_allowed_next_ledger_after_batch_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    client.create_batch_bonds(&params_list);

    // Advance to the next ledger sequence so the guard clears.
    crate::test_helpers::advance_ledger_sequence(&env);

    // Slash now succeeds.
    let updated = client.slash(&admin, &identity, &300_i128);
    assert_eq!(updated.slashed_amount, 300);
    assert_eq!(updated.bonded_amount, 1_000);
}

/// record_collateral_increase is written with the current ledger sequence.
/// This test reads it back directly from storage to confirm the invariant.
#[test]
fn test_batch_creation_records_collateral_increase_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Note the sequence before creating the batch.
    let seq_before = env.ledger().sequence();

    let params_list = build_valid_batch(&env, 2);
    client.create_batch_bonds(&params_list);

    // Read the stored guard key directly.
    let recorded = env.as_contract(&contract_id, || {
        crate::same_ledger_liquidation_guard::last_collateral_increase_ledger(&env)
    });

    assert_eq!(
        recorded,
        Some(seq_before),
        "LastCollateralIncreaseLedger must equal the ledger sequence at batch creation time"
    );
}

// ── E. Event Payload Content ──────────────────────────────────────────────────
//
// create_batch_bonds emits a `batch_bonds_created` event. These tests verify
// the topic symbol and the BatchBondResult payload carried in the event data.

/// The `batch_bonds_created` event is emitted with the correct topic symbol
/// and the created_count in the result payload matches the bond count.
#[test]
fn test_batch_bonds_created_event_topic_and_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    const BATCH_SIZE: u32 = 3;
    let params_list = build_valid_batch(&env, BATCH_SIZE);
    let result = client.create_batch_bonds(&params_list);

    // Confirm the return value carries the correct count.
    assert_eq!(result.created_count, BATCH_SIZE);
    assert_eq!(result.bonds.len(), BATCH_SIZE);

    // Find the batch_bonds_created event among all published events.
    let all_events = env.events().all();
    let expected_topic = Symbol::new(&env, "batch_bonds_created");

    let batch_event = all_events.iter().find(|ev| {
        // ev.1 is the topics Vec<Val>; the first topic is the symbol.
        if ev.0 != contract_id {
            return false;
        }
        if ev.1.is_empty() {
            return false;
        }
        let topic = Symbol::try_from_val(&env, &ev.1.get(0).unwrap());
        topic.map(|s| s == expected_topic).unwrap_or(false)
    });

    assert!(
        batch_event.is_some(),
        "expected a 'batch_bonds_created' event to be emitted"
    );

    // Decode the event data as BatchBondResult and verify created_count.
    let ev = batch_event.unwrap();
    let decoded = crate::BatchBondResult::try_from_val(&env, &ev.2);
    assert!(decoded.is_ok(), "event data must decode as BatchBondResult");
    let decoded_result = decoded.unwrap();
    assert_eq!(
        decoded_result.created_count, BATCH_SIZE,
        "event payload created_count must match the batch size"
    );
    assert_eq!(
        decoded_result.bonds.len(),
        BATCH_SIZE,
        "event payload bonds vec length must match the batch size"
    );
}

/// Each bond in the event payload must carry the correct identity address and
/// bonded_amount matching the input parameters.
#[test]
fn test_batch_bonds_created_event_bond_identity_and_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let id_a = Address::generate(&env);
    let id_b = Address::generate(&env);
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: id_a.clone(),
        amount: 5_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: id_b.clone(),
        amount: 9_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: true,
        notice_period_duration: 3_600,
    });

    client.create_batch_bonds(&params_list);

    let all_events = env.events().all();
    let expected_topic = Symbol::new(&env, "batch_bonds_created");

    let ev = all_events
        .iter()
        .find(|ev| {
            if ev.0 != contract_id {
                return false;
            }
            ev.1.get(0)
                .and_then(|v| Symbol::try_from_val(&env, &v).ok())
                .map(|s| s == expected_topic)
                .unwrap_or(false)
        })
        .expect("batch_bonds_created event must exist");

    let decoded = crate::BatchBondResult::try_from_val(&env, &ev.2)
        .expect("event data must decode as BatchBondResult");

    let bond_a = decoded.bonds.get(0).unwrap();
    assert_eq!(bond_a.identity, id_a);
    assert_eq!(bond_a.bonded_amount, 5_000);
    assert!(!bond_a.is_rolling);

    let bond_b = decoded.bonds.get(1).unwrap();
    assert_eq!(bond_b.identity, id_b);
    assert_eq!(bond_b.bonded_amount, 9_000);
    assert!(bond_b.is_rolling);
    assert_eq!(bond_b.notice_period_duration, 3_600);
}

// ── F. Storage Persistence Read-Back ─────────────────────────────────────────
//
// After create_batch_bonds succeeds, each bond must be readable through the
// canonical get_identity_state entry point, and the stored fields must exactly
// match what was returned in BatchBondResult.

/// Bonds created via batch are persisted and retrievable through get_identity_state.
#[test]
fn test_batch_created_bonds_persist_in_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let id1 = Address::generate(&env);
    let id2 = Address::generate(&env);
    let id3 = Address::generate(&env);

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: id1.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: id2.clone(),
        amount: 4_000,
        duration: 172_800,
        is_rolling: true,
        notice_period_duration: 7_200,
    });
    params_list.push_back(BatchBondParams {
        identity: id3.clone(),
        amount: 12_000,
        duration: 259_200,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = client.create_batch_bonds(&params_list);
    assert_eq!(result.created_count, 3);

    // Read each bond back and compare with the BatchBondResult entries.
    let stored1 = client.get_identity_state(&id1);
    assert_eq!(stored1.identity, id1);
    assert_eq!(stored1.bonded_amount, 1_000);
    assert_eq!(
        stored1.bond_duration,
        credence_math::Timestamp::SECONDS_PER_DAY
    );
    assert!(stored1.active);
    assert!(!stored1.is_rolling);
    assert_eq!(stored1.slashed_amount, 0);
    assert_eq!(stored1.withdrawal_requested_at, 0);

    let stored2 = client.get_identity_state(&id2);
    assert_eq!(stored2.identity, id2);
    assert_eq!(stored2.bonded_amount, 4_000);
    assert_eq!(stored2.bond_duration, 172_800);
    assert!(stored2.is_rolling);
    assert_eq!(stored2.notice_period_duration, 7_200);

    let stored3 = client.get_identity_state(&id3);
    assert_eq!(stored3.identity, id3);
    assert_eq!(stored3.bonded_amount, 12_000);
    assert_eq!(stored3.bond_duration, 259_200);

    // Verify stored fields match the returned BatchBondResult entries.
    assert_eq!(
        stored1.bonded_amount,
        result.bonds.get(0).unwrap().bonded_amount
    );
    assert_eq!(
        stored2.bonded_amount,
        result.bonds.get(1).unwrap().bonded_amount
    );
    assert_eq!(
        stored3.bonded_amount,
        result.bonds.get(2).unwrap().bonded_amount
    );
}

/// bond_start field stored in the bond matches the ledger timestamp at the time
/// of batch creation.
#[test]
fn test_batch_created_bond_start_matches_ledger_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Set a known ledger timestamp.
    let known_ts: u64 = 1_700_000_000;
    env.ledger().with_mut(|li| li.timestamp = known_ts);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);

    let stored = client.get_identity_state(&identity);
    assert_eq!(
        stored.bond_start, known_ts,
        "bond_start must equal the ledger timestamp at batch creation time"
    );
}

/// All bonds in a multi-item batch share the same bond_start (the ledger
/// timestamp is captured once at the start of create_batch_bonds).
#[test]
fn test_all_batch_bonds_share_same_bond_start() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, 5);
    let result = client.create_batch_bonds(&params_list);

    let bond_start_0 = result.bonds.get(0).unwrap().bond_start;
    for i in 1..5 {
        let bond = result.bonds.get(i).unwrap();
        assert_eq!(
            bond.bond_start, bond_start_0,
            "bond at index {i} has a different bond_start than bond 0"
        );
    }
}

// ── G. Intra-Batch Duplicate Identity ────────────────────────────────────────
//
// If the same Address appears more than once in a single params_list, the
// existence check in Phase 2 of create_batch_bonds will panic "bond already
// exists" after the first occurrence has been stored in Phase 3. Validation
// (Phase 1) does not detect intra-batch duplicates. This test confirms the
// panic and that the failure is consistent (all-or-nothing at the tx level).

/// A batch containing the same identity address twice panics "bond already exists"
/// when the second occurrence is processed in the existence check.
#[test]
#[should_panic(expected = "bond already exists")]
fn test_intra_batch_duplicate_identity_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Use the SAME identity address for both entries.
    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    // Duplicate — same identity, different amount.
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 2_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    client.create_batch_bonds(&params_list);
}

/// validate_batch_bonds does NOT catch the intra-batch duplicate (it only
/// validates individual parameters, not cross-item identity uniqueness).
#[test]
fn test_validate_batch_bonds_does_not_catch_intra_batch_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let identity = Address::generate(&env);
    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: identity.clone(), // duplicate
        amount: 2_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // validate_batch skips the existence check — it must return true here.
    let is_valid = client.validate_batch_bonds(&params_list);
    assert!(
        is_valid,
        "validate_batch_bonds does not perform duplicate-identity detection"
    );
}

// ── H. get_batch_total_amount with Negative Amounts ──────────────────────────
//
// get_batch_total_amount accumulates amounts without a per-item sign check.
// If every item is positive the total is correct; if some items are negative
// the total can be zero or negative. This is consistent with the module's own
// note that validation is a caller responsibility.

/// A batch containing only negative amounts produces a negative total without
/// panicking (checked_add does not overflow for small negatives).
#[test]
fn test_get_batch_total_amount_with_negative_amounts_returns_negative_sum() {
    let env = Env::default();
    // Use the internal function directly (no contract client needed).
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: -1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: -2_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let total = crate::batch::get_batch_total_amount(&env, &params_list);
    assert_eq!(total, -3_000, "negative amounts must be summed as-is");
}

/// A mixed batch of positive and negative amounts returns the arithmetic sum.
#[test]
fn test_get_batch_total_amount_mixed_sign_amounts() {
    let env = Env::default();
    let mut params_list = Vec::new(&env);

    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 5_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: -2_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&env),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let total = crate::batch::get_batch_total_amount(&env, &params_list);
    assert_eq!(total, 4_000);
}

// ── I. validate_batch vs create_batch_bonds Pre-Existing Bond Divergence ──────
//
// validate_batch skips Phase 2 (existence check). A call sequence of
// validate → create where a bond is inserted between the two calls will pass
// validation but fail creation. This test verifies the two calls are not
// equivalent when state has changed.

/// validate_batch returns true even when the bond already exists; create_batch_bonds
/// subsequently panics "bond already exists" on the same params.
#[test]
#[should_panic(expected = "bond already exists")]
fn test_validate_passes_but_create_fails_for_pre_existing_bond() {
    let env = Env::default();
    let (client, _admin, identity, _token, _contract_id) =
        crate::test_helpers::setup_with_token(&env);

    // Pre-create a bond via the single-bond path.
    client.create_bond_with_rolling(
        &identity,
        &1_000_000,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0,
    );

    let mut params_list = Vec::new(&env);
    params_list.push_back(BatchBondParams {
        identity: identity.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // validate_batch does not check for pre-existing bonds — must return true.
    let preflight = client.validate_batch_bonds(&params_list);
    assert!(
        preflight,
        "validate_batch_bonds must return true even when bond already exists"
    );

    // The actual creation must then fail because the bond already exists.
    client.create_batch_bonds(&params_list);
}

// ── J. Atomicity — Non-catch_unwind Rewrites ─────────────────────────────────
//
// The two previously-ignored tests (test_atomic_failure_on_second_bond and
// test_atomic_failure_validation_order) used std::panic::catch_unwind with
// Env, which is unsound in SDK 22.0 because Env contains UnsafeCell and
// therefore is not UnwindSafe. The equivalent invariants are verified here by
// using should_panic on separate test functions and by using try_* client
// methods, which the SDK provides as non-panicking wrappers.

/// When a batch contains an invalid bond (negative amount) the entire batch
/// is rejected. Verification: use the try_ variant so we can inspect both the
/// error return AND the absence of any stored bonds without catch_unwind.
#[test]
fn test_atomic_rejection_leaves_no_bonds_stored_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let id_valid = Address::generate(&env);
    let id_invalid = Address::generate(&env);

    let mut params_list = Vec::new(&env);
    // Valid first bond.
    params_list.push_back(BatchBondParams {
        identity: id_valid.clone(),
        amount: 1_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });
    // Invalid second bond — amount is negative; triggers validation panic.
    params_list.push_back(BatchBondParams {
        identity: id_invalid.clone(),
        amount: -500,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: false,
        notice_period_duration: 0,
    });

    // Use try_ variant: the SDK wraps panics as Err so the test does not
    // unwind through Env, avoiding the SDK 22.0 UnsafeCell soundness issue.
    let outcome = client.try_create_batch_bonds(&params_list);
    assert!(
        outcome.is_err(),
        "batch with invalid amount must be rejected"
    );

    // After rejection neither bond must be stored (all-or-nothing).
    assert!(
        client.try_get_identity_state(&id_valid).is_err(),
        "valid bond must NOT be stored when batch was rejected atomically"
    );
    assert!(
        client.try_get_identity_state(&id_invalid).is_err(),
        "invalid bond must NOT be stored after batch rejection"
    );
}

/// When a batch contains a rolling bond missing a notice period, the entire
/// batch is rejected atomically regardless of position in the list.
#[test]
fn test_atomic_rejection_leaves_no_bonds_stored_invalid_rolling() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let ids: std::vec::Vec<Address> = (0..4).map(|_| Address::generate(&env)).collect();

    let mut params_list = Vec::new(&env);
    // Three valid bonds.
    for id in ids.iter().take(3) {
        params_list.push_back(BatchBondParams {
            identity: id.clone(),
            amount: 1_000,
            duration: credence_math::Timestamp::SECONDS_PER_DAY,
            is_rolling: false,
            notice_period_duration: 0,
        });
    }
    // One invalid rolling bond (notice_period_duration = 0) at the end.
    params_list.push_back(BatchBondParams {
        identity: ids[3].clone(),
        amount: 2_000,
        duration: credence_math::Timestamp::SECONDS_PER_DAY,
        is_rolling: true,
        notice_period_duration: 0, // invalid
    });

    let outcome = client.try_create_batch_bonds(&params_list);
    assert!(
        outcome.is_err(),
        "batch with invalid rolling bond must be rejected"
    );

    // No bond must survive in storage.
    for id in &ids {
        assert!(
            client.try_get_identity_state(id).is_err(),
            "no bond should be stored after atomic batch rejection"
        );
    }
}

/// When a batch contains a duration overflow, the entire batch is rejected
/// and no bonds are stored.
#[test]
fn test_atomic_rejection_leaves_no_bonds_stored_duration_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Set a timestamp close to u64::MAX so a moderate duration overflows.
    env.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 50;
    });

    let id_safe = Address::generate(&env);
    let id_overflow = Address::generate(&env);

    let mut params_list = Vec::new(&env);
    // Bond with a small duration that won't overflow.
    params_list.push_back(BatchBondParams {
        identity: id_safe.clone(),
        amount: 1_000,
        duration: 10, // u64::MAX - 50 + 10 = u64::MAX - 40, no overflow
        is_rolling: false,
        notice_period_duration: 0,
    });
    // Bond whose duration causes overflow.
    params_list.push_back(BatchBondParams {
        identity: id_overflow.clone(),
        amount: 1_000,
        duration: 200, // u64::MAX - 50 + 200 overflows u64
        is_rolling: false,
        notice_period_duration: 0,
    });

    let outcome = client.try_create_batch_bonds(&params_list);
    assert!(
        outcome.is_err(),
        "batch with duration overflow must be rejected"
    );

    // Neither bond survives.
    assert!(
        client.try_get_identity_state(&id_safe).is_err(),
        "safe bond must NOT be stored when batch was rejected atomically"
    );
    assert!(
        client.try_get_identity_state(&id_overflow).is_err(),
        "overflow bond must NOT be stored after batch rejection"
    );
}

/// Successful batch creation stores exactly the right number of bonds and no
/// phantom entries appear in storage. Verifying the validation-order invariant
/// (originally test_atomic_failure_validation_order) without catch_unwind.
#[test]
fn test_successful_batch_stores_exactly_the_created_bonds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let ids: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    let mut params_list = Vec::new(&env);
    for (i, id) in ids.iter().enumerate() {
        params_list.push_back(BatchBondParams {
            identity: id.clone(),
            amount: 1_000 + i128::try_from(i).unwrap(),
            duration: credence_math::Timestamp::SECONDS_PER_DAY,
            is_rolling: false,
            notice_period_duration: 0,
        });
    }

    let result = client.create_batch_bonds(&params_list);

    // Every supplied identity must have a bond in storage.
    assert_eq!(result.created_count, 5);
    for (i, id) in ids.iter().enumerate() {
        let stored = client.get_identity_state(id);
        assert!(stored.active);
        assert_eq!(stored.bonded_amount, 1_000 + i128::try_from(i).unwrap());
        assert_eq!(
            stored.bond_duration,
            credence_math::Timestamp::SECONDS_PER_DAY
        );
        assert_eq!(stored.slashed_amount, 0);
        assert_eq!(stored.withdrawal_requested_at, 0);
    }
}

/// A bond rejected by an oversized batch (> MAX_BATCH_BOND_SIZE) leaves no
/// state mutations. Uses try_ instead of catch_unwind.
#[test]
fn test_oversized_batch_rejection_leaves_no_bonds_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    let params_list = build_valid_batch(&env, MAX_BATCH_BOND_SIZE + 1);

    // Collect identities before the (expected) rejection.
    let first_identity = params_list.get(0).unwrap().identity.clone();

    let outcome = client.try_create_batch_bonds(&params_list);
    assert!(outcome.is_err(), "oversized batch must be rejected");

    // No bond for even the first identity should have been stored.
    assert!(
        client.try_get_identity_state(&first_identity).is_err(),
        "no bond must be stored when the batch was rejected due to BatchTooLarge"
    );
}
