use crate::claims::{self, ClaimType};
use crate::slash_history;
use crate::test_helpers::{advance_ledger_sequence, setup_with_token};
use crate::tiered_bond;
use crate::{BondTier, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn tier_rank(t: &BondTier) -> u8 {
    match t {
        BondTier::Bronze => 0,
        BondTier::Silver => 1,
        BondTier::Gold => 2,
        BondTier::Platinum => 3,
    }
}

/// Create a bond and configure the slash treasury, returning (bond_amount).
fn setup_bond_and_treasury(
    e: &Env,
    client: &CredenceBondClient,
    admin: &Address,
    identity: &Address,
) -> i128 {
    let treasury = Address::generate(e);
    client.set_slash_treasury(admin, &treasury);
    let bond_amount = 1_000_000_i128;
    client.create_bond(identity, &bond_amount, &86400_u64, &false, &0_u64);
    advance_ledger_sequence(e);
    bond_amount
}

// ---------------------------------------------------------------------------
// 1. Nonce monotonicity
// ---------------------------------------------------------------------------

#[test]
fn nonce_starts_at_zero() {
    let e = Env::default();
    let (client, _admin, identity, _token, _contract_id) = setup_with_token(&e);
    let nonce = client.get_nonce(&identity);
    assert_eq!(nonce, 0);
}

#[test]
fn nonce_increases_monotonically_after_consume() {
    let e = Env::default();
    let (client, _admin, identity, _token, contract_id) = setup_with_token(&e);

    let initial = client.get_nonce(&identity);
    assert_eq!(initial, 0);

    e.as_contract(&contract_id, || {
        crate::nonce::consume_nonce(&e, &identity, 0);
    });

    let after_one = client.get_nonce(&identity);
    assert_eq!(after_one, 1);

    e.as_contract(&contract_id, || {
        crate::nonce::consume_nonce(&e, &identity, 1);
    });

    let after_two = client.get_nonce(&identity);
    assert!(after_two > after_one, "nonce must increase");
    assert_eq!(after_two, 2);
}

#[test]
fn nonce_never_decreases_after_any_number_of_consumes() {
    let e = Env::default();
    let (client, _admin, identity, _token, contract_id) = setup_with_token(&e);

    let mut prev = 0u64;
    for i in 0..10u64 {
        let expected = i;
        e.as_contract(&contract_id, || {
            crate::nonce::consume_nonce(&e, &identity, expected);
        });
        let current = client.get_nonce(&identity);
        assert!(
            current > prev,
            "nonce must strictly increase: prev={prev}, current={current}"
        );
        prev = current;
    }
}

// ---------------------------------------------------------------------------
// 2. Claim counter monotonicity
// ---------------------------------------------------------------------------

#[test]
fn claim_counter_starts_at_zero() {
    let e = Env::default();
    let (_client, _admin, _identity, _token, contract_id) = setup_with_token(&e);

    let counter: u64 = e.as_contract(&contract_id, || {
        e.storage()
            .persistent()
            .get(&crate::DataKey::ClaimCounter)
            .unwrap_or(0u64)
    });
    assert_eq!(counter, 0);
}

#[test]
fn claim_ids_are_strictly_increasing() {
    let e = Env::default();
    let (_client, _admin, identity, _token, contract_id) = setup_with_token(&e);

    let mut prev_id = 0u64;
    for i in 0..5u32 {
        let claim_id = e.as_contract(&contract_id, || {
            claims::add_pending_claim(
                &e,
                &identity,
                ClaimType::VerifierReward,
                (i as i128) + 1,
                i as u64,
                None,
            )
        });
        assert!(
            claim_id > prev_id,
            "claim IDs must be strictly increasing: prev={prev_id}, current={claim_id}"
        );
        prev_id = claim_id;
    }
}

// ---------------------------------------------------------------------------
// 3. Slash history ordering
// ---------------------------------------------------------------------------

#[test]
fn slash_history_records_are_appended_in_order() {
    let e = Env::default();
    let (client, admin, identity, _token, _contract_id) = setup_with_token(&e);

    setup_bond_and_treasury(&e, &client, &admin, &identity);

    for amount in [10_i128, 20, 5] {
        advance_ledger_sequence(&e);
        client.slash(&admin, &amount);
    }

    let count = e.as_contract(&_contract_id, || {
        slash_history::get_slash_count(&e, &identity)
    });
    assert_eq!(count, 3, "should have 3 slash records");

    let history = e.as_contract(&_contract_id, || {
        slash_history::get_slash_history_page(&e, &identity, 0, 200)
    });
    assert_eq!(history.len(), 3);
    assert_eq!(history.get_unchecked(0).slash_amount, 10);
    assert_eq!(history.get_unchecked(1).slash_amount, 20);
    assert_eq!(history.get_unchecked(2).slash_amount, 5);
}

#[test]
fn slash_timestamps_are_non_decreasing() {
    let e = Env::default();
    let (client, admin, identity, _token, _contract_id) = setup_with_token(&e);

    setup_bond_and_treasury(&e, &client, &admin, &identity);

    for _ in 0..4 {
        advance_ledger_sequence(&e);
        client.slash(&admin, &1);
    }

    let history = e.as_contract(&_contract_id, || {
        slash_history::get_slash_history_page(&e, &identity, 0, 200)
    });
    let count = history.len();
    let mut prev_ts = 0u64;
    for i in 0..count {
        let ts = history.get_unchecked(i).timestamp;
        assert!(
            ts >= prev_ts,
            "timestamps must be non-decreasing: prev={prev_ts}, current={ts}"
        );
        prev_ts = ts;
    }
}

// ---------------------------------------------------------------------------
// 4. Tier thresholds ordering
// ---------------------------------------------------------------------------

#[test]
fn tier_thresholds_are_ascending() {
    let e = Env::default();
    let (client, _admin, _identity, _token, contract_id) = setup_with_token(&e);

    e.as_contract(&contract_id, || {
        e.storage().instance().set(
            &crate::DataKey::TierThresholds,
            &crate::TierThresholds {
                bronze_max: 1_000,
                silver_max: 5_000,
                gold_max: 20_000,
            },
        );
    });

    let amounts: Vec<i128> = Vec::from_array(&e, [100, 999, 1_000, 4_999, 5_000, 19_999, 20_000]);
    let mut prev_rank = 0u8;
    for amt in amounts.iter() {
        let tier = e.as_contract(&contract_id, || tiered_bond::get_tier_for_amount(&e, amt));
        let rank = tier_rank(&tier);
        assert!(
            rank >= prev_rank,
            "tier ranks must be non-decreasing for ascending amounts: rank={rank} for amt={amt}"
        );
        prev_rank = rank;
    }
}

#[test]
fn tier_rank_assigns_correct_values() {
    assert_eq!(tier_rank(&BondTier::Bronze), 0);
    assert_eq!(tier_rank(&BondTier::Silver), 1);
    assert_eq!(tier_rank(&BondTier::Gold), 2);
    assert_eq!(tier_rank(&BondTier::Platinum), 3);
}

// ---------------------------------------------------------------------------
// 5. Slashed amount monotonicity
// ---------------------------------------------------------------------------

#[test]
fn slashed_amount_never_exceeds_bonded_amount() {
    let e = Env::default();
    let (client, admin, identity, _token, _contract_id) = setup_with_token(&e);

    let bonded = setup_bond_and_treasury(&e, &client, &admin, &identity);

    for _ in 0..3 {
        advance_ledger_sequence(&e);
        client.slash(&admin, &(bonded / 4));
        let state = client.get_identity_state();
        assert!(
            state.slashed_amount <= state.bonded_amount,
            "slashed {} must not exceed bonded {}",
            state.slashed_amount,
            state.bonded_amount
        );
    }
}

#[test]
fn slashed_amount_is_monotonic() {
    let e = Env::default();
    let (client, admin, identity, _token, _contract_id) = setup_with_token(&e);

    setup_bond_and_treasury(&e, &client, &admin, &identity);

    let mut prev_slashed = 0_i128;
    for amount in [10, 5, 3] {
        advance_ledger_sequence(&e);
        client.slash(&admin, &amount);
        let state = client.get_identity_state();
        assert!(
            state.slashed_amount >= prev_slashed,
            "slashed amount must not decrease: prev={prev_slashed}, current={}",
            state.slashed_amount
        );
        prev_slashed = state.slashed_amount;
    }
}

// ---------------------------------------------------------------------------
// 6. Bonded amount monotonicity
// ---------------------------------------------------------------------------

#[test]
fn bonded_amount_never_decreases_from_top_up() {
    let e = Env::default();
    let (client, _admin, identity, _token, _contract_id) = setup_with_token(&e);

    let initial_amount = 1_000_000_i128;
    client.create_bond(&identity, &initial_amount, &86400_u64, &false, &0_u64);

    let mut prev_bonded = client.get_identity_state().bonded_amount;

    for extra in [100, 200, 50] {
        client.top_up(&identity, &extra);
        let current = client.get_identity_state().bonded_amount;
        assert!(
            current >= prev_bonded,
            "bonded amount must not decrease from top_up: prev={prev_bonded}, current={current}"
        );
        prev_bonded = current;
    }
}

// ---------------------------------------------------------------------------
// 7. No-wraparound: nonce overflow
// ---------------------------------------------------------------------------

#[test]
fn nonce_does_not_wrap_around_from_zero_after_consume() {
    let e = Env::default();
    let (_client, _admin, identity, _token, contract_id) = setup_with_token(&e);

    e.as_contract(&contract_id, || {
        crate::nonce::consume_nonce(&e, &identity, 0);
    });

    let after = e.as_contract(&contract_id, || crate::nonce::get_nonce(&e, &identity));
    assert_eq!(after, 1, "nonce must not wrap to 0");
}
