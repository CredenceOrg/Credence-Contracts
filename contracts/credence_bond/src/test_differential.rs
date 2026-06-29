//! Regression guard for the canonical bond implementation.
//!
//! # What changed (see docs/bond-crate-layout.md)
//! The previous harness ran four live fork contracts in lock-step and asserted
//! they all produced identical output.  After consolidation:
//!   - `fork_base`, `fork_ours`, and `fork_theirs` were removed (they were
//!     redundant copies of the canonical path that invited silent drift).
//!   - `fork_divergent` is kept as a deliberately-broken probe (Gold for every
//!     amount) to prove the harness still detects behavioural divergence.
//!
//! # Design
//! Scenarios drive the *canonical* `CredenceBond` contract through a scripted
//! lifecycle.  At each checkpoint an `AssertBond` step compares the live bond
//! state against a pinned expected struct.  Because only one contract runs,
//! there is a single authoritative source of truth — no drift is possible.
//!
//! `deliberate_divergence_is_caught` registers both canonical and
//! `fork_divergent` and asserts their tier outputs differ, confirming the
//! comparison logic is still exercised end-to-end.

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

use crate::test_helpers::{self, advance_ledger_sequence};
use crate::{CredenceBond, CredenceBondClient, IdentityBond};

// ---------------------------------------------------------------------------
// Pinned state struct
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Pinned {
    bonded_amount: i128,
    slashed_amount: i128,
    bond_duration: u64,
    active: bool,
    is_rolling: bool,
    withdrawal_requested_at: u64,
}

fn assert_pinned(label: &str, bond: &IdentityBond, p: &Pinned) {
    assert_eq!(
        bond.bonded_amount, p.bonded_amount,
        "[{label}] bonded_amount"
    );
    assert_eq!(
        bond.slashed_amount, p.slashed_amount,
        "[{label}] slashed_amount"
    );
    assert_eq!(
        bond.bond_duration, p.bond_duration,
        "[{label}] bond_duration"
    );
    assert_eq!(bond.active, p.active, "[{label}] active");
    assert_eq!(bond.is_rolling, p.is_rolling, "[{label}] is_rolling");
    assert_eq!(
        bond.withdrawal_requested_at, p.withdrawal_requested_at,
        "[{label}] withdrawal_requested_at"
    );
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

/// Full bond lifecycle: create → top-up → withdraw → slash × 2 → withdraw.
#[test]
fn scenario_full_bond_lifecycle() {
    let env = Env::default();
    let (c, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&env);
    let slash_treasury = Address::generate(&env);
    c.set_slash_treasury(&admin, &slash_treasury);

    c.create_bond(&identity, &1_000_i128, &10_000_u64, &false, &0_u64);
    assert_pinned(
        "after_create",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 1_000,
            slashed_amount: 0,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    c.top_up(&identity, &5_000_i128);
    assert_pinned(
        "after_top_up",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 6_000,
            slashed_amount: 0,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    // Advance past lock-up: bond_start=0, duration=10_000, now=10_001.
    env.ledger().with_mut(|l| l.timestamp = 10_001);
    c.withdraw(&identity, &2_000_i128);
    assert_pinned(
        "after_first_withdraw",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 4_000,
            slashed_amount: 0,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    advance_ledger_sequence(&env);
    c.slash(&admin, &500_i128);
    assert_pinned(
        "after_slash",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 4_000,
            slashed_amount: 500,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    c.slash_bond(&admin, &200_i128);
    assert_pinned(
        "after_slash_bond",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 4_000,
            slashed_amount: 700,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    // Second withdrawal: available = bonded - slashed = 4000 - 700 = 3300.
    env.ledger().with_mut(|l| l.timestamp = 20_001);
    c.withdraw(&identity, &3_300_i128);
    assert_pinned(
        "final",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 700,
            slashed_amount: 700,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );
}

/// Rolling bond: period expiry → renewal → withdrawal request → withdrawal.
///
/// Timing:
///   t=0      create (bond_start=0, duration=5_000, notice=1_000)
///   t=5_001  period ended; RenewIfRolling resets bond_start=5_001
///   t=5_001  RequestWithdrawal → withdrawal_requested_at=5_001
///   t=10_001 past renewed period end (5_001+5_000=10_001) AND notice elapsed
///            (5_001+1_000=6_001 ≤ 10_001)
#[test]
fn scenario_rolling_bond_with_renewal() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(CredenceBond, ());
    let c = CredenceBondClient::new(&env, &id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    c.initialize(&admin, &None);

    c.create_bond(&identity, &50_000_i128, &5_000_u64, &true, &1_000_u64);
    assert_pinned(
        "after_create",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 50_000,
            slashed_amount: 0,
            bond_duration: 5_000,
            active: true,
            is_rolling: true,
            withdrawal_requested_at: 0,
        },
    );

    env.ledger().with_mut(|l| l.timestamp = 5_001);
    c.renew_if_rolling(&identity);
    // apply_renewal sets bond_start = now (5_001) and resets withdrawal_requested_at = 0.
    let bond_after_renew = c.get_identity_state();
    assert_eq!(
        bond_after_renew.bond_start, 5_001,
        "bond_start after renewal"
    );
    assert_eq!(
        bond_after_renew.withdrawal_requested_at, 0,
        "withdrawal_requested_at reset"
    );
    assert_eq!(bond_after_renew.bonded_amount, 50_000);

    c.request_withdrawal(&identity);
    let bond_after_req = c.get_identity_state();
    assert_eq!(
        bond_after_req.withdrawal_requested_at, 5_001,
        "withdrawal_requested_at set to current timestamp"
    );

    // Advance to end of renewed period: 5_001 + 5_000 = 10_001.
    env.ledger().with_mut(|l| l.timestamp = 10_001);
    c.withdraw(&identity, &10_000_i128);
    assert_pinned(
        "after_partial_withdraw",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 40_000,
            slashed_amount: 0,
            bond_duration: 5_000,
            active: true,
            is_rolling: true,
            withdrawal_requested_at: 5_001,
        },
    );
}

/// Early-exit penalty: withdraw before lock-up; verify bonded_amount decreases
/// by the full withdrawal amount (penalty is routed to treasury, not deducted
/// again from bonded_amount).
#[test]
fn scenario_early_exit_and_penalty() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(CredenceBond, ());
    let c = CredenceBondClient::new(&env, &id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    let treasury = Address::generate(&env);
    c.initialize(&admin, &None);

    // Configure a mock token for the test environment.
    let token_id = env.register(crate::test_helpers::MockStellarAsset, ());
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&identity, &20_000_i128); // Mint enough for bond + fees
    c.set_token(&admin, &token_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token_id);
    token_client.approve(&identity, &id, &20_000_i128, &99999);

    // 500 bps = 5% max penalty (time-decayed).
    c.set_early_exit_config(&admin, &treasury, &500_u32);

    c.create_bond(&identity, &10_000_i128, &10_000_u64, &false, &0_u64);
    // Half-way through: remaining = 5_000, duration = 10_000.
    // penalty = 2_000 * (500/10_000) * (5_000/10_000) = 50
    // bonded_amount decreases by amount (2_000), not by (amount - penalty).
    env.ledger().with_mut(|l| l.timestamp = 5_000);
    c.withdraw_early(&identity, &2_000_i128);
    assert_pinned(
        "after_early_exit",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 8_000,
            slashed_amount: 0,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );

    // Past expiry: withdraw remaining available balance.
    env.ledger().with_mut(|l| l.timestamp = 10_001);
    c.withdraw(&identity, &8_000_i128);
    assert_pinned(
        "after_final_withdraw",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 0,
            slashed_amount: 0,
            bond_duration: 10_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );
}

/// Slashing a zero amount must leave bond state unchanged.
#[test]
fn scenario_zero_amount_slash() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(CredenceBond, ());
    let c = CredenceBondClient::new(&env, &id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    c.initialize(&admin, &None);
    c.create_bond(&identity, &5_000_i128, &1_000_u64, &false, &0_u64);

    c.slash(&admin, &0_i128);
    assert_pinned(
        "after_zero_slash",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 5_000,
            slashed_amount: 0,
            bond_duration: 1_000,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );
}

/// `extend_duration` must increase `bond_duration` by the given amount.
#[test]
fn scenario_extend_duration() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(CredenceBond, ());
    let c = CredenceBondClient::new(&env, &id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    c.initialize(&admin, &None);
    c.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    c.extend_duration(&identity, &1_800_u64);
    assert_pinned(
        "after_extend",
        &c.get_identity_state(),
        &Pinned {
            bonded_amount: 1_000,
            slashed_amount: 0,
            bond_duration: 5_400,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
        },
    );
}

/// Renewal at the exact expiry boundary: period ends when `now == bond_start + duration`.
#[test]
fn scenario_rolling_renew_at_exact_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(CredenceBond, ());
    let c = CredenceBondClient::new(&env, &id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    c.initialize(&admin, &None);
    c.create_bond(&identity, &1_000_i128, &3_600_u64, &true, &600_u64);

    // Exactly at expiry: is_period_ended(3600, 0, 3600) → 3600 >= 3600 → true.
    env.ledger().with_mut(|l| l.timestamp = 3_600);
    c.renew_if_rolling(&identity);
    let bond = c.get_identity_state();
    assert_eq!(bond.bond_start, 3_600, "bond_start after first renewal");
    assert_eq!(bond.bond_duration, 3_600);
    assert_eq!(
        bond.withdrawal_requested_at, 0,
        "withdrawal_requested_at reset by renewal"
    );

    // Past end of renewed period: 3_600 + 3_600 = 7_200; advance past it.
    env.ledger().with_mut(|l| l.timestamp = 7_201);
    c.renew_if_rolling(&identity);
    let bond2 = c.get_identity_state();
    assert_eq!(bond2.bond_start, 7_201, "bond_start after second renewal");
}

// ---------------------------------------------------------------------------
// Deliberate-divergence smoke test — proves the harness still detects bugs.
// ---------------------------------------------------------------------------

/// `fork_divergent` returns `Gold` for every amount ≥ 1, while the canonical
/// implementation returns `Bronze` for amounts below 1×10²¹.  Asserting the
/// two differ confirms the comparison logic is exercised.
#[test]
fn deliberate_divergence_is_caught() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let identity = Address::generate(&env);

    let canonical_id = env.register(CredenceBond, ());
    let canonical = CredenceBondClient::new(&env, &canonical_id);

    let divergent_id = env.register(crate::fork_divergent::CredenceBond, ());
    let divergent = crate::fork_divergent::CredenceBondClient::new(&env, &divergent_id);

    canonical.initialize(&admin, &None);
    divergent.initialize(&admin);

    canonical.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
    divergent.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);

    // Canonical: 1_000 < TIER_BRONZE_MAX (1×10²¹) → Bronze.
    // Divergent: any amount ≥ 1 → Gold.
    assert_ne!(
        canonical.get_tier(),
        divergent.get_tier(),
        "divergent fork tier must differ from canonical"
    );
    assert_eq!(canonical.get_tier(), crate::BondTier::Bronze);
    assert_eq!(divergent.get_tier(), crate::BondTier::Gold);
}
