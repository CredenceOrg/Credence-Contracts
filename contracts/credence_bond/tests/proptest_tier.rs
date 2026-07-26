#![cfg(test)]

//! # Bond Tier Transition Proptests
//!
//! Deepens the fuzz surface for tier transitions across rolling renew + slash sequencing.
//!
//! ## Invariants Enforced:
//! - **I2 - Slashed never exceeds bonded**: `bonded_amount >= slashed_amount`.
//! - **No negative net bond**: `available_amount >= 0` where `available_amount = bonded_amount - slashed_amount`.
//! - **Bonded non-negative**: `bonded_amount >= 0`.
//! - **Slashed non-negative**: `slashed_amount >= 0`.
//! - **Tier is a pure function of net bond**: A bond's tier matches the configured threshold
//!   applied directly to its available (net) bond amount.
//! - **Tier Monotonicity**: operations that decrease the value/net bond of a bond (Slash,
//!   WithdrawEarly, Settle) must never increase its tier rank (`tier_after <= tier_before`).

use credence_bond::soroban_sdk::testutils::{Address as _, Ledger as _};
use credence_bond::soroban_sdk::{Address, Env};
use credence_bond::{BondTier, CredenceBond, CredenceBondClient};
use proptest::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Actions that can be performed on a bond during property-based fuzzing.
#[derive(Clone, Debug)]
pub enum BondAction {
    /// Create a new bond.
    Deposit {
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    },
    /// Increase the bonded amount.
    TopUp { amount: i128 },
    /// Slash a portion of the bond.
    Slash { amount: i128 },
    /// Renew the bond if it is rolling.
    RollRenew,
    /// Withdraw early with a penalty.
    WithdrawEarly { amount: i128 },
    /// Request withdrawal for a rolling bond.
    RequestWithdraw,
    /// Fully settle and withdraw the bond.
    Settle,
}

// Custom Arbitrary implementation for BondAction to generate realistic test scenarios
impl Arbitrary for BondAction {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        // Generate amounts spanning all tier boundaries:
        // Bronze: < 10^21
        // Silver: 10^21 .. 5 * 10^21
        // Gold: 5 * 10^21 .. 20 * 10^21
        // Platinum: >= 20 * 10^21
        let amount_strategy = prop_oneof![
            1_000i128..1_000_000_000_000_000_000_000i128, // Bronze range
            1_000_000_000_000_000_000_000i128..5_000_000_000_000_000_000_000i128, // Silver range
            5_000_000_000_000_000_000_000i128..20_000_000_000_000_000_000_000i128, // Gold range
            20_000_000_000_000_000_000_000i128..100_000_000_000_000_000_000_000i128 // Platinum range
        ];

        prop_oneof![
            // Deposit / Create Bond
            // Durations must be valid: [MIN_BOND_DURATION (86_400), MAX_BOND_DURATION (31_536_000)]
            (
                amount_strategy.clone(),
                86400u64..31536000u64,
                any::<bool>()
            )
                .prop_flat_map(|(amount, duration, is_rolling)| {
                    let notice_period_strat = if is_rolling {
                        1u64..=duration
                    } else {
                        0u64..=0
                    };
                    notice_period_strat.prop_map(move |notice_period_duration| {
                        BondAction::Deposit {
                            amount,
                            duration,
                            is_rolling,
                            notice_period_duration,
                        }
                    })
                }),
            // TopUp
            amount_strategy
                .clone()
                .prop_map(|amount| BondAction::TopUp { amount }),
            // Slash
            amount_strategy
                .clone()
                .prop_map(|amount| BondAction::Slash { amount }),
            // RollRenew
            Just(BondAction::RollRenew),
            // WithdrawEarly
            amount_strategy
                .clone()
                .prop_map(|amount| BondAction::WithdrawEarly { amount }),
            // RequestWithdraw
            Just(BondAction::RequestWithdraw),
            // Settle
            Just(BondAction::Settle),
        ]
        .boxed()
    }
}

/// Helper to get the rank of a BondTier for monotonicity assertions.
fn get_tier_rank(tier: &BondTier) -> u8 {
    match tier {
        BondTier::Bronze => 0,
        BondTier::Silver => 1,
        BondTier::Gold => 2,
        BondTier::Platinum => 3,
    }
}

/// Helper to determine the expected BondTier for a given amount using default thresholds.
fn expected_tier_for_amount(amount: i128) -> BondTier {
    const TIER_BRONZE_MAX: i128 = 1_000_000_000_000_000_000_000;
    const TIER_SILVER_MAX: i128 = 5_000_000_000_000_000_000_000;
    const TIER_GOLD_MAX: i128 = 20_000_000_000_000_000_000_000;

    if amount < TIER_BRONZE_MAX {
        BondTier::Bronze
    } else if amount < TIER_SILVER_MAX {
        BondTier::Silver
    } else if amount < TIER_GOLD_MAX {
        BondTier::Gold
    } else {
        BondTier::Platinum
    }
}

use std::sync::Once;
static INIT: Once = Once::new();

/// Executes a sequence of generated BondActions and validates core invariants.
fn run_sequence(actions: &[BondAction]) {
    INIT.call_once(|| {
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
    });

    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    // Initialize the contract
    client.initialize(&admin);

    for (step_idx, action) in actions.iter().enumerate() {
        // 1. Advance sequence and timestamp slightly before every action
        // This avoids same-ledger guard limitations and lets time flow naturally.
        e.ledger().with_mut(|li| {
            li.sequence_number = li.sequence_number.saturating_add(1);
            li.timestamp = li.timestamp.saturating_add(1);
        });

        // 2. Query bond state before executing the action
        let bond_before = client.describe_bond(&identity);

        // 3. Time orchestration: to ensure coverage of maturity, notice period, and rolling
        // renewals, we optionally force time forward to exact match conditions.
        if let Some(ref bond) = bond_before {
            match action {
                BondAction::RollRenew => {
                    // Force time to exact expiration of the current period
                    let expiry = bond.bond_start.saturating_add(bond.bond_duration);
                    if e.ledger().timestamp() < expiry {
                        e.ledger().with_mut(|li| li.timestamp = expiry);
                    }
                }
                BondAction::Settle => {
                    if bond.is_rolling && bond.withdrawal_requested_at > 0 {
                        // Force time to end of notice period
                        let eligible = bond
                            .withdrawal_requested_at
                            .saturating_add(bond.notice_period_duration);
                        if e.ledger().timestamp() < eligible {
                            e.ledger().with_mut(|li| li.timestamp = eligible);
                        }
                    } else if !bond.is_rolling {
                        // Force time to maturity
                        let maturity = bond.bond_start.saturating_add(bond.bond_duration);
                        if e.ledger().timestamp() < maturity {
                            e.ledger().with_mut(|li| li.timestamp = maturity);
                        }
                    }
                }
                _ => {}
            }
        }

        // 4. Execute the action, catching expected/validation contract panics
        let res = catch_unwind(AssertUnwindSafe(|| match action {
            BondAction::Deposit {
                amount,
                duration,
                is_rolling,
                notice_period_duration,
            } => {
                client.create_bond(
                    &identity,
                    amount,
                    duration,
                    is_rolling,
                    notice_period_duration,
                );
            }
            BondAction::TopUp { amount } => {
                client.top_up(amount);
            }
            BondAction::Slash { amount } => {
                client.slash(&admin, amount);
            }
            BondAction::RollRenew => {
                client.renew_if_rolling();
            }
            BondAction::WithdrawEarly { amount } => {
                client.withdraw_early(amount);
            }
            BondAction::RequestWithdraw => {
                client.request_withdrawal();
            }
            BondAction::Settle => {
                client.withdraw_bond(&identity);
            }
        }));

        // 5. Query bond state after executing the action
        let bond_after = client.describe_bond(&identity);

        // 6. Assert invariants
        if res.is_ok() {
            if let Some(ref after) = bond_after {
                // Invariant I2: bonded_amount >= slashed_amount
                assert!(
                    after.bonded_amount >= after.slashed_amount,
                    "[Step {}] Invariant violation (bonded < slashed): bonded={}, slashed={} after action {:?}",
                    step_idx,
                    after.bonded_amount,
                    after.slashed_amount,
                    action
                );

                // Invariant: no negative net bond (available_amount >= 0)
                assert!(
                    after.available_amount >= 0,
                    "[Step {}] Invariant violation (negative net bond): available_amount={} after action {:?}",
                    step_idx,
                    after.available_amount,
                    action
                );

                // Invariant I4: bonded_amount >= 0
                assert!(after.bonded_amount >= 0);

                // Invariant I5: slashed_amount >= 0
                assert!(after.slashed_amount >= 0);

                // Invariant: tier is a pure function of net bond (available_amount)
                let expected = expected_tier_for_amount(after.available_amount);
                assert_eq!(
                    after.tier, expected,
                    "[Step {}] Invariant violation: described tier {:?} does not match expected tier {:?} for available_amount {}",
                    step_idx,
                    after.tier,
                    expected,
                    after.available_amount
                );

                let calculated_tier = client.get_tier();
                assert_eq!(
                    after.tier, calculated_tier,
                    "[Step {}] Invariant violation: described tier does not match get_tier()",
                    step_idx
                );

                // Invariant: tier monotonicity (tier rank must never increase after operations that decrease amount/value)
                if let Some(ref before) = bond_before {
                    let rank_before = get_tier_rank(&before.tier);
                    let rank_after = get_tier_rank(&after.tier);

                    match action {
                        BondAction::Slash { .. } => {
                            assert!(
                                rank_after <= rank_before,
                                "[Step {}] Monotonicity violation: tier increased from {:?} to {:?} after Slash",
                                step_idx,
                                before.tier,
                                after.tier
                            );
                        }
                        BondAction::WithdrawEarly { .. } | BondAction::Settle => {
                            assert!(
                                rank_after <= rank_before,
                                "[Step {}] Monotonicity violation: tier increased from {:?} to {:?} after Withdrawal/Settle",
                                step_idx,
                                before.tier,
                                after.tier
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Post-sequence check: Final integrity of storage bond
    if let Some(final_bond) = client.describe_bond(&identity) {
        assert!(final_bond.bonded_amount >= final_bond.slashed_amount);
        assert!(final_bond.available_amount >= 0);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10000,
        .. ProptestConfig::default()
    })]
    #[test]
    fn test_tier_fuzz(ref actions in prop::collection::vec(any::<BondAction>(), 1..256)) {
        run_sequence(actions);
    }
}
