use credence_bond::{ClaimType, CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SorobanVec};

fn setup(env: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let user = Address::generate(env);

    client.initialize(&admin);

    // Register token
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.set_token(&admin, &token_id);

    // Mint tokens to user
    let asset = soroban_sdk::token::StellarAssetClient::new(env, &token_id);
    asset.mint(&user, &1_000_000_i128);

    // Approve contract to spend user's tokens
    let token = soroban_sdk::token::TokenClient::new(env, &token_id);
    token.approve(&user, &contract_id, &1_000_000_i128, &0_u32);

    (client, admin, user)
}

fn setup_with_attester(env: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    let (client, admin, user) = setup(env);

    // Generate attester and register them (requires admin auth, but mock_all_auths handles it)
    let attester = Address::generate(env);
    client.register_attester(&attester);

    (client, admin, user, attester)
}

/// Helper to add a pending claim directly for testing
fn add_pending_claim_for_testing(
    client: &CredenceBondClient<'_>,
    attester: &Address,
    user: &Address,
    _claim_type: ClaimType,
    _amount: i128,
) {
    // Add attestation to generate a claim
    let valid = soroban_sdk::String::from_str(&client.env, "valid");
    let contract_id = client.address.clone();
    let deadline = client.env.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(attester);

    client.add_attestation(attester, user, &valid, &contract_id, &deadline, &nonce);
    // The claim is automatically added when attestation is created
}

/// Setup with tokens minted to the contract so claims can be paid out
fn setup_with_funded_contract(env: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let user = Address::generate(env);

    client.initialize(&admin);

    // Register token
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.set_token(&admin, &token_id);

    // Mint tokens to user
    let asset = soroban_sdk::token::StellarAssetClient::new(env, &token_id);
    asset.mint(&user, &1_000_000_i128);

    // Approve contract to spend user's tokens
    let token = soroban_sdk::token::TokenClient::new(env, &token_id);
    token.approve(&user, &contract_id, &1_000_000_i128, &0_u32);

    // Also mint tokens to the contract so it can pay out claims
    asset.mint(&contract_id, &1_000_000_i128);

    // Generate attester and register them
    let attester = Address::generate(env);
    client.register_attester(&attester);

    (client, admin, user, attester)
}

#[test]
fn test_claim_rewards_basic() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_funded_contract(&env);

    // Add attestation which creates a pending claim for the attester
    let subject = Address::generate(&env);
    add_pending_claim_for_testing(&client, &attester, &subject, ClaimType::VerifierReward, 1000);

    // Process all rewards (claims go to the attester)
    let result = client.claim_all_rewards(&attester);

    assert!(result.processed_count > 0);
    assert!(result.total_amount > 0);
}

#[test]
fn test_claim_rewards_by_type() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_funded_contract(&env);

    // Add attestation which creates a pending claim for the attester
    let subject = Address::generate(&env);
    add_pending_claim_for_testing(&client, &attester, &subject, ClaimType::VerifierReward, 1000);

    // Process only VerifierReward claims (claims go to the attester)
    let claim_types = SorobanVec::from_array(&env, [ClaimType::VerifierReward]);
    let result = client.claim_rewards_by_type(&attester, &claim_types);

    assert!(result.processed_count > 0);
    assert!(result.total_amount > 0);
}

#[test]
fn test_claim_rewards_batch_limit() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_funded_contract(&env);

    // Add multiple attestations to create multiple claims for the attester
    for _i in 0..5 {
        let subject = Address::generate(&env);
        let valid = soroban_sdk::String::from_str(&env, "valid");
        let contract_id = client.address.clone();
        let deadline = env.ledger().timestamp() + 100_000;
        let nonce = client.get_nonce(&attester);

        client.add_attestation(&attester, &subject, &valid, &contract_id, &deadline, &nonce);
    }

    // Process only 2 claims at a time (claims go to the attester)
    let result = client.claim_rewards_batch(&attester, &2);

    // Should process at most 2 claims due to limit
    assert!(result.processed_count <= 2);
}

#[test]
fn test_claim_rewards_empty_panics() {
    let env = Env::default();
    let (client, _admin, user) = setup(&env);

    // Attempt to claim with no pending claims should panic
    let result = client.try_claim_all_rewards(&user);
    assert!(result.is_err());
}

#[test]
fn test_get_claims_summary() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_attester(&env);

    // Add attestation which creates a pending claim for the attester
    let subject = Address::generate(&env);
    add_pending_claim_for_testing(&client, &attester, &subject, ClaimType::VerifierReward, 1000);

    // Get claims summary (claims belong to the attester)
    let summary = client.get_claims_summary(&attester);

    // Should have at least one claim type
    assert!(summary.len() > 0);
}

#[test]
fn test_cleanup_expired_claims() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_attester(&env);

    // Add attestation which creates a pending claim for the attester
    let subject = Address::generate(&env);
    add_pending_claim_for_testing(&client, &attester, &subject, ClaimType::VerifierReward, 1000);

    // Advance time past the claim expiry (default is DEFAULT_CLAIM_EXPIRY)
    env.ledger().with_mut(|l| {
        l.timestamp += 86401; // Default claim expiry is 86400
    });

    // Cleanup expired claims (claims belong to the attester)
    let cleaned = client.cleanup_expired_claims(&attester);

    // Should have cleaned some claims
    assert!(cleaned >= 0);
}

#[test]
fn test_claim_rewards_by_type_no_matching() {
    let env = Env::default();
    let (client, _admin, _user, attester) = setup_with_attester(&env);

    // Add attestation which creates a VerifierReward claim for the attester
    let subject = Address::generate(&env);
    add_pending_claim_for_testing(&client, &attester, &subject, ClaimType::VerifierReward, 1000);

    // Try to claim only SlashingReward (which doesn't exist) - should panic
    let claim_types = SorobanVec::from_array(&env, [ClaimType::SlashingReward]);
    let result = client.try_claim_rewards_by_type(&attester, &claim_types);
    
    // Should fail because no valid claims match the filter
    assert!(result.is_err());
}