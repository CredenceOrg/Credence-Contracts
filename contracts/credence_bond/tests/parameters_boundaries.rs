//! Boundary-case coverage for `parameters.rs` (issue #1343).
//!
//! ## Why this lives in `tests/` and not `src/`
//!
//! The in-crate `--lib` test target does not compile on `main` (431 errors
//! across `test_batch`, `test_slashing`, `test_events_*`, `test_validation`,
//! `test_pausable_*`, and others). An in-crate module would therefore never be
//! compiled and never execute. Integration tests link the production `rlib`,
//! which does build, so the coverage here actually runs. This mirrors the
//! decision already taken for `access_control` in issue #1316.

#![cfg(test)]

use credence_bond::parameters::{
    require_not_borrow_frozen, set_borrow_frozen_with_approval, GovernanceApproval,
    DEFAULT_ATTESTATION_FEE_BPS, DEFAULT_BRONZE_THRESHOLD, DEFAULT_GOLD_THRESHOLD,
    DEFAULT_MAX_LEVERAGE, DEFAULT_PLATINUM_THRESHOLD, DEFAULT_PROTOCOL_FEE_BPS,
    DEFAULT_SILVER_THRESHOLD, DEFAULT_SLASH_COOLDOWN_SECS, DEFAULT_WITHDRAWAL_COOLDOWN_SECS,
    MAX_ATTESTATION_FEE_BPS, MAX_BRONZE_THRESHOLD, MAX_GOLD_THRESHOLD, MAX_MAX_LEVERAGE,
    MAX_PLATINUM_THRESHOLD, MAX_PROTOCOL_FEE_BPS, MAX_QUERY_LIMIT, MAX_SILVER_THRESHOLD,
    MAX_SLASH_COOLDOWN_SECS, MAX_WITHDRAWAL_COOLDOWN_SECS, MIN_ATTESTATION_FEE_BPS,
    MIN_BRONZE_THRESHOLD, MIN_GOLD_THRESHOLD, MIN_MAX_LEVERAGE, MIN_PLATINUM_THRESHOLD,
    MIN_PROTOCOL_FEE_BPS, MIN_SILVER_THRESHOLD, MIN_SLASH_COOLDOWN_SECS,
    MIN_WITHDRAWAL_COOLDOWN_SECS,
};
use credence_bond::soroban_sdk::testutils::{Address as _, Ledger};
use credence_bond::soroban_sdk::{symbol_short, Address, Env, Symbol};
use credence_bond::{CredenceBond, CredenceBondClient};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::{String as StdString, ToString};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

fn panic_message<F: FnOnce()>(f: F) -> StdString {
    let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a panic");
    let raw = if let Some(s) = payload.downcast_ref::<StdString>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        panic!("panic payload was neither String nor &str");
    };
    unwrap_contract_panic(&raw)
}

/// A contract-invoked panic reaches the caller wrapped by the Soroban host as
/// `HostError: Error(WasmVm, InvalidAction)` with the real message embedded in a
/// `caught panic '<message>'` diagnostic line. Strip that envelope so assertions
/// can pin the contract's own message.
fn unwrap_contract_panic(raw: &str) -> StdString {
    const MARKER: &str = "caught panic '";
    let Some(start) = raw.find(MARKER) else {
        return raw.to_string();
    };
    let rest = &raw[start + MARKER.len()..];
    let Some(end) = rest.find('\'') else {
        return raw.to_string();
    };
    rest[..end].to_string()
}

/// `ContractError::BorrowFrozen` in the shared error crate. Asserted numerically
/// because the host renders a `panic_with_error!` as `Error(Contract, #114)`.
const BORROW_FROZEN_ERROR_CODE: u32 = credence_errors::ContractError::BorrowFrozen as u32;

#[track_caller]
fn expect_panic_with<F: FnOnce()>(expected: &str, f: F) {
    let msg = panic_message(f);
    assert_eq!(msg, expected);
}

// ---------------------------------------------------------------------------
// B1: Fee-rate bounds (u32)
// ---------------------------------------------------------------------------

/// Protocol fee bps accepts both inclusive endpoints and rejects one step past
/// either edge, pinning `MIN..=MAX` as inclusive.
#[test]
fn protocol_fee_bps_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &MIN_PROTOCOL_FEE_BPS);
    assert_eq!(client.get_protocol_fee_bps(), MIN_PROTOCOL_FEE_BPS);

    client.set_protocol_fee_bps(&admin, &MAX_PROTOCOL_FEE_BPS);
    assert_eq!(client.get_protocol_fee_bps(), MAX_PROTOCOL_FEE_BPS);

    client.set_protocol_fee_bps(&admin, &(MIN_PROTOCOL_FEE_BPS + 1));
    assert_eq!(client.get_protocol_fee_bps(), 1);

    client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS - 1));
    assert_eq!(client.get_protocol_fee_bps(), 999);
}

/// One basis point above the documented 10% ceiling must be rejected.
#[test]
fn protocol_fee_bps_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("protocol_fee_bps out of bounds", || {
        client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
    });
}

/// `u32::MAX` must be rejected rather than silently wrapping into range.
#[test]
fn protocol_fee_bps_u32_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("protocol_fee_bps out of bounds", || {
        client.set_protocol_fee_bps(&admin, &u32::MAX);
    });
}

/// Attestation fee bps uses a tighter 5% ceiling than the protocol fee.
#[test]
fn attestation_fee_bps_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &MIN_ATTESTATION_FEE_BPS);
    assert_eq!(client.get_attestation_fee_bps(), 0);

    client.set_attestation_fee_bps(&admin, &MAX_ATTESTATION_FEE_BPS);
    assert_eq!(client.get_attestation_fee_bps(), 500);
}

/// One basis point above the attestation ceiling is rejected.
#[test]
fn attestation_fee_bps_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("attestation_fee_bps out of bounds", || {
        client.set_attestation_fee_bps(&admin, &(MAX_ATTESTATION_FEE_BPS + 1));
    });
}

// ---------------------------------------------------------------------------
// B2: Cooldown bounds (u64)
// ---------------------------------------------------------------------------

/// Withdrawal cooldown accepts 0 (no cooldown) and the full 30-day cap.
#[test]
fn withdrawal_cooldown_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &MIN_WITHDRAWAL_COOLDOWN_SECS);
    assert_eq!(client.get_withdrawal_cooldown_secs(), 0);

    client.set_withdrawal_cooldown_secs(&admin, &MAX_WITHDRAWAL_COOLDOWN_SECS);
    assert_eq!(client.get_withdrawal_cooldown_secs(), 2_592_000);
}

/// One second past the 30-day withdrawal cap is rejected.
#[test]
fn withdrawal_cooldown_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("withdrawal_cooldown_secs out of bounds", || {
        client.set_withdrawal_cooldown_secs(&admin, &(MAX_WITHDRAWAL_COOLDOWN_SECS + 1));
    });
}

/// `u64::MAX` must be rejected rather than wrapping.
#[test]
fn withdrawal_cooldown_u64_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("withdrawal_cooldown_secs out of bounds", || {
        client.set_withdrawal_cooldown_secs(&admin, &u64::MAX);
    });
}

/// Slash cooldown accepts 0 and the full 7-day cap.
#[test]
fn slash_cooldown_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &MIN_SLASH_COOLDOWN_SECS);
    assert_eq!(client.get_slash_cooldown_secs(), 0);

    client.set_slash_cooldown_secs(&admin, &MAX_SLASH_COOLDOWN_SECS);
    assert_eq!(client.get_slash_cooldown_secs(), 604_800);
}

/// One second past the 7-day slash cap is rejected.
#[test]
fn slash_cooldown_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("slash_cooldown_secs out of bounds", || {
        client.set_slash_cooldown_secs(&admin, &(MAX_SLASH_COOLDOWN_SECS + 1));
    });
}

// ---------------------------------------------------------------------------
// B3: Tier-threshold bounds (i128)
// ---------------------------------------------------------------------------

/// Bronze accepts 0 and its documented 1M-token ceiling.
#[test]
fn bronze_threshold_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &MIN_BRONZE_THRESHOLD);
    assert_eq!(client.get_bronze_threshold(), 0);

    client.set_bronze_threshold(&admin, &MAX_BRONZE_THRESHOLD);
    assert_eq!(client.get_bronze_threshold(), 1_000_000_000_000);
}

/// One unit above the bronze ceiling is rejected.
#[test]
fn bronze_threshold_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("bronze_threshold out of bounds", || {
        client.set_bronze_threshold(&admin, &(MAX_BRONZE_THRESHOLD + 1));
    });
}

/// Tier thresholds are `i128`; a negative value must be rejected, not wrapped.
#[test]
fn bronze_threshold_negative_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("bronze_threshold out of bounds", || {
        client.set_bronze_threshold(&admin, &-1);
    });
}

/// `i128::MIN` must be rejected without panicking on negation/overflow.
#[test]
fn bronze_threshold_i128_min_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("bronze_threshold out of bounds", || {
        client.set_bronze_threshold(&admin, &i128::MIN);
    });
}

/// Silver floor is above bronze's, so a below-floor value is rejected.
#[test]
fn silver_threshold_below_min_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("silver_threshold out of bounds", || {
        client.set_silver_threshold(&admin, &(MIN_SILVER_THRESHOLD - 1));
    });
}

/// Silver accepts its 10M-token ceiling.
#[test]
fn silver_threshold_max_is_accepted() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_silver_threshold(&admin, &MAX_SILVER_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), 10_000_000_000_000);
}

/// One unit above the silver ceiling is rejected.
#[test]
fn silver_threshold_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("silver_threshold out of bounds", || {
        client.set_silver_threshold(&admin, &(MAX_SILVER_THRESHOLD + 1));
    });
}

/// Gold floor is above silver's; one unit below is rejected.
#[test]
fn gold_threshold_below_min_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("gold_threshold out of bounds", || {
        client.set_gold_threshold(&admin, &(MIN_GOLD_THRESHOLD - 1));
    });
}

/// Gold accepts its 100M-token ceiling.
#[test]
fn gold_threshold_max_is_accepted() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_gold_threshold(&admin, &MAX_GOLD_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), 100_000_000_000_000);
}

/// One unit above the gold ceiling is rejected.
#[test]
fn gold_threshold_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("gold_threshold out of bounds", || {
        client.set_gold_threshold(&admin, &(MAX_GOLD_THRESHOLD + 1));
    });
}

/// Platinum floor is above gold's; one unit below is rejected.
#[test]
fn platinum_threshold_below_min_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("platinum_threshold out of bounds", || {
        client.set_platinum_threshold(&admin, &(MIN_PLATINUM_THRESHOLD - 1));
    });
}

/// Platinum accepts its 1B-token ceiling.
#[test]
fn platinum_threshold_max_is_accepted() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_platinum_threshold(&admin, &MAX_PLATINUM_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), 1_000_000_000_000_000);
}

/// One unit above the platinum ceiling is rejected.
#[test]
fn platinum_threshold_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("platinum_threshold out of bounds", || {
        client.set_platinum_threshold(&admin, &(MAX_PLATINUM_THRESHOLD + 1));
    });
}

/// Tier thresholds round-trip independently at their exact endpoints.
#[test]
fn all_tier_thresholds_round_trip_at_endpoints() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &MAX_BRONZE_THRESHOLD);
    client.set_silver_threshold(&admin, &MIN_SILVER_THRESHOLD);
    client.set_gold_threshold(&admin, &MIN_GOLD_THRESHOLD);
    client.set_platinum_threshold(&admin, &MIN_PLATINUM_THRESHOLD);

    assert_eq!(client.get_bronze_threshold(), MAX_BRONZE_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), MIN_SILVER_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), MIN_GOLD_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), MIN_PLATINUM_THRESHOLD);
}

// ---------------------------------------------------------------------------
// B4: Max-leverage bounds (u32)
// ---------------------------------------------------------------------------

/// Max leverage accepts 1x (single-token positions) and the 100M ceiling.
#[test]
fn max_leverage_bounds_are_inclusive() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_max_leverage(&admin, &MIN_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), 1);

    client.set_max_leverage(&admin, &MAX_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), 100_000_000);
}

/// Zero leverage is rejected: 0 would reject every bond.
#[test]
fn max_leverage_zero_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("max_leverage out of bounds", || {
        client.set_max_leverage(&admin, &0);
    });
}

/// One above the 100M ceiling is rejected.
#[test]
fn max_leverage_above_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("max_leverage out of bounds", || {
        client.set_max_leverage(&admin, &(MAX_MAX_LEVERAGE + 1));
    });
}

/// `u32::MAX` must be rejected rather than truncating to an in-range value.
#[test]
fn max_leverage_u32_max_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    expect_panic_with("max_leverage out of bounds", || {
        client.set_max_leverage(&admin, &u32::MAX);
    });
}

// ---------------------------------------------------------------------------
// B5: GovernanceApproval expiry off-by-one
// ---------------------------------------------------------------------------

/// `validate_governance_approval` rejects only when `timestamp > expires_at`,
/// so an approval is still valid *at* its expiry second. This is the exact
/// off-by-one the `>` (not `>=`) comparison produces and must not silently
/// drift to `>=`.
#[test]
fn approval_is_valid_at_the_exact_expiry_second() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 1_000);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 1_000,
        category: symbol_short!("cooldown"),
    };
    client.set_slash_cd_secs_appr(&admin, &4_321, &approval);
    assert_eq!(client.get_slash_cooldown_secs(), 4_321);
}

/// One second past expiry is rejected.
#[test]
fn approval_one_second_past_expiry_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 1_000);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 1_000,
        category: symbol_short!("cooldown"),
    };
    e.ledger().with_mut(|li| li.timestamp = 1_001);
    expect_panic_with("governance approval expired", || {
        client.set_slash_cd_secs_appr(&admin, &4_321, &approval);
    });
}

/// One second before expiry is still accepted, bracketing the boundary.
#[test]
fn approval_one_second_before_expiry_is_accepted() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 999);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 1_000,
        category: symbol_short!("cooldown"),
    };
    client.set_slash_cd_secs_appr(&admin, &4_321, &approval);
    assert_eq!(client.get_slash_cooldown_secs(), 4_321);
}

/// `expires_at == 0` is the documented "no expiry" sentinel and must never be
/// treated as already-expired, even at a very late ledger timestamp.
#[test]
fn approval_with_zero_expiry_never_expires() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 9_999_999_999);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    client.set_protocol_fee_bps_appr(&admin, &250, &approval);
    assert_eq!(client.get_protocol_fee_bps(), 250);
}

/// An approval must be signed by the same address as the admin argument.
#[test]
fn approval_rejects_approver_mismatch() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let other = Address::generate(&e);

    let approval = GovernanceApproval {
        approver: other,
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    expect_panic_with("governance approver mismatch", || {
        client.set_protocol_fee_bps_appr(&admin, &250, &approval);
    });
}

// ---------------------------------------------------------------------------
// B6: Approval category matrix
// ---------------------------------------------------------------------------

/// Each setter enforces exactly one category. `fee` is rejected by a cooldown
/// setter, proving the category is checked and not merely defaulted.
#[test]
fn cooldown_setter_rejects_fee_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_withdrawal_cd_secs_appr(&admin, &3_600, &approval);
    });
}

/// A tier approval must not authorise a cooldown change.
#[test]
fn cooldown_setter_rejects_tier_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("tier"),
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_slash_cd_secs_appr(&admin, &3_600, &approval);
    });
}

/// A tier approval correctly authorises a tier change.
#[test]
fn tier_setter_accepts_tier_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("tier"),
    };
    client.set_bronze_threshold_appr(&admin, &200_000_000, &approval);
    assert_eq!(client.get_bronze_threshold(), 200_000_000);
}

/// A `risk` approval authorises max leverage but not a tier change.
#[test]
fn tier_setter_rejects_risk_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("risk"),
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_gold_threshold_appr(&admin, &2_000_000_000, &approval);
    });
}

/// `risk` correctly authorises max leverage.
#[test]
fn max_leverage_setter_accepts_risk_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("risk"),
    };
    client.set_max_leverage_appr(&admin, &50_000, &approval);
    assert_eq!(client.get_max_leverage(), 50_000);
}

/// `fee` correctly authorises the attestation fee rate.
#[test]
fn attestation_fee_setter_accepts_fee_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    client.set_attestation_fee_bps_appr(&admin, &77, &approval);
    assert_eq!(client.get_attestation_fee_bps(), 77);
}

// ---------------------------------------------------------------------------
// B7: Validation ordering
// ---------------------------------------------------------------------------

/// Admin authorisation is checked before the approval envelope, so a
/// non-admin using a self-consistent approval still hits `"not admin"`. This
/// pins the order `validate_admin` -> `validate_governance_approval` -> bounds
/// and prevents an approval check from shadowing the authorisation gate.
#[test]
fn admin_check_precedes_approval_validation() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    // The approval is internally consistent for `stranger`, so only the admin
    // gate can produce this panic.
    let approval = GovernanceApproval {
        approver: stranger.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    expect_panic_with("not admin", || {
        client.set_protocol_fee_bps_appr(&stranger, &250, &approval);
    });
}

/// Approval validation precedes bounds checking, so an admin presenting a
/// wrong-category approval *and* an out-of-range value sees the approval error.
#[test]
fn approval_check_precedes_bounds_check() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("tier"), // wrong for a cooldown setter
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_withdrawal_cd_secs_appr(&admin, &u64::MAX, &approval);
    });
}

/// Expiry is checked before category, so an expired approval with a wrong
/// category reports expiry first.
#[test]
fn expiry_check_precedes_category_check() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 5_000);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 4_999,               // expired
        category: symbol_short!("tier"), // also wrong
    };
    expect_panic_with("governance approval expired", || {
        client.set_withdrawal_cd_secs_appr(&admin, &3_600, &approval);
    });
}

// ---------------------------------------------------------------------------
// B8: Constant invariants
// ---------------------------------------------------------------------------

/// Every parameter must satisfy `MIN <= DEFAULT <= MAX`. A default outside its
/// own declared bounds would be unreachable-by-validation and unsafe.
#[test]
fn defaults_sit_within_declared_bounds() {
    assert!(MIN_PROTOCOL_FEE_BPS <= DEFAULT_PROTOCOL_FEE_BPS);
    assert!(DEFAULT_PROTOCOL_FEE_BPS <= MAX_PROTOCOL_FEE_BPS);

    assert!(MIN_ATTESTATION_FEE_BPS <= DEFAULT_ATTESTATION_FEE_BPS);
    assert!(DEFAULT_ATTESTATION_FEE_BPS <= MAX_ATTESTATION_FEE_BPS);

    assert!(MIN_WITHDRAWAL_COOLDOWN_SECS <= DEFAULT_WITHDRAWAL_COOLDOWN_SECS);
    assert!(DEFAULT_WITHDRAWAL_COOLDOWN_SECS <= MAX_WITHDRAWAL_COOLDOWN_SECS);

    assert!(MIN_SLASH_COOLDOWN_SECS <= DEFAULT_SLASH_COOLDOWN_SECS);
    assert!(DEFAULT_SLASH_COOLDOWN_SECS <= MAX_SLASH_COOLDOWN_SECS);

    assert!(MIN_BRONZE_THRESHOLD <= DEFAULT_BRONZE_THRESHOLD);
    assert!(DEFAULT_BRONZE_THRESHOLD <= MAX_BRONZE_THRESHOLD);

    assert!(MIN_SILVER_THRESHOLD <= DEFAULT_SILVER_THRESHOLD);
    assert!(DEFAULT_SILVER_THRESHOLD <= MAX_SILVER_THRESHOLD);

    assert!(MIN_GOLD_THRESHOLD <= DEFAULT_GOLD_THRESHOLD);
    assert!(DEFAULT_GOLD_THRESHOLD <= MAX_GOLD_THRESHOLD);

    assert!(MIN_PLATINUM_THRESHOLD <= DEFAULT_PLATINUM_THRESHOLD);
    assert!(DEFAULT_PLATINUM_THRESHOLD <= MAX_PLATINUM_THRESHOLD);

    assert!(MIN_MAX_LEVERAGE <= DEFAULT_MAX_LEVERAGE);
    assert!(DEFAULT_MAX_LEVERAGE <= MAX_MAX_LEVERAGE);
}

/// `MIN_{tier i+1} == DEFAULT_{tier i}` for all four tiers. This identity is what
/// makes the ladder contiguous: each tier's floor equals the tier below's
/// default, so there is neither a gap nor an overlap at the defaults.
#[test]
fn tier_floor_equals_next_tier_default() {
    assert_eq!(MIN_SILVER_THRESHOLD, DEFAULT_BRONZE_THRESHOLD);
    assert_eq!(MIN_GOLD_THRESHOLD, DEFAULT_SILVER_THRESHOLD);
    assert_eq!(MIN_PLATINUM_THRESHOLD, DEFAULT_GOLD_THRESHOLD);
}

/// All four tier bands must be strictly ordered with non-overlapping ranges.
#[test]
fn tier_bands_are_strictly_ordered() {
    assert!(MIN_BRONZE_THRESHOLD < MIN_SILVER_THRESHOLD);
    assert!(MIN_SILVER_THRESHOLD < MIN_GOLD_THRESHOLD);
    assert!(MIN_GOLD_THRESHOLD < MIN_PLATINUM_THRESHOLD);

    assert!(MAX_BRONZE_THRESHOLD < MAX_SILVER_THRESHOLD);
    assert!(MAX_SILVER_THRESHOLD < MAX_GOLD_THRESHOLD);
    assert!(MAX_GOLD_THRESHOLD < MAX_PLATINUM_THRESHOLD);
}

/// A fresh contract must return every documented default before any write.
#[test]
fn uninitialised_parameters_return_documented_defaults() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    assert_eq!(client.get_protocol_fee_bps(), DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(
        client.get_attestation_fee_bps(),
        DEFAULT_ATTESTATION_FEE_BPS
    );
    assert_eq!(
        client.get_withdrawal_cooldown_secs(),
        DEFAULT_WITHDRAWAL_COOLDOWN_SECS
    );
    assert_eq!(
        client.get_slash_cooldown_secs(),
        DEFAULT_SLASH_COOLDOWN_SECS
    );
    assert_eq!(client.get_bronze_threshold(), DEFAULT_BRONZE_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), DEFAULT_SILVER_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), DEFAULT_GOLD_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), DEFAULT_PLATINUM_THRESHOLD);
    assert_eq!(client.get_max_leverage(), DEFAULT_MAX_LEVERAGE);
}

/// The cooldown maxima are derived from shared time constants; pin the
/// relationship so a change to one without the other is caught.
#[test]
fn cooldown_bounds_derive_from_shared_time_constants() {
    assert_eq!(MAX_SLASH_COOLDOWN_SECS, credence_math::SECONDS_PER_WEEK);
    assert_eq!(DEFAULT_SLASH_COOLDOWN_SECS, credence_math::SECONDS_PER_DAY);
    assert_eq!(
        DEFAULT_WITHDRAWAL_COOLDOWN_SECS,
        credence_math::SECONDS_PER_WEEK
    );
    assert_eq!(
        MAX_WITHDRAWAL_COOLDOWN_SECS,
        30 * credence_math::SECONDS_PER_DAY
    );
}

/// The 10% protocol-fee ceiling must stay strictly below a full 100% so a
/// governance mistake can never set a fee that confiscates the full bond.
#[test]
fn protocol_fee_ceiling_is_strictly_below_full_bps_denominator() {
    assert!(MAX_PROTOCOL_FEE_BPS < credence_math::BPS_DENOMINATOR as u32);
    assert!(MAX_ATTESTATION_FEE_BPS < credence_math::BPS_DENOMINATOR as u32);
}

/// `MAX_MAX_LEVERAGE` must be a real ceiling: strictly positive and above the
/// `MAX_LEVERAGE_FLOOR` default, so governance can only move it within a
/// meaningful range.
#[test]
fn max_leverage_ceiling_is_above_the_floor_default() {
    assert!(MAX_MAX_LEVERAGE > 0);
    assert!(MAX_MAX_LEVERAGE > MIN_MAX_LEVERAGE);
}

/// `MAX_QUERY_LIMIT` is documented to match the liquidation scanner's hard
/// iteration cap so all collection-read caps stay consistent. The scanner module
/// is not declared in the crate tree (its two test modules are also orphans), so
/// the shared value is asserted directly against the documented 200.
#[test]
fn query_limit_matches_documented_scanner_cap() {
    assert_eq!(MAX_QUERY_LIMIT, 200);
}

/// `DEFAULT_CHUNK_SIZE` must stay non-zero: a zero chunk would either panic or
/// loop forever in `vec_chunks`.
#[test]
fn default_chunk_size_is_non_zero() {
    assert!(credence_bond::parameters::DEFAULT_CHUNK_SIZE > 0);
    assert!(credence_bond::parameters::DEFAULT_CHUNK_SIZE as usize <= MAX_QUERY_LIMIT as usize);
}

// ---------------------------------------------------------------------------
// B9: Borrow-freeze boolean has no numeric bounds but does have an ordering
//     boundary against the other `risk`-category parameter.
// ---------------------------------------------------------------------------

/// Setting the freeze flag is idempotent: repeating the same value must not
/// change observable state. `risk` is the category `max_leverage` also uses,
/// so this doubles as a category-collision boundary.
#[test]
fn borrow_freeze_round_trips_and_is_idempotent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    assert!(!client.is_borrow_frozen());

    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());
    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());

    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
}

/// A freeze approval envelope must still match the `risk` category; the module
/// entrypoint `set_borrow_frozen_with_approval` is reachable only inside the
/// contract frame, so invoke it through `as_contract`.
///
/// This asserts the rejection only. Checking that the freeze flag stayed clear
/// afterwards is not possible on the same `Env`: a contract panic leaves the
/// frame in an error state that refuses further contract calls
/// (`Error(Context, InvalidAction)`, "Contract re-entry is not allowed"). The
/// state-preservation guarantee is covered on the `try_*` path instead, in
/// `parameters_recovery.rs`.
#[test]
#[should_panic(expected = "governance approval category mismatch")]
fn borrow_freeze_with_approval_rejects_wrong_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    let admin_clone = admin.clone();
    e.as_contract(&client.address, || {
        set_borrow_frozen_with_approval(&e, &admin_clone, true, &approval);
    });
}

/// The same envelope with the correct `risk` category is accepted and takes
/// effect, proving the previous rejection was the category and not the frame.
#[test]
fn borrow_freeze_with_approval_accepts_risk_category() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("risk"),
    };
    let admin_clone = admin.clone();
    e.as_contract(&client.address, || {
        set_borrow_frozen_with_approval(&e, &admin_clone, true, &approval);
    });
    assert!(client.is_borrow_frozen());
}

/// `require_not_borrow_frozen` must surface the dedicated `BorrowFrozen`
/// contract error code (114) and never a generic contract error. The path uses
/// `panic_with_error!`, so the host reports `Error(Contract, #114)` rather than
/// a text message.
#[test]
fn require_not_borrow_frozen_surfaces_borrow_frozen_code() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_borrow_frozen(&admin, &true);

    let msg = panic_message(|| {
        e.as_contract(&client.address, || {
            require_not_borrow_frozen(&e);
        });
    });
    assert!(
        msg.starts_with(&format!(
            "HostError: Error(Contract, #{BORROW_FROZEN_ERROR_CODE})"
        )),
        "expected the dedicated BorrowFrozen code, got: {msg}"
    );
}

/// The gate must be a no-op while the contract is unfrozen.
#[test]
fn require_not_borrow_frozen_passes_when_unfrozen() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    e.as_contract(&client.address, || {
        require_not_borrow_frozen(&e);
    });
}

/// A category symbol that is well-formed but meaningless must still be
/// rejected rather than silently matching a permissive default.
#[test]
fn unknown_category_symbol_is_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: Symbol::new(&e, "not_a_category"),
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_protocol_fee_bps_appr(&admin, &250, &approval);
    });
}
