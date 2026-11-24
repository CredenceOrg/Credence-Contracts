use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

/// Test successful bond creation with valid parameters
#[test]
fn test_create_bond_success() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let amount = 1000_i128;
    let duration = credence_math::SECONDS_PER_DAY;

    let bond = client.create_bond(&identity, &amount, &duration, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, amount);
    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bond_duration, duration);
}

/// Test bond creation with zero amount.
///
/// Updated for current behaviour: `create_bond` now runs
/// `validation::validate_bond_amount`, so a zero amount is below the test-mode
/// minimum (`validation::MIN_BOND_AMOUNT`) and is rejected. Previously this
/// test asserted the (now removed) "no validation exists" behaviour.
#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &0_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
}

/// Test bond creation with negative amount.
///
/// Updated for current behaviour: `validate_bond_amount` rejects negative
/// amounts outright ("bond amount cannot be negative").
#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_create_bond_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &(-100_i128),
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
}

/// Test bond creation with an amount above the accepted maximum.
///
/// Updated for current behaviour: `i128::MAX` exceeds
/// `validation::MAX_BOND_AMOUNT` and is rejected rather than stored.
#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_create_bond_max_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let max_amount = i128::MAX;
    client.create_bond(
        &identity,
        &max_amount,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
}

/// The `validation::MAX_BOND_AMOUNT` constant is unreachable in practice.
///
/// Updated for current behaviour: `create_bond` enforces both
/// `validate_bond_amount` (> max) and `validate_leverage`. Because
/// `MAX_BOND_AMOUNT / MIN_BOND_AMOUNT` (1e14 / 1e3 = 1e11) exceeds the
/// absolute leverage ceiling (`parameters::MAX_MAX_LEVERAGE` = 1e8), a bond at
/// the notional maximum trips `LeverageExceeded` (212) first. This test now
/// pins that observed ordering instead of asserting storage succeeds.
#[test]
#[should_panic(expected = "#212")]
fn test_create_bond_at_max_amount_succeeds() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);
    client.set_max_leverage(&admin, &parameters::MAX_MAX_LEVERAGE);

    let identity = Address::generate(&e);
    let amount = validation::MAX_BOND_AMOUNT;
    client.create_bond(
        &identity,
        &amount,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
}

/// Test bond creation with zero duration
#[test]
fn test_create_bond_zero_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1000_i128, &0_u64, &false, &0_u64);

    assert_eq!(bond.bond_duration, 0);
    assert!(bond.active);
}

/// Test bond creation with maximum duration that doesn't overflow
#[test]
fn test_create_bond_max_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let duration = u64::MAX / 2; // Safe duration that won't overflow with typical timestamps
    let bond = client.create_bond(&identity, &1000_i128, &duration, &false, &0_u64);

    assert_eq!(bond.bond_duration, duration);
}

/// Test bond creation with duration that causes timestamp overflow
#[test]
#[should_panic(expected = "bond end timestamp would overflow")]
fn test_create_bond_duration_overflow() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 1000; // Set timestamp close to max
    });

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let duration = 2000_u64; // Will overflow when added to timestamp
    client.create_bond(&identity, &1000_i128, &duration, &false, &0_u64);
}

/// Test duplicate bond creation is rejected.
///
/// Updated for current behaviour: a second `create_bond` for the same identity
/// now reverts with `ContractError::BondAlreadyExists` (218) instead of
/// silently overwriting the previous bond.
#[test]
#[should_panic(expected = "#218")]
fn test_create_bond_duplicate() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);

    // Create first bond
    let bond1 = client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    assert_eq!(bond1.bonded_amount, 1000);

    // A second bond for the same identity must be rejected.
    client.create_bond(&identity, &2000_i128, &172800_u64, &false, &0_u64);
}

/// Test bond creation with different identities (overwrites due to single bond storage)
#[test]
fn test_create_bond_different_identities() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity1 = Address::generate(&e);
    let identity2 = Address::generate(&e);

    client.create_bond(
        &identity1,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    let _bond2 = client.create_bond(&identity2, &2000_i128, &172800_u64, &false, &0_u64);

    // Due to single bond storage, only the last bond is stored
    let stored_bond = client.get_identity_state(&identity2);
    assert_eq!(stored_bond.identity, identity2);
    assert_eq!(stored_bond.bonded_amount, 2000);
}

/// Test bond creation initializes all fields correctly
#[test]
fn test_create_bond_field_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &5000_i128, &604800_u64, &false, &0_u64);

    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 5000);
    assert_eq!(bond.bond_duration, 604800);
    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);
}

/// Test bond creation persists to storage
#[test]
fn test_create_bond_storage_persistence() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let amount = 3000_i128;
    let duration = 259200_u64;

    client.create_bond(&identity, &amount, &duration, &false, &0_u64);

    let retrieved_bond = client.get_identity_state(&identity);
    assert_eq!(retrieved_bond.identity, identity);
    assert_eq!(retrieved_bond.bonded_amount, amount);
    assert_eq!(retrieved_bond.bond_duration, duration);
}

/// Test bond creation with minimum positive amount
#[test]
fn test_create_bond_min_positive_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let amount = validation::MIN_BOND_AMOUNT;
    let bond = client.create_bond(
        &identity,
        &amount,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// Test bond creation with typical USDC amount (6 decimals)
#[test]
fn test_create_bond_usdc_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);
    // Raise the leverage ceiling so a 1000 USDC bond is within the cap
    // (`validate_leverage` compares amount / MIN_BOND_AMOUNT against the cap).
    client.set_max_leverage(&admin, &parameters::MAX_MAX_LEVERAGE);

    let identity = Address::generate(&e);
    let usdc_amount = 1000_000000_i128; // 1000 USDC with 6 decimals
    let bond = client.create_bond(
        &identity,
        &usdc_amount,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    assert_eq!(bond.bonded_amount, usdc_amount);
}

/// Test bond_start timestamp is set correctly
#[test]
fn test_create_bond_timestamp() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);
    let bond = client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // bond_start should be set to ledger timestamp (can be 0 in test env)
    let ledger_time = e.ledger().timestamp();
    assert_eq!(bond.bond_start, ledger_time);
}

/// Test multiple sequential bond creations across distinct identities.
///
/// Updated for current behaviour: a bond can only be created once per identity
/// (duplicates revert with `BondAlreadyExists`), so this creates one bond per
/// identity instead of repeatedly overwriting a single identity's bond.
#[test]
fn test_create_bond_sequential() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let mut last_identity = Address::generate(&e);
    for i in 1..=5 {
        let amount = i * 1000;
        let identity = Address::generate(&e);
        let bond = client.create_bond(
            &identity,
            &amount,
            &credence_math::SECONDS_PER_DAY,
            &false,
            &0_u64,
        );
        assert_eq!(bond.bonded_amount, amount);
        last_identity = identity;
    }

    // The most recently created bond is stored under its own identity.
    let stored_bond = client.get_identity_state(&last_identity);
    assert_eq!(stored_bond.bonded_amount, 5000);
}

// ── lifecycle edge-cases (issue #284) ────────────────────────────────────────

/// bond_start is recorded at the ledger timestamp of creation, not at a later time.
#[test]
fn test_create_bond_bond_start_not_affected_by_later_time() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 500_000);
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);

    // Advance time — bond_start must still reflect creation time
    e.ledger().with_mut(|li| li.timestamp = 999_999);
    let b = client.get_identity_state(&identity);
    assert_eq!(
        b.bond_start, 500_000,
        "bond_start must be frozen at creation time"
    );
}

/// slashed_amount is always zero on a freshly created bond.
#[test]
fn test_create_bond_slashed_amount_zero_on_creation() {
    let e = Env::default();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &50_000_i128, &86_400_u64, &false, &0_u64);
    assert_eq!(bond.slashed_amount, 0);
}

/// withdrawal_requested_at is always zero on a freshly created bond.
#[test]
fn test_create_bond_withdrawal_requested_at_zero_on_creation() {
    let e = Env::default();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

/// A duration one second below the legacy minimum is accepted.
///
/// Updated for current behaviour: `create_bond` no longer calls
/// `validation::validate_bond_duration`, so the legacy 1-day/365-day
/// `validation::MIN_BOND_DURATION` / `MAX_BOND_DURATION` bounds are not
/// enforced at creation. The only duration guard in `create_bond` is the
/// `bond_start + duration` overflow check. `docs/bond-input-constraints.md`
/// documents the current required invariant as `duration > 0`; changing the
/// contract to enforce it is out of scope for this issue (no production
/// behaviour change), so this test now pins the observed behaviour.
#[test]
fn test_create_bond_one_below_min_duration_accepted() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    let duration = validation::MIN_BOND_DURATION - 1;
    let bond = client.create_bond(&identity, &1_000_i128, &duration, &false, &0_u64);
    assert_eq!(bond.bond_duration, duration);
}

/// A duration one second above the legacy maximum is accepted.
///
/// See `test_create_bond_one_below_min_duration_accepted`: the legacy
/// `validation::MAX_BOND_DURATION` bound is not enforced by `create_bond`.
#[test]
fn test_create_bond_one_above_max_duration_accepted() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    let duration = validation::MAX_BOND_DURATION + 1;
    let bond = client.create_bond(&identity, &1_000_i128, &duration, &false, &0_u64);
    assert_eq!(bond.bond_duration, duration);
}

/// Amount one below MIN is rejected.
#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_one_below_min_amount_rejected() {
    let e = Env::default();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &(validation::MIN_BOND_AMOUNT - 1),
        &86_400_u64,
        &false,
        &0_u64,
    );
}

/// Amount one above MAX is rejected.
#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_create_bond_one_above_max_amount_rejected() {
    let e = Env::default();
    let (client, _) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &(validation::MAX_BOND_AMOUNT + 1),
        &86_400_u64,
        &false,
        &0_u64,
    );
}

/// Phantom-balance mode (no token configured) keeps validating the raw amount
/// against the test bounds: a minimum-size bond still succeeds unchanged
/// (regression guard for issue #1612).
#[test]
fn phantom_mode_bounds_unchanged() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let alice = Address::generate(&e);
    let amount = validation::MIN_BOND_AMOUNT;

    let bond = client.create_bond(&alice, &amount, &86_400_u64, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, amount);
}
