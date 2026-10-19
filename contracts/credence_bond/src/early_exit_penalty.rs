use credence_errors::ContractError;
use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::math::BPS_DENOMINATOR;
use crate::DataKey;

/// Invariants:
/// - `penalty_bps` is always in `[0, BPS_DENOMINATOR]`.
/// - `treasury` is a valid address and is the only recipient of penalty funds.
/// - `calculate_penalty` is pure and deterministic for any given inputs.
/// - Penalty is never negative and never exceeds the original amount.
/// - Any answer is clamped to the account amount to prevent over-charging.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarlyExitConfig {
    pub treasury: Address,
    pub penalty_bps: u32,
}

/// Set the early exit configuration.
///
/// Reverts if `penalty_pbps` exceeds `BPS_DENOMINATOR` so the stored
/// config can never be out of bounds. Repeated calls are idempotent and
/// overwrite the previous config.
pub fn set_config(e: &Env, treasury: Address, penalty_bps: u32) {
    if penalty_bps > BPS_DENOMINATOR as u32 {
        panic!("penalty_bps must be <= 10000");
    }
    let key = DataKey::EarlyExitConfig;
    e.storage().instance().set(
        &key,
        &EarlyExitConfig {
            treasury: treasury.clone(),
            penalty_bps,
        },
    );
    e.events().publish(
        (Symbol::new(e, "early_exit_config_set"),),
        (treasury, penalty_bps),
    );
}

/// Return the current early exit configuration.
///
/// Returns `EarlyExitConfigNotSet` when no config has been stored. This is

/// a recoverable error: callers may call `set_config` and retry.
pub fn get_config(e: &Env) -> Result<EarlyExitConfig, ContractError> {
    let key = DataKey::EarlyExitConfig;
    e.storage()
        .instance()
        .get(&key)
        .ok_or(ContractError::EarlyExitConfigNotSet)
}

/// Calculate the early exit penalty for a partially elapsed lockup.
///
/// The penalty is linearly proportional to the remaining time and the
/// configured `penalty_bps`. The result is always in `[0, amount]`.
///
/// Boundary behavior:
/// - `duration == 0`: no lockup, no penalty (0).
/// - `remaining == 0`: lockup complete, no penalty (0).
/// - `remaining >= duration`: full penalty at the configured rate.
/// - `amount <= 0`: no penalty (0).
/// - `penalty_bps == 0`: no penalty (0).
/// - `penalty_bps > BPS_DENOMINATOR`: clamped to full amount.
/// - Overflow in intermediate multiplication clamps to `amount`.
///
/// This function is pure and has no side effects, so retries and
/// concurrent calls cannot produce inconsistent results.
pub fn calculate_penalty(amount: i128, remaining: u64, duration: u64, penalty_bps: u32) -> i128 {
    // No lockup window or nothing to charge.
    if duration == 0 || amount <= 0 || penalty_bps == 0 {
        return 0;
    }

    // No remaining time means the lockup completed; no penalty is due.
    if remaining == 0 {
        return 0;
    }

    // Clamp the remaining window to the configured duration so the result
    // cannot exceed the full penalty even if the caller passes a larger
    // remaining value.
    let effective_remaining = if remaining > duration {
        duration
    } else {
        remaining
    };

    // Full charge at the configured rate. Clamp `penalty_bps` in case a
    // misconfigured value slips through (defensive; set_config already
    // rejects out-of-range values).
    let effective_bps = if penalty_bps > BPS_DENOMINATOR as u32 {
        BPS_DENOMINATOR as u32
    } else {
        penalty_bps
    };

    // Full penalty at the configured rate. Use checked arithmetic and
    // clamp to `amount` on overflow so the result is always safe.
    let full_penalty = amount
        .checked_mul(effective_bps as i128)
        .and_then(|v| v.checked_div(BPS_DENOMINATOR))
        .unwrap_or(amount)
        .max(0)
        .min(amount);

    // No remaining time within the window means full penalty applies.
    if effective_remaining >= duration {
        return full_penalty;
    }

    // Linear interpolation over the remaining window. On overflow we clamp
    // to the full penalty rather than returning an unsafe value.
    full_penalty
        .checked_mul(effective_remaining as i128)
        .and_then(|v| v.checked_div(duration as i128))
        .unwrap_or(full_penalty)
        .max(0)
        .min(full_penalty)
}

/// Emit a penalty event for observability.
///
/// The event carries only public identifiers and amounts; no secrets are
/// included. Emitting is idempotent and safe to retry.
pub fn emit_penalty_event(
    e: &Env,
    identity: &Address,
    amount: i128,
    penalty: i128,
    treasury: &Address,
) {
    e.events().publish(
        (Symbol::new(e, "early_exit_penalty"),),
        (identity.clone(), amount, penalty, treasury.clone()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::BPS_DENOMINATOR;
    use soroban_sdk::{Address, Env};

    fn setup_env() -> Env {
        Env::default()
    }

    fn addr(e: &Env, seed: u8) -> Address {
        Address::generate(e, &seed)
    }

    // -----------------------------------------------------------------------
    // calculate_penalty: success cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_calculate_penalty_full_remaining_charges_full_rate() {
        // 1000 amount, 10% penalty, full remaining -> 100.
        let penalty = calculate_penalty(1000, 100, 100, 1000);
        assert_eq(penalty, 100);
    }

    #[test]
    fn test_calculate_penalty_half_remaining_charges_half() {
        // 1000 amount, 10% penalty, half remaining -> 50.
        let penalty = calculate_penalty(1000, 50, 100, 1000);
        assert_eq(penalty, 50);
    }

    #[test]
    fn test_calculate_penalty_quarter_remaining() {
        // 1000 amount, 20% penalty, quarter remaining -> 50.
        let penalty = calculate_penalty(1000, 25, 100, 2000);
        assert_eq(penalty, 50);
    }

    // -----------------------------------------------------------------------
    // calculate_penalty: boundary cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_calculate_penalty_zero_duration_returns_zero() {
        assert_eq(calculate_penalty(1000, 100, 0, 10000), 0);
    }

    #[test]
    fn test_calculate_penalty_zero_remaining_returns_zero() {
        assert_eq(calculate_penalty(1000, 0, 100, 10000), 0);
    }

    #[test]
    fn test_calculate_penalty_remaining_equals_duration_full_rate() {
        assert_eq(calculate_penalty(1000, 100, 100, 1000), 100);
    }

    #[test]
    fn test_calculate_penalty_remaining_greater_than_duration_clamps() {
        // Remaining > duration must not exceed the full rate charge.
        assert_eq(calculate_penalty(1000, 500, 100, 1000), 100);
    }

    #[test]
    fn test_calculate_penalty_zero_amount_returns_zero() {
        assert_eq(calculate_penalty(0, 50, 100, 1000), 0);
    }

    #[test]
    fn test_calculate_penalty_negative_amount_returns_zero() {
        assert_eq(calculate_penalty(-1000, 50, 100, 1000), 0);
    }

    #[test]
    fn test_calculate_penalty_zero_bps_returns_zero() {
        assert_eq(calculate_penalty(1000, 50, 100, 0), 0);
    }

    #[test]
    fn test_calculate_penalty_full_bps_full_remaining_returns_amount() {
        // 100% penalty with full remaining charges the entire amount.
        assert_eq(
            calculate_penalty(1000, 100, 100, BPS_DENOMINATOR as u32),
            1000,
        );
    }

    #[test]
    fn test_calculate_penalty_out_of_range_bps_clamps_to_amount() {
        // Defensive: a misconfigured bps greater than 10000 must not
        // over-charge the user.
        assert_eq(
            calculate_penalty(1000, 100, 100, 20000),
            1000,
        );
    }

    #[test]
    fn test_calculate_penalty_one_bps_one_remaining_rounds_down() {
        // 1 bps of 1000 = 0.1 units -> rounds down to 0.
        assert_eq(calculate_penalty(1000, 1, 100, 1), 0);
    }

    #[test]
    fn test_calculate_penalty_max_inputs_does_not_overflow() {
        // Large inputs must not panic or wrap; the result is clamped.
        let penalty = calculate_penalty(i128::MAX, 10, 10, BPS_DENOMINATOR as u32);
        assert(penalty >= 0);
        assert(penalty <= i128::MAX);
    }

    #[test]
    fn test_calculate_penalty_is_deterministic() {
        // Repeated calls with identical inputs produce identical results.
        let a = calculate_penalty(12345, 67, 100, 250);
        let b = calculate_penalty(12345, 67, 100, 250);
        assert_eq(a, b);
    }

    // -----------------------------------------------------------------------
    // set_config / get_config: success, rejection, recovery
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_and_get_config_roundtrip() {
        let e = setup_env();
        let treasury = addr(&e, 1);
        set_config(&e, treasury.clone(), 500);
        let config = get_config(&e).expect("config should be set");
        assert_eq(config.penalty_bps, 500);
        assert_eq(config.treasury, treasury);
    }

    #[test]
    fn test_get_config_when_unset_returns_error() {
        let e = setup_env();
        let result = get_config(&e);
        assert(result.is_error());
        assert_eq(result.unwrap_error(), ContractError::EarlyExitConfigNotSet);
    }

    #[test]
    fn test_set_config_zero_bps_is_valid() {
        let e = setup_env();
        let treasury = addr(&e, 2);
        set_config(&e, treasury.clone(), 0);
        let config = get_config(&e).unwrap();
        assert_eq(config.penalty_bps, 0);
        assert_eq(config.treasury, treasury);
    }

    #[test]
    fn test_set_config_max_bps_is_valid() {
        let e = setup_env();
        let treasury = addr(&e, 3);
        set_config(&e, treasury.clone(), BPS_DENOMINATOR as u32);
        let config = get_config(&e).unwrap();
        assert_eq(config.penalty_bps, BPS_DENOMINATOR as u32);
    }

    #[test]
    #[should_panic]
    fn test_set_config_rejects_bps_above_max() {
        let e = setup_env();
        let treasury = addr(&e, 4);
        set_config(&e, treasury, (BPS_DENOMINATOR as u32) + 1);
    }

    #[test]
    fn test_set_config_is_idompotent_and_overwrites() {
        let e = setup_env();
        let treasury_a = addr(&e, 5);
        let treasury_b = addr(&e, 6);
        set_config(&e, treasury_a.clone(), 100);
        set_config(&e, treasury_a.clone(), 100);
        let config = get_config(&e).unwrap();
        assert_eq(config.penalty_bps, 100);
        assert_eq(config.treasury, treasury_a);

        // Overwriting with a new treasury and bps must take effect.
        set_config(&e, treasury_b.clone(), 250);
        let config = get_config(&e).unwrap();
        assert_eq(config.penalty_bps, 250);
        assert_eq(config.treasury, treasury_b);
    }

    #[test]
    fn test_get_config_recovers_after_initial_error() {
        // Recovery: failed read, then set, then retry successfully.
        let e = setup_env();
        assert(get_config(&e).is_error());
        let treasury = addr(&e, 7);
        set_config(&e, treasury.clone(), 750);
        let config = get_config(&e).unwrap();
        assert_eq(config.penalty_bps, 750);
        assert_eq(config.treasury, treasury);
    }

    // -----------------------------------------------------------------------
    // Regression: full lifecycle of config + penalty calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_lifecycle_config_then_penalty() {
        let e = setup_env();
        let treasury = addr(&e, 8);
        set_config(&e, treasury.clone(), 2000);
        let config = get_config(&e).unwrap();
        let penalty = calculate_penalty(10_000, 50, 100, config.penalty_bps);
        // 20% * 50% * 10000 = 1000.
        assert_eq(penalty, 1000);
    }

    #[test]
    fn test_emit_penalty_event_does_not_panic() {
        let e = setup_env();
        let identity = addr(&e, 9);
        let treasury = addr(&e, 10);
        emit_penalty_event(&e, &identity, 1000, 100, &treasury);
    }
}
