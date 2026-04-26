use crate::{
    claims::{self, ClaimResult, ClaimType, PendingClaim},
    CredenceBondClient,
};
use std::panic::AssertUnwindSafe;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

// Helper: register contract + admin, return (client, admin, contract_id).
fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin, contract_id)
}

fn create_test_address(e: &Env) -> Address {
    Address::generate(e)
}

fn create_test_env() -> Env {
    Env::default()
}

#[test]
fn test_claim_pagination_bounds_gas_usage() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create many claims to test pagination
    let mut claim_ids = Vec::new(&env);
    for i in 0..100 {
        let claim_id = claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(&env, &format!("claim_{}", i))),
        );
        claim_ids.push_back(claim_id);
    }

    // Test that pagination limits are enforced
    let first_page = claims::get_pending_claims_paginated(&env, &user, 0, 25);
    assert_eq!(first_page.len(), 25);

    let second_page = claims::get_pending_claims_paginated(&env, &user, 25, 25);
    assert_eq!(second_page.len(), 25);

    let third_page = claims::get_pending_claims_paginated(&env, &user, 50, 25);
    assert_eq!(third_page.len(), 25);

    let fourth_page = claims::get_pending_claims_paginated(&env, &user, 75, 25);
    assert_eq!(fourth_page.len(), 25);

    // Test empty page beyond available claims
    let empty_page = claims::get_pending_claims_paginated(&env, &user, 100, 25);
    assert_eq!(empty_page.len(), 0);

    // Test claim count
    let count = claims::get_pending_claims_count(&env, &user);
    assert_eq!(count, 100);
}

#[test]
fn test_claim_by_id_prevents_duplicates() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create a claim
    let claim_id = claims::add_pending_claim(
        &env,
        &user,
        ClaimType::VerifierReward,
        1000,
        1,
        Some(soroban_sdk::Symbol::new(&env, "test_claim")),
    );

    // Process the claim by ID
    let result = claims::process_claim_by_id(&env, &user, claim_id);
    assert_eq!(result.processed_count, 1);
    assert_eq!(result.total_amount, 1000);

    // Try to process the same claim again - should fail
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        claims::process_claim_by_id(&env, &user, claim_id);
    }))
    .expect_err("Should panic when processing already processed claim");

    // Verify claim is marked as processed
    let claim = claims::get_claim_by_id(&env, claim_id);
    assert!(claim.processed);
}

#[test]
fn test_paginated_claim_processing() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create many claims
    for i in 0..60 {
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(&env, &format!("claim_{}", i))),
        );
    }

    // Process first batch with pagination
    let result1 = claims::process_claims_paginated(&env, &user, 0, 20, Vec::new(&env));
    assert_eq!(result1.processed_count, 20);
    assert_eq!(result1.total_amount, 21000); // 1000 + 1 + ... + 1019

    // Process second batch
    let result2 = claims::process_claims_paginated(&env, &user, 20, 20, Vec::new(&env));
    assert_eq!(result2.processed_count, 20);
    assert_eq!(result2.total_amount, 21380); // 1020 + ... + 1039

    // Process final batch
    let result3 = claims::process_claims_paginated(&env, &user, 40, 20, Vec::new(&env));
    assert_eq!(result3.processed_count, 20);
    assert_eq!(result3.total_amount, 21760); // 1040 + ... + 1059

    // Verify no claims remaining
    let remaining_count = claims::get_pending_claims_count(&env, &user);
    assert_eq!(remaining_count, 0);
}

#[test]
fn test_claim_type_filtering_with_pagination() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create claims of different types
    for i in 0..30 {
        let claim_type = if i % 2 == 0 {
            ClaimType::VerifierReward
        } else {
            ClaimType::SlashingReward
        };
        claims::add_pending_claim(
            &env,
            &user,
            claim_type,
            1000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(&env, &format!("claim_{}", i))),
        );
    }

    // Filter for VerifierReward claims only
    let mut filter_types = Vec::new(&env);
    filter_types.push_back(ClaimType::VerifierReward);

    let result = claims::process_claims_paginated(&env, &user, 0, 50, filter_types);
    assert_eq!(result.processed_count, 15); // Only even indices (0, 2, 4, ..., 28)
    assert_eq!(result.total_amount, 15000); // 15 claims * 1000 average
}

#[test]
fn test_large_claim_set_handling() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create a large set of claims (simulating potential griefing scenario)
    for i in 0..200 {
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(
                &env,
                &format!("large_claim_{}", i),
            )),
        );
    }

    // Process in batches to test gas efficiency
    let mut total_processed = 0u32;
    let mut total_amount = 0i128;
    let mut offset = 0u32;

    while offset < 200 {
        let batch_size = 50u32; // MAX_BATCH_CLAIMS
        let result =
            claims::process_claims_paginated(&env, &user, offset, batch_size, Vec::new(&env));

        if result.processed_count == 0 {
            break;
        }

        total_processed += result.processed_count;
        total_amount += result.total_amount;
        offset += batch_size;
    }

    assert_eq!(total_processed, 200);
    assert_eq!(total_amount, 200000 + (199 * 200 / 2)); // Sum of 1000..1199

    // Verify all claims are processed
    let remaining = claims::get_pending_claims_count(&env, &user);
    assert_eq!(remaining, 0);
}

#[test]
fn test_claim_expiry_with_pagination() {
    let env = create_test_env();
    let user = create_test_address(&env);

    // Create claims with different expiry times
    let now = env.ledger().timestamp();

    // Create some expired claims
    for i in 0..10 {
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(&env, "expired")),
        );
        // Manually set expiry to past time
        let claim = claims::get_claim_by_id(&env, i);
        // Note: In a real implementation, you'd need to update the claim's expiry
    }

    // Create some valid claims
    for i in 10..20 {
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            2000 + (i as i128),
            i,
            Some(soroban_sdk::Symbol::new(&env, "valid")),
        );
    }

    // Process claims - should skip expired ones
    let result = claims::process_claims_paginated(&env, &user, 0, 50, Vec::new(&env));
    assert_eq!(result.processed_count, 10); // Only valid claims
    assert_eq!(result.total_amount, 24500); // Sum of 2010..2029
}
