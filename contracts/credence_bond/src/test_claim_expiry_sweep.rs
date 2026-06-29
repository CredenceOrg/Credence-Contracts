#![cfg(test)]

use std::format;
extern crate std;

use crate::{
    claims::{self, ClaimType, PendingClaim},
    CredenceBond, CredenceBondClient,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};
use core::fmt::Write;

fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin, contract_id)
}

fn as_bond<R>(e: &Env, contract_id: &Address, f: impl FnOnce() -> R) -> R {
    e.as_contract(contract_id, f)
}

#[test]
fn test_expire_claims_empty_user() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);

    // Uses contract entrypoint, so it already runs in-contract.
    let pruned = client.expire_claims(&user, &50);
    assert_eq!(pruned, 0);
}

#[test]
fn test_expire_claims_no_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);

    // Uses contract entrypoint, so it already runs in-contract.
    // With an empty queue, there is nothing to prune.
    let pruned = client.expire_claims(&user, &50);
    assert_eq!(pruned, 0);
}

#[test]
fn test_expire_claims_all_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, _contract_id) = setup_with_contract(&env);
    let user = Address::generate(&env);
    let now = env.ledger().timestamp();

    // Add 5 claims that will expire
    for i in 0u32..5u32 {
        let mut label = alloc::string::String::from("claim_");
        write!(&mut label, "{}", i).unwrap();
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i as u64,
            Some(Symbol::new(&env, &label)),
            1000,
            1,
            Some(Symbol::new(&env, "normal_expiry")),
        );
    }

    as_bond(&env, &contract_id, || {
        // Add a claim that expires far in the future
        let _claim_id = claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000,
            1,
            Some(Symbol::new(&env, "test1")),
        );

        // Sweep should find nothing expired
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 0);

        // Verify claim still exists
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

    // Add 100 claims
    for i in 0u32..100u32 {
        let mut label = alloc::string::String::from("claim_");
        write!(&mut label, "{}", i).unwrap();
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i as u64,
            Some(Symbol::new(&env, &label)),
        );
    }

    // Advance time past expiry
    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    let now = env.ledger().timestamp();

    as_bond(&env, &contract_id, || {
        // Add 5 claims that will expire
        for i in 0..5 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Advance time past expiry (default is 30 days)
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        // Sweep with max_iter=50 should remove all 5
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 5);

        // Verify all claims removed
        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 0);

        // Verify claimable amount reset to 0
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

    as_bond(&env, &contract_id, || {
        // Add 100 claims
        for i in 0..100 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Advance time past expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        // First sweep with max_iter=20 should only process first 20
        let pruned1 = claims::expire_claims_bounded(&env, &user, 20);
        assert_eq!(pruned1, 20);

        // Remaining should still be 80
        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 80);

        // Second sweep with max_iter=30 should process 30 more
        let pruned2 = claims::expire_claims_bounded(&env, &user, 30);
        assert_eq!(pruned2, 30);

        // Remaining should be 50
        let claims_vec = claims::get_pending_claims(&env, &user);
        assert_eq!(claims_vec.len(), 50);

        // Third sweep with max_iter=100 should process remaining 50
        let pruned3 = claims::expire_claims_bounded(&env, &user, 100);
        assert_eq!(pruned3, 50);

        // All gone
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

    // Add 10 claims
    for i in 0u32..10u32 {
        let mut label = alloc::string::String::from("claim_");
        write!(&mut label, "{}", i).unwrap();

        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i as u64,
            Some(Symbol::new(&env, &label)),
            1000,
            1,
            Some(Symbol::new(&env, "normal_expiry")),
        );

        // Manually add a claim with no expiry (expires_at = 0)
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

        // Advance time past expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        // Sweep should only remove the one with expiry, not the one with expires_at=0
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

    as_bond(&env, &contract_id, || {
        // Add 10 claims
        for i in 0..10 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Advance time to middle of expiry window (20 days)
        env.ledger().set_timestamp(now + 20 * 24 * 60 * 60);

        // None should be expired yet
        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 0);

        // Advance past the 30-day expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        // Now all should be expired
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

    // Add 5 claims; we'll expire some and keep others
    for i in 0u32..5u32 {
        let mut label = alloc::string::String::from("claim_");
        write!(&mut label, "{}", i).unwrap();
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i as u64,
            Some(Symbol::new(&env, &label)),
        );
    }

    // Manually set some claims to shorter expiry to ensure they expire
    let mut claims_vec = claims::get_pending_claims(&env, &user);
    // Make claims 0, 2, 4 expire; keep 1, 3
    for i in [0, 2, 4] {
        let mut claim = claims_vec.get(i).unwrap();
        claim.expires_at = now + 1000; // Short expiry
        claims_vec.set(i, claim);
    }
    for i in [1, 3] {
        let mut claim = claims_vec.get(i).unwrap();
        claim.expires_at = now + 100 * 24 * 60 * 60; // Long expiry
        claims_vec.set(i, claim);
    }
    env.storage()
        .persistent()
        .set(&crate::DataKey::PendingClaims(user.clone()), &claims_vec);

    // Advance past short expiry
    env.ledger().set_timestamp(now + 2000);

    // Sweep should remove 3 claims (0, 2, 4)
    let pruned = claims::expire_claims_bounded(&env, &user, 50);
    assert_eq!(pruned, 3);

    // Verify remaining claims are in correct order and have correct amounts
    let remaining = claims::get_pending_claims(&env, &user);
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining.get(0).unwrap().amount, 1001); // claim 1
    assert_eq!(remaining.get(1).unwrap().amount, 1003); // claim 3
    as_bond(&env, &contract_id, || {
        // Add 5 claims
        for i in 0..5 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Mutate expiry to control which claims expire
        let mut claims_vec = claims::get_pending_claims(&env, &user);

        // Make claims 0, 2, 4 expire; keep 1, 3
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

        // Advance past short expiry
        env.ledger().set_timestamp(now + 2000);

        // Sweep should remove 3 claims
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

    as_bond(&env, &contract_id, || {
        // Add 10 claims
        for i in 0..10 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Advance past expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

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

    // Add 100 claims
    for i in 0u32..100u32 {
        let mut label = alloc::string::String::from("claim_");
        write!(&mut label, "{}", i).unwrap();
        claims::add_pending_claim(
            &env,
            &user,
            ClaimType::VerifierReward,
            1000 + (i as i128),
            i as u64,
            Some(Symbol::new(&env, &label)),
        );
    }

    // Advance past expiry
    env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

    // Sweep with max_iter=0 should use default (50)
    let pruned = claims::expire_claims_bounded(&env, &user, 0);
    assert_eq!(pruned, 50);
    let now = env.ledger().timestamp();

    as_bond(&env, &contract_id, || {
        // Add 100 claims
        for i in 0..100 {
            claims::add_pending_claim(
                &env,
                &user,
                ClaimType::VerifierReward,
                1000 + (i as i128),
                i as u64,
                Some(Symbol::new(&env, &format!("claim_{}", i))),
            );
        }

        // Advance past expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        // max_iter=0 should use DEFAULT (50)
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

    as_bond(&env, &contract_id, || {
        // Add 3 claims with known amounts
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 1000, 1, None);
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 2000, 2, None);
        claims::add_pending_claim(&env, &user, ClaimType::VerifierReward, 3000, 3, None);

        let initial_claimable = claims::get_claimable_amount(&env, &user);
        assert_eq!(initial_claimable, 6000);

        // Advance past expiry
        env.ledger().set_timestamp(now + 31 * 24 * 60 * 60);

        let pruned = claims::expire_claims_bounded(&env, &user, 50);
        assert_eq!(pruned, 3);

        let final_claimable = claims::get_claimable_amount(&env, &user);
        assert_eq!(final_claimable, 0);
    });
}
