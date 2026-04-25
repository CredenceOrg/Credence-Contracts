//! Tests for the bond status snapshot helper (issue #275).
//!
//! Coverage targets:
//! - Tier derivation for all four tiers
//! - Cooldown remaining: no request, active, elapsed
//! - Emergency mode: unset (default false), set false, set true
//! - Available balance: unslashed, partially slashed, fully slashed
//! - snapshot_timestamp reflects ledger time

use crate::test_helpers;
use crate::{BondTier, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    let governance = Address::generate(e);
    let treasury = Address::generate(e);
    (client, admin, identity, governance, treasury)
}

// ── tier derivation ───────────────────────────────────────────────────────────

#[test]
fn test_snapshot_tier_bronze() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // < 1_000_000_000 → Bronze
    client.create_bond_with_rolling(&identity, &500_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.tier, BondTier::Bronze);
}

#[test]
fn test_snapshot_tier_silver() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 1_000_000_000 ≤ x < 5_000_000_000 → Silver
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.tier, BondTier::Silver);
}

#[test]
fn test_snapshot_tier_gold() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 5_000_000_000 ≤ x < 20_000_000_000 → Gold
    client.create_bond_with_rolling(&identity, &5_000_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.tier, BondTier::Gold);
}

#[test]
fn test_snapshot_tier_platinum() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // ≥ 20_000_000_000 → Platinum
    client.create_bond_with_rolling(&identity, &20_000_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.tier, BondTier::Platinum);
}

// ── cooldown remaining ────────────────────────────────────────────────────────

#[test]
fn test_snapshot_cooldown_no_request() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    client.set_cooldown_period(&admin, &3600_u64);
    // No withdrawal request → cooldown_remaining_secs == 0
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 0);
}

#[test]
fn test_snapshot_cooldown_active() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &true, &3600_u64);
    client.set_cooldown_period(&admin, &3600_u64);
    // Request withdrawal at t=1000; cooldown ends at t=4600
    client.request_withdrawal(&identity);
    // Still at t=1000 → 3600 s remaining
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 3600);
}

#[test]
fn test_snapshot_cooldown_partially_elapsed() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &true, &3600_u64);
    client.set_cooldown_period(&admin, &3600_u64);
    client.request_withdrawal(&identity);
    // Advance 1800 s → 1800 s remaining
    e.ledger().with_mut(|li| li.timestamp = 2800);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 1800);
}

#[test]
fn test_snapshot_cooldown_elapsed() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &true, &3600_u64);
    client.set_cooldown_period(&admin, &3600_u64);
    client.request_withdrawal(&identity);
    // Advance past cooldown end
    e.ledger().with_mut(|li| li.timestamp = 4601);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 0);
}

#[test]
fn test_snapshot_cooldown_zero_period() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &true, &3600_u64);
    // No cooldown period set → 0
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 0);
}

// ── emergency mode ────────────────────────────────────────────────────────────

#[test]
fn test_snapshot_emergency_mode_default_false() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    // No emergency config set → defaults to false
    let snap = client.get_bond_status_snapshot();
    assert!(!snap.emergency_mode);
}

#[test]
fn test_snapshot_emergency_mode_set_false() {
    let e = Env::default();
    let (client, admin, identity, governance, treasury) = setup(&e);
    client.set_emergency_config(&admin, &governance, &treasury, &500_u32, &false);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert!(!snap.emergency_mode);
}

#[test]
fn test_snapshot_emergency_mode_enabled() {
    let e = Env::default();
    let (client, admin, identity, governance, treasury) = setup(&e);
    client.set_emergency_config(&admin, &governance, &treasury, &500_u32, &true);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert!(snap.emergency_mode);
}

#[test]
fn test_snapshot_emergency_mode_toggled() {
    let e = Env::default();
    let (client, admin, identity, governance, treasury) = setup(&e);
    client.set_emergency_config(&admin, &governance, &treasury, &500_u32, &false);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    assert!(!client.get_bond_status_snapshot().emergency_mode);

    client.set_emergency_mode(&admin, &governance, &true);
    assert!(client.get_bond_status_snapshot().emergency_mode);

    client.set_emergency_mode(&admin, &governance, &false);
    assert!(!client.get_bond_status_snapshot().emergency_mode);
}

// ── available balance ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_available_balance_unslashed() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.available_balance, 1_000);
}

#[test]
fn test_snapshot_available_balance_partially_slashed() {
    let e = Env::default();
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &300_i128);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.available_balance, 700);
}

#[test]
fn test_snapshot_available_balance_fully_slashed() {
    let e = Env::default();
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &1_000_i128);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.available_balance, 0);
}

// ── snapshot_timestamp ────────────────────────────────────────────────────────

#[test]
fn test_snapshot_timestamp_reflects_ledger() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 99_999);
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.snapshot_timestamp, 99_999);
}

// ── no bond guard ─────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "no bond")]
fn test_snapshot_panics_when_no_bond() {
    let e = Env::default();
    let (client, ..) = setup(&e);
    client.get_bond_status_snapshot();
}

// ── combined scenario ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_combined_state() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 5000);
    let (client, admin, identity, governance, treasury) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &0_u32, &true);
    client.set_cooldown_period(&admin, &2000_u64);
    // Gold tier: 10_000_000_000
    client.create_bond_with_rolling(
        &identity,
        &10_000_000_000_i128,
        &86400_u64,
        &true,
        &2000_u64,
    );
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &1_000_000_000_i128);
    client.request_withdrawal(&identity);

    // Advance 500 s into the 2000 s cooldown
    e.ledger().with_mut(|li| li.timestamp = 5500);
    let snap = client.get_bond_status_snapshot();

    assert_eq!(snap.tier, BondTier::Gold);
    assert_eq!(snap.cooldown_remaining_secs, 1500);
    assert!(snap.emergency_mode);
    assert_eq!(snap.available_balance, 9_000_000_000);
    assert_eq!(snap.snapshot_timestamp, 5500);
}

// ── tier boundary edges ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_tier_boundary_bronze_max() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 999_999_999 → still Bronze
    client.create_bond_with_rolling(&identity, &999_999_999_i128, &86400_u64, &false, &0_u64);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Bronze);
}

#[test]
fn test_snapshot_tier_boundary_silver_min() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 1_000_000_000 → Silver
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Silver);
}

#[test]
fn test_snapshot_tier_boundary_gold_min() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 5_000_000_000 → Gold
    client.create_bond_with_rolling(&identity, &5_000_000_000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Gold);
}

#[test]
fn test_snapshot_tier_boundary_platinum_min() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // 20_000_000_000 → Platinum
    client.create_bond_with_rolling(&identity, &20_000_000_000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Platinum);
}

// ── snapshot is read-only (does not mutate state) ─────────────────────────────

#[test]
fn test_snapshot_does_not_mutate_bond() {
    let e = Env::default();
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &200_i128);

    let before = client.get_identity_state();
    let _ = client.get_bond_status_snapshot();
    let after = client.get_identity_state();

    assert_eq!(before.bonded_amount, after.bonded_amount);
    assert_eq!(before.slashed_amount, after.slashed_amount);
}

// ── snapshot after top-up changes tier ───────────────────────────────────────

#[test]
fn test_snapshot_tier_updates_after_top_up() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    // Start Bronze
    client.create_bond_with_rolling(&identity, &500_000_000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Bronze);

    // Top up to Silver
    client.top_up(&identity, &500_000_000_i128);
    assert_eq!(client.get_bond_status_snapshot().tier, BondTier::Silver);
}

// ── cooldown exact boundary ───────────────────────────────────────────────────

#[test]
fn test_snapshot_cooldown_exactly_at_end() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &1_000_000_000_i128, &86400_u64, &true, &3600_u64);
    client.set_cooldown_period(&admin, &3600_u64);
    client.request_withdrawal(&identity);
    // Exactly at end (t = 1000 + 3600 = 4600) → elapsed, remaining = 0
    e.ledger().with_mut(|li| li.timestamp = 4600);
    let snap = client.get_bond_status_snapshot();
    assert_eq!(snap.cooldown_remaining_secs, 0);
}

// ── snapshot symbol used in lib.rs ───────────────────────────────────────────

#[test]
fn test_snapshot_symbol_unused_in_test() {
    // Ensures the Symbol import compiles correctly in the test module.
    let e = Env::default();
    let _sym = Symbol::new(&e, "test");
}
