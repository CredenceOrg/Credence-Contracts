use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, Vec};

use crate::{claims, CredenceBond, DataKey};

fn register_contract(env: &Env) -> Address {
    env.register(CredenceBond, ())
}

fn setup_token(env: &Env, contract_id: &Address) -> Address {
    let admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let asset = soroban_sdk::token::StellarAssetClient::new(env, &token_id);
    asset.mint(contract_id, &1_000_000_i128);

    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::BondToken, &token_id);
    });

    token_id
}

#[test]
fn test_add_and_get_pending_claims() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            100,
            1,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::FeeRebate,
            200,
            2,
            None,
        );

        let claims_list = claims::get_pending_claims(&e, &user);
        assert_eq!(claims_list.len(), 2);
        assert_eq!(claims::get_claimable_amount(&e, &user), 300);
    });
}

#[test]
fn test_get_pending_claims_paginated() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        for i in 0..3u64 {
            claims::add_pending_claim(
                &e,
                &user,
                claims::ClaimType::VerifierReward,
                10,
                i,
                None,
            );
        }

        let first_page = claims::get_pending_claims_paginated(&e, &user, 0, 2);
        assert_eq!(first_page.len(), 2);

        let second_page = claims::get_pending_claims_paginated(&e, &user, 3, 2);
        assert_eq!(second_page.len(), 1);
    });
}

#[test]
fn test_process_claims_filter_and_limit() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            100,
            1,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::FeeRebate,
            200,
            2,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            300,
            3,
            None,
        );

        let mut claim_types = Vec::new(&e);
        claim_types.push_back(claims::ClaimType::VerifierReward);

        let result = claims::process_claims(&e, &user, claim_types, 1);
        assert_eq!(result.processed_count, 1);
        assert_eq!(result.total_amount, 100);

        let remaining = claims::get_pending_claims(&e, &user);
        assert_eq!(remaining.len(), 2);
    });
}

#[test]
fn test_process_claims_skips_expired() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        e.ledger().with_mut(|l| l.timestamp = 100);
        let now = e.ledger().timestamp();
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            100,
            1,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::FeeRebate,
            200,
            2,
            None,
        );

        let mut claims_list = claims::get_pending_claims(&e, &user);
        let mut claim = claims_list.get(0).unwrap();
        claim.expires_at = now.saturating_sub(1);
        claims_list.set(0, claim);

        e.storage()
            .persistent()
            .set(&DataKey::PendingClaims(user.clone()), &claims_list);

        let result = claims::process_claims(&e, &user, Vec::new(&e), 0);
        assert_eq!(result.processed_count, 1);
        assert_eq!(result.total_amount, 200);
    });
}

#[test]
fn test_process_claims_paginated_offset_and_filter() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            100,
            1,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::FeeRebate,
            200,
            2,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            300,
            3,
            None,
        );

        let mut claim_types = Vec::new(&e);
        claim_types.push_back(claims::ClaimType::VerifierReward);

        let result = claims::process_claims_paginated(&e, &user, 1, 1, claim_types);
        assert_eq!(result.processed_count, 1);
        assert_eq!(result.total_amount, 300);
    });
}

#[test]
fn test_cleanup_expired_claims_updates_amount() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let _token_id = setup_token(&e, &contract_id);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        e.ledger().with_mut(|l| l.timestamp = 100);
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::VerifierReward,
            100,
            1,
            None,
        );
        claims::add_pending_claim(
            &e,
            &user,
            claims::ClaimType::FeeRebate,
            200,
            2,
            None,
        );

        let mut claims_list = claims::get_pending_claims(&e, &user);
        let mut expired = claims_list.get(0).unwrap();
        expired.expires_at = 90;
        claims_list.set(0, expired);

        e.storage()
            .persistent()
            .set(&DataKey::PendingClaims(user.clone()), &claims_list);
        e.storage()
            .persistent()
            .set(&DataKey::ClaimableAmount(user.clone()), &300i128);

        let cleaned = claims::cleanup_expired_claims(&e, &user);
        assert_eq!(cleaned, 1);
        assert_eq!(claims::get_claimable_amount(&e, &user), 200);
    });
}
