extern crate std;

use crate::{
    claims::{self, ClaimType, PendingClaim},
    CredenceBond, CredenceBondClient,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin, contract_id)
}

fn in_contract<T>(env: &Env, contract_id: &Address, f: impl FnOnce() -> T) -> T {
    env.as_contract(contract_id, f)
}

#[test]
fn test_expire_claims_empty_user() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);

    // Should return 0 for user with no claims
    let pruned = client.expire_claims(&user, &50);
    assert_eq!(pruned, 0);
}

#[test]
fn test_expire_claims_no_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);

    in_contract(&env, &contract_id, || {
        let _claim_id = claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000,
            1,
            Some(Symbol::new(&env, "test1")),
        );

        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 0);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 1);
    });
}

#[test]
fn test_expire_claims_all_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..5 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 5);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 0);

        let claimable = claims::get_claimable_amount(&env, &user);
        assert_eq!(claimable, 0);
    });
}

#[test]
fn test_expire_claims_bounded_by_max_iter() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..100 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned1 = claims::expire_claims_bounded(&env, &user, 20);
        assert_eq!(pruned1, 20);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 80);

        let pruned2 = claims::expire_claims_bounded(&env, &user, 30);
        assert_eq!(pruned2, 30);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 50);

        let pruned3 = claims::expire_claims_bounded(&env, &user, 100);
        assert_eq!(pruned3, 50);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 0);
    });
}

#[test]
fn test_expire_claims_skips_no_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000,
            1,
            Some(Symbol::new(&env, "normal_expiry")),
        );

        let no_expiry_claim = PendingClaim {
            claim_id: 999,
            claim_type: ClaimType::VerifierReward,
            amount: 500,
            created_at: now,
            expires_at: 0,
            source_id: 2,
            metadata: Symbol::new(&env, "no_expiry"),
            processed: false,
        };
        let mut claims_vec = claims::get_pending_claims(&env, &user);
        claims_vec.push_back(no_expiry_claim);
        env.storage()
            .persistent()
            .set(&crate::DataKey::PendingClaims(user.clone()), &claims_vec);
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 1);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 1);
        assert_eq!(claims_vec.get(0).unwrap().expires_at, 0);
    });
}

#[test]
fn test_expire_claims_mixed_expired_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..10 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }
    });

    env.ledger().set_timestamp(now + 20 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 0);
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 10);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 0);
    });
}

#[test]
fn test_expire_claims_preserves_valid_claims_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..5 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }

        let mut claims_vec = claims::get_pending_claims(&env, &user);
        for i in [0, 2, 4] {
            let mut claim = claims_vec.get(i).unwrap();
            claim.expires_at = now + 1000;
            claims_vec.set(i, claim);
        }
        for i in [1, 3] {
            let mut claim = claims_vec.get(i).unwrap();
            claim.expires_at = now + 100 * 24 * 60 * 60;
            claims_vec.set(i, claim);
        }
        env.storage()
            .persistent()
            .set(&crate::DataKey::PendingClaims(user.clone()), &claims_vec);
    });

    env.ledger().set_timestamp(now + 2000);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 3);

        let remaining = claims::get_pending_claims(&env, &user);
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining.get(0).unwrap().amount, 1001);
        assert_eq!(remaining.get(1).unwrap().amount, 1003);
    });
}

#[test]
fn test_expire_claims_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..10 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned1 = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned1, 10);

        let pruned2 = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned2, 0);

        let pruned3 = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned3, 0);
    });
}

#[test]
fn test_expire_claims_max_iter_zero_uses_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        for i in 0..100 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &std::format!("claim_{}", i))),
            );
        }
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 0);
        assert_eq!(pruned, 50);

        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 50);
    });
}

#[test]
fn test_expire_claims_claimable_amount_updated() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    in_contract(&env, &contract_id, || {
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 1000, 1, None);
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 2000, 2, None);
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 3000, 3, None);

        let initial_claimable = claims::get_claimable_amount(&env, &user);
        assert_eq!(initial_claimable, 6000);
    });

    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    in_contract(&env, &contract_id, || {
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 3);

        let final_claimable = claims::get_claimable_amount(&env, &user);
        assert_eq!(final_claimable, 0);
    });
}
