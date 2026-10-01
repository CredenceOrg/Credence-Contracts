//! Boundary and recovery tests for the slashing subsystem.
//!
//! Companion to `test_slashing.rs`, which covers the happy-path slashing
//! surface. This file concentrates on the edges that a slash touches:
//!
//! 1. `unslash_bond` boundaries — the exact top of the range, one past it,
//!    and the slash/unslash round trip.
//! 2. Failed-slash atomicity — a rejected slash must leave *no* trace, and
//!    the caller must be able to retry successfully.
//! 3. Same-ledger collateral-increase guard and its recovery on the next
//!    ledger.
//! 4. The 10% slasher-reward threshold. The reward is `amount / 10` with
//!    integer division, so it is zero for any slash below 10 and one unit at
//!    exactly 10. The boundary sits between 9 and 10.
//! 5. Exact tier transitions. Tiers are derived from the *available* balance
//!    (`bonded - slashed`), and an amount landing exactly on a threshold
//!    advances to the next tier, so the interesting values are the threshold
//!    itself and threshold - 1.

use crate::same_ledger_liquidation_guard::SLASH_BLOCKED_REASON;
use crate::test_helpers;
use crate::{BondTier, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

/// Registers the contract, configures a slash treasury, and returns the client
/// alongside the admin, identity and contract id.
fn deploy(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    let (client, admin, identity, _asset, contract_id) = test_helpers::setup_with_token(e);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    (client, admin, identity, contract_id)
}

/// Deploy, then bond `amount` and step past the ledger that created it.
///
/// `create_bond` records a collateral increase for the current ledger, which
/// blocks a same-ledger slash. Every helper that goes on to slash therefore
/// has to advance the sequence first.
fn deploy_with_bond(e: &Env, amount: i128) -> (CredenceBondClient<'_>, Address, Address, Address) {
    let (client, admin, identity, contract_id) = deploy(e);
    client.create_bond(
        &identity,
        &amount,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(e);
    (client, admin, identity, contract_id)
}

/// Overwrites the tier thresholds so tests can cross them with bond amounts
/// the leverage cap actually permits.
///
/// The shipped defaults (1e21 / 5e21 / 2e22) sit far above the largest bond
/// the test configuration allows, so every test bond would otherwise be
/// Bronze forever and no transition would be reachable.
fn set_tier_thresholds(e: &Env, contract_id: &Address, bronze: i128, silver: i128, gold: i128) {
    e.as_contract(contract_id, || {
        e.storage().instance().set(
            &crate::DataKey::TierThresholds,
            &crate::TierThresholds {
                bronze_max: bronze,
                silver_max: silver,
                gold_max: gold,
            },
        );
    });
}

fn tier_of(client: &CredenceBondClient<'_>, identity: &Address) -> BondTier {
    client.describe_bond(identity).unwrap().tier
}

fn available_of(client: &CredenceBondClient<'_>, identity: &Address) -> i128 {
    client.describe_bond(identity).unwrap().available_amount
}

fn slashed_of(client: &CredenceBondClient<'_>, identity: &Address) -> i128 {
    client.get_identity_state(identity).slashed_amount
}

fn claim_count(client: &CredenceBondClient<'_>, user: &Address) -> u32 {
    client.get_pending_claims_page(user, &0, &50).0.len()
}

// ============================================================================
// 1. unslash_bond boundaries
// ============================================================================

/// Unslashing the entire slashed amount is the top of the valid range and must
/// restore the full available balance.
#[test]
fn unslash_full_amount_restores_entire_available_balance() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);
    assert_eq!(available_of(&client, &identity), 600);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 400_i128);
    });

    assert_eq!(slashed_of(&client, &identity), 0);
    assert_eq!(available_of(&client, &identity), 1000);
}

/// A partial unslash leaves exactly the remainder slashed.
#[test]
fn unslash_partial_leaves_exact_remainder() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 150_i128);
    });

    assert_eq!(slashed_of(&client, &identity), 250);
    assert_eq!(available_of(&client, &identity), 750);
}

/// `amount == slashed_amount` is the last accepted value; one unit beyond it
/// must revert rather than saturate.
#[test]
#[should_panic(expected = "unslashing would reduce below 0")]
fn unslash_beyond_remaining_reverts() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 401_i128);
    });
}

/// Unslashing far more than was slashed must revert too, not wrap.
///
/// Regression test: the guard used to be
/// `slashed_amount.checked_sub(amount).expect("unslashing would reduce below 0")`.
/// `checked_sub` only reports i128 *overflow*, and a negative result such as
/// `400 - 500 = -100` is perfectly representable, so the `expect` never fired.
/// `slashed_amount` went negative, which inflated the available balance to
/// `bonded - slashed = 1100` for a 1000 bond and let `withdraw(1100)` pay out
/// more than the contract held.
#[test]
#[should_panic(expected = "unslashing would reduce below 0")]
fn unslash_far_beyond_remaining_reverts() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 500_i128);
    });
}

/// The available balance must never exceed the bonded amount, which is the
/// property the negative-`slashed_amount` bug violated.
#[test]
fn available_never_exceeds_bonded_across_unslash_sequences() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    // Step the remaining slashed amount down to zero, unslashing the delta at
    // each stage and re-checking the invariant. The sequence ends on 0, the
    // exact top of the unslash range.
    let mut remaining = 400_i128;
    for target in [400_i128, 399, 300, 1, 0] {
        let delta = remaining - target;
        assert!(delta >= 0);
        if delta > 0 {
            e.as_contract(&contract_id, || {
                crate::slashing::unslash_bond(&e, &admin, &identity, delta);
            });
        }
        let view = client.describe_bond(&identity).unwrap();
        assert_eq!(view.slashed_amount, target);
        assert_eq!(view.available_amount, 1000 - target);
        assert!(
            view.available_amount <= view.bonded_amount,
            "available {} exceeded bonded {} at remaining {}",
            view.available_amount,
            view.bonded_amount,
            target
        );
        assert!(view.slashed_amount >= 0, "slashed_amount went negative");
        remaining = target;
    }

    // Re-slash to restore the original state.
    client.slash(&admin, &identity, &400_i128);
    assert_eq!(slashed_of(&client, &identity), 400);
}

/// The contract's token balance must always cover the collateral it reports.
/// A negative `slashed_amount` previously let a withdrawal drain 1100 from a
/// contract holding 600.
#[test]
fn contract_balance_covers_reported_collateral() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);

    let contract_balance = || {
        e.as_contract(&contract_id, || {
            let token: Address = e
                .storage()
                .instance()
                .get(&crate::DataKey::BondToken)
                .unwrap();
            TokenClient::new(&e, &token).balance(&contract_id)
        })
    };

    let total_collateral = || {
        client
            .describe_bond(&identity)
            .map(|v| v.bonded_amount)
            .unwrap_or(0)
    };

    client.slash(&admin, &identity, &400_i128);
    // Unslash the exact slashed amount, then immediately re-slash, so the
    // sequence ends in the same state it started from.
    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 400_i128);
    });
    client.slash(&admin, &identity, &400_i128);

    assert!(
        contract_balance() >= total_collateral(),
        "contract balance {} no longer covers collateral {}",
        contract_balance(),
        total_collateral()
    );
    assert_eq!(available_of(&client, &identity), 600);
}

/// A negative unslash is rejected up front, before any state is read.
#[test]
#[should_panic(expected = "unslash amount must be non-negative")]
fn unslash_negative_reverts() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, -1_i128);
    });
}

/// Zero is a legal no-op: the contract accepts it rather than treating it as
/// an error, and leaves the slash untouched.
#[test]
fn unslash_zero_is_accepted_noop() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 0_i128);
    });

    assert_eq!(slashed_of(&client, &identity), 400);
    assert_eq!(available_of(&client, &identity), 600);
}

/// Unslashing is an admin capability just like slashing.
#[test]
#[should_panic(expected = "not admin")]
fn unslash_requires_admin() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    let impostor = Address::generate(&e);
    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &impostor, &identity, 100_i128);
    });
}

/// Full round trip: slash, unslash back to zero, then slash the same amount
/// again. The second slash must behave exactly like the first, which is the
/// property that makes unslash a genuine reversal rather than a one-way
/// adjustment.
#[test]
fn slash_unslash_reslash_round_trip_is_symmetric() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);

    let first = client.slash(&admin, &identity, &300_i128);
    assert_eq!(first.slashed_amount, 300);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 300_i128);
    });
    assert_eq!(slashed_of(&client, &identity), 0);
    assert_eq!(available_of(&client, &identity), 1000);

    // Same amount is slashable again, and nothing about the first slash
    // lingers to block it.
    let second = client.slash(&admin, &identity, &300_i128);
    assert_eq!(second.slashed_amount, 300);
    assert_eq!(second.bonded_amount, 1000);
    assert_eq!(available_of(&client, &identity), 700);
}

/// After an unslash restores available balance, a slash that was previously
/// rejected must now be accepted. This is the recovery half of the
/// over-slash boundary.
#[test]
fn previously_rejected_slash_succeeds_after_unslash() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);

    client.slash(&admin, &identity, &800_i128);
    // available = 200, so 500 is out of range.
    assert!(client.try_slash(&admin, &identity, &500_i128).is_err());

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 300_i128);
    });
    // available = 500 again.
    let bond = client.slash(&admin, &identity, &500_i128);
    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(available_of(&client, &identity), 0);
}

// ============================================================================
// 2. Failed-slash atomicity and retry
// ============================================================================

/// A slash rejected for exceeding the available balance must leave no trace:
/// not the slashed amount, not the history record, and not the reward claim.
#[test]
fn rejected_slash_leaves_no_state_behind() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &600_i128);

    let slashed_before = slashed_of(&client, &identity);
    let history_before = client.get_slash_count(&identity);
    let claims_before = claim_count(&client, &admin);

    // available = 400; 401 must be rejected.
    assert!(client.try_slash(&admin, &identity, &401_i128).is_err());

    assert_eq!(slashed_of(&client, &identity), slashed_before);
    assert_eq!(client.get_slash_count(&identity), history_before);
    assert_eq!(claim_count(&client, &admin), claims_before);
    assert_eq!(available_of(&client, &identity), 400);
}

/// A slash rejected for a non-admin caller must also be inert.
#[test]
fn unauthorized_slash_leaves_no_state_behind() {
    let e = Env::default();
    let (client, _admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    let impostor = Address::generate(&e);
    assert!(client.try_slash(&impostor, &identity, &500_i128).is_err());

    assert_eq!(slashed_of(&client, &identity), 0);
    assert_eq!(client.get_slash_count(&identity), 0);
    assert_eq!(claim_count(&client, &impostor), 0);
}

/// After a rejected attempt the caller can retry successfully, and the
/// successful retry records exactly one history entry — the failed attempt
/// must not be counted.
#[test]
fn retry_after_rejected_slash_succeeds_and_records_once() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &700_i128);

    assert!(client.try_slash(&admin, &identity, &400_i128).is_err());
    assert_eq!(client.get_slash_count(&identity), 1);

    let bond = client.slash(&admin, &identity, &300_i128);

    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(client.get_slash_count(&identity), 2);

    let history = client.get_slash_history_page(&identity, &0, &10);
    assert_eq!(history.get(1).unwrap().slash_amount, 300);
    assert_eq!(history.get(1).unwrap().total_slashed_after, 1000);
}

/// Retrying a rejected slash at the exact available amount is accepted, so
/// the boundary is inclusive on the high side.
#[test]
fn slash_at_exact_available_amount_is_accepted() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &400_i128);

    let bond = client.slash(&admin, &identity, &600_i128);

    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(available_of(&client, &identity), 0);
    // Nothing further is slashable once available hits zero.
    assert!(client.try_slash(&admin, &identity, &1_i128).is_err());
}

/// Once fully slashed the bond has no available balance, and a rejected slash
/// cannot move it back off zero.
#[test]
fn fully_slashed_bond_stays_fully_slashed_after_rejected_slash() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &1000_i128);
    assert_eq!(available_of(&client, &identity), 0);

    assert!(client.try_slash(&admin, &identity, &1_i128).is_err());

    assert_eq!(slashed_of(&client, &identity), 1000);
    assert_eq!(available_of(&client, &identity), 0);
}

// ============================================================================
// 3. Same-ledger collateral-increase guard
// ============================================================================

/// A slash in the same ledger as the collateral increase is rejected, which
/// is the anti-sandwich guarantee the guard exists for.
#[test]
fn slash_in_same_ledger_as_collateral_increase_is_blocked() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy(&e);
    // Deliberately no advance_ledger_sequence: same ledger as create_bond.
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    assert!(client.try_slash(&admin, &identity, &100_i128).is_err());
    assert_eq!(slashed_of(&client, &identity), 0);
}

/// Recovery: once the ledger advances, the same slash succeeds unchanged.
#[test]
fn slash_recovers_on_next_ledger() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    assert!(client.try_slash(&admin, &identity, &100_i128).is_err());

    test_helpers::advance_ledger_sequence(&e);
    let bond = client.slash(&admin, &identity, &100_i128);

    assert_eq!(bond.slashed_amount, 100);
    assert_eq!(slashed_of(&client, &identity), 100);
}

/// A single ledger advance is enough — the guard is one-ledger-only, not a
/// cooldown.
#[test]
fn guard_clears_after_a_single_ledger_advance() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    assert!(client.try_slash(&admin, &identity, &100_i128).is_err());
    test_helpers::advance_ledger_sequence(&e);
    assert!(client.try_slash(&admin, &identity, &100_i128).is_ok());
}

/// The guard is scoped to slashing only. `unslash_bond` is a corrective path
/// and stays available in the same ledger, so a mistaken slash can always be
/// walked back without waiting.
#[test]
fn unslash_is_not_blocked_by_the_guard() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    client.slash(&admin, &identity, &300_i128);
    test_helpers::advance_ledger_sequence(&e);
    // Record a fresh collateral increase in the current ledger.
    e.as_contract(&contract_id, || {
        crate::same_ledger_liquidation_guard::record_collateral_increase(&e);
    });

    // A slash is refused right now...
    assert!(client.try_slash(&admin, &identity, &100_i128).is_err());
    // ...but the unslash goes through.
    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 300_i128);
    });
    assert_eq!(slashed_of(&client, &identity), 0);
}

/// A contract that has never recorded a collateral increase is unaffected by
/// the guard, so pre-upgrade bonds are not bricked.
#[test]
fn guard_is_a_noop_when_no_collateral_increase_was_recorded() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // Simulate a pre-upgrade instance: forget the recorded sequence.
    e.as_contract(&contract_id, || {
        e.storage()
            .instance()
            .remove(&crate::DataKey::LastCollateralIncreaseLedger);
    });
    let recorded = e.as_contract(&contract_id, || {
        crate::same_ledger_liquidation_guard::last_collateral_increase_ledger(&e)
    });
    assert_eq!(recorded, None);

    // Same ledger, but the guard has nothing to compare against.
    let bond = client.slash(&admin, &identity, &100_i128);
    assert_eq!(bond.slashed_amount, 100);
}

// ============================================================================
// 4. Reward threshold: 9 vs 10
// ============================================================================

/// The slasher reward is `slash_amount / 10` with integer division, so a slash
/// of 9 yields a zero reward and the contract skips the claim entirely.
#[test]
fn slash_below_reward_threshold_creates_no_claim() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    let bond = client.slash(&admin, &identity, &9_i128);

    assert_eq!(bond.slashed_amount, 9);
    assert_eq!(claim_count(&client, &admin), 0);
}

/// One unit higher the reward becomes claimable. This is the exact edge of the
/// threshold.
#[test]
fn slash_at_reward_threshold_creates_claim() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    let bond = client.slash(&admin, &identity, &10_i128);

    assert_eq!(bond.slashed_amount, 10);
    let (claims, _next) = client.get_pending_claims_page(&admin, &0, &50);
    assert_eq!(claims.len(), 1);
    let claim = claims.get(0).unwrap();
    assert_eq!(claim.amount, 1);
    assert_eq!(claim.claim_type, crate::claims::ClaimType::SlashingReward);
    assert!(!claim.processed);
}

/// The reward truncates rather than rounding, so 19 still pays 1, not 2.
#[test]
fn reward_truncates_rather_than_rounding() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    client.slash(&admin, &identity, &19_i128);

    let (claims, _next) = client.get_pending_claims_page(&admin, &0, &50);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims.get(0).unwrap().amount, 1);
}

/// 20 crosses to a 2-unit reward, confirming the division is exactly tenths.
#[test]
fn reward_is_exactly_one_tenth_truncated() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    client.slash(&admin, &identity, &250_i128);

    let (claims, _next) = client.get_pending_claims_page(&admin, &0, &50);
    assert_eq!(claims.get(0).unwrap().amount, 25);
}

/// Several sub-threshold slashes add up to a claimable reward only once the
/// cumulative reward clears the threshold. Two slashes of 9 stay claimless.
#[test]
fn sub_threshold_slashes_accumulate_no_reward() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    client.slash(&admin, &identity, &9_i128);
    client.slash(&admin, &identity, &9_i128);
    client.slash(&admin, &identity, &9_i128);

    assert_eq!(slashed_of(&client, &identity), 27);
    // Each slash truncated to 0, so no claim was ever created.
    assert_eq!(claim_count(&client, &admin), 0);
}

/// A rejected slash must not mint a reward either.
#[test]
fn rejected_slash_creates_no_reward_claim() {
    let e = Env::default();
    let (client, admin, identity, _contract_id) = deploy_with_bond(&e, 1000_i128);

    assert!(client.try_slash(&admin, &identity, &1001_i128).is_err());

    assert_eq!(claim_count(&client, &admin), 0);
}

// ============================================================================
// 5. Exact tier transitions
// ============================================================================

/// Tier thresholds are strict `<` comparisons, so an available balance
/// landing exactly on `bronze_max` has already advanced to Silver. Slashing
/// to precisely the threshold must therefore report Silver, not Bronze.
#[test]
fn available_exactly_on_threshold_advances_tier() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);

    assert_eq!(tier_of(&client, &identity), BondTier::Platinum);

    // available 1000 -> 500, i.e. exactly bronze_max.
    client.slash(&admin, &identity, &500_i128);

    assert_eq!(available_of(&client, &identity), 500);
    assert_eq!(tier_of(&client, &identity), BondTier::Silver);
}

/// One unit short of the threshold must stay in the lower tier. Together with
/// the previous test this pins the comparison to a strict `<`.
#[test]
fn available_one_below_threshold_stays_in_tier() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);

    // available 1000 -> 499, one short of bronze_max.
    client.slash(&admin, &identity, &501_i128);

    assert_eq!(available_of(&client, &identity), 499);
    assert_eq!(tier_of(&client, &identity), BondTier::Bronze);
}

/// A single slash can cross more than one threshold at once; the resulting
/// tier is the one the final available balance maps to, not an intermediate.
#[test]
fn slash_crossing_multiple_thresholds_lands_on_final_tier() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);

    // Platinum -> Bronze in one step, skipping Silver and Gold.
    client.slash(&admin, &identity, &999_i128);

    assert_eq!(available_of(&client, &identity), 1);
    assert_eq!(tier_of(&client, &identity), BondTier::Bronze);
}

/// Walking down the tiers one boundary at a time reports each intermediate
/// tier exactly, in order.
#[test]
fn successive_slashes_walk_tiers_in_order() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);

    // 1000 -> 850: crosses gold_max (900), landing in Gold.
    client.slash(&admin, &identity, &150_i128);
    assert_eq!(available_of(&client, &identity), 850);
    assert_eq!(tier_of(&client, &identity), BondTier::Gold);

    // 850 -> 500: exactly bronze_max, so Silver.
    client.slash(&admin, &identity, &350_i128);
    assert_eq!(available_of(&client, &identity), 500);
    assert_eq!(tier_of(&client, &identity), BondTier::Silver);

    // 500 -> 0: Bronze.
    client.slash(&admin, &identity, &500_i128);
    assert_eq!(available_of(&client, &identity), 0);
    assert_eq!(tier_of(&client, &identity), BondTier::Bronze);
}

/// Unslashing restores available balance and must restore the tier with it.
#[test]
fn unslash_restores_previous_tier() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);
    assert_eq!(tier_of(&client, &identity), BondTier::Platinum);

    client.slash(&admin, &identity, &500_i128);
    assert_eq!(tier_of(&client, &identity), BondTier::Silver);

    e.as_contract(&contract_id, || {
        crate::slashing::unslash_bond(&e, &admin, &identity, 500_i128);
    });

    assert_eq!(available_of(&client, &identity), 1000);
    assert_eq!(tier_of(&client, &identity), BondTier::Platinum);
}

/// The tier is derived from the available balance, not the bonded amount:
/// after a 400 slash the bond is still bonded at 1000 but reports Bronze.
#[test]
fn tier_follows_available_not_bonded_amount() {
    let e = Env::default();
    let (client, admin, identity, contract_id) = deploy_with_bond(&e, 1000_i128);
    set_tier_thresholds(&e, &contract_id, 500, 800, 900);

    client.slash(&admin, &identity, &600_i128);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.bonded_amount, 1000);
    assert_eq!(view.available_amount, 400);
    assert_eq!(view.tier, BondTier::Bronze);
}

/// The guard reason is a single exported constant so callers can match on it
/// without duplicating the string.
#[test]
fn blocked_reason_string_is_stable() {
    assert_eq!(
        SLASH_BLOCKED_REASON,
        "slash blocked: collateral increased in this ledger"
    );
}
