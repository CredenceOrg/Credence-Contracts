use crate::IdentityBond;

/// Errors returned by rolling bond operations.
///
/// These are explicit, non-panicking failure modes so callers can
/// recover and report diagnosable errors without losing user data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollingBondError {
    /// `bond_start + bond_duration` overflows u64.
    DurationOverflow,
    /// `bond_duration` is zero, so the period can never end.
    ZeroDuration,
}

/// Returns the exclusive end timestamp of the bond period.
///
/// Uses checked arithmetic and returns an error instead of panicking
/// on overflow. A zero duration is rejected because it would make
/// `is_period_ended` always true and `bond_start == end`.
pub fn period_end(bond_start: u64, bond_duration: u64) -> Result<u64, RollingBondError> {
    if bond_duration == 0 {
        return Err(RollingBondError::ZeroDuration);
    }
    bond_start
        .checked_add(bond_duration)
        .ok_or(RollingBondError::DurationOverflow)
}

/// Returns true once `now` has reached or passed the bond period end.
///
/// Boundary behavior: the period is considered ended at the exact
/// end timestamp (inclusive). Timestamps before the end return false.
/// Invalid inputs (zero duration or overflow) return false rather than
/// panicking, so callers can recover deterministically.
pub fn is_period_ended(now: u64, bond_start: u64, bond_duration: u64) -> bool {
    match period_end(bond_start, bond_duration) {
        Ok(end) => now >= end,
        Err(_) => false,
    }
}

/// Applies a renewal to a bond.
///
/// The bond is restarted at `now` and any pending withdrawal request is
/// cleared. This is the only mutation path and it is idempotent with respect
/// to the resulting state for a given `now`.
///
/// Returns an error and leaves the bond unchanged when the renewal would
/// produce an invalid state (e.g. zero duration or overflowing end).
/// This guarantees partial failure cannot corrupt the bond.
pub fn apply_renewal(bond: &mut IdentityBond, now: u64) -> Result<(), RollingBondError> {
    // Validate the resulting period before mutating any state.
    period_end(now, bond.bond_duration)?;
    bond.bond_start = now;
    bond.withdrawal_requested_at = 0;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::IdentityBond;
    use soroban_sdk::testutils::Address as _;

    fn bond(bond_start: u64, bond_duration: u64, withdrawal_requested_at: u64) -> IdentityBond {
        // The identity is not read by any assertion in this module; a fixed
        // placeholder keeps the fixture independent of the test env.
        IdentityBond {
            identity: soroban_sdk::Address::generate(&soroban_sdk::Env::default()),

            bonded_amount: 0,
            bond_start,
            bond_duration,
            slashed_amount: 0,
            active: true,
            is_rolling: true,
            withdrawal_requested_at,
            ..Default::default()
        }
    }

    #[test]
    fn period_end_rejects_zero_duration() {
        assert_eq!(period_end(10, 0), Err(RollingBondError::ZeroDuration));
    }

    #[test]
    fn period_end_rejects_overflow() {
        assert_eq!(
            period_end(u64::MAX - 1, 2),
            Err(RollingBondError::DurationOverflow)
        );
    }

    #[test]
    fn period_end_exact_boundary() {
        assert_eq!(period_end(0, u64::MAX), Ok(u64::MAX));
    }

    #[test]
    fn is_period_ended_boundaries() {
        // Before end.
        assert_eq!(is_period_ended(99, 100, 100), false);
        // Exact end is inclusive.
        assert_eq!(is_period_ended(200, 100, 100), true);
        // After end.
        assert_eq!(is_period_ended(201, 100, 100), true);
    }

    #[test]
    fn is_period_ended_invalid_inputs_return_false() {
        assert_eq!(is_period_ended(u64::MAX, 0, 0), false);
        assert_eq!(is_period_ended(u64::MAX, u64::MAX - 1, 2), false);
    }

    #[test]
    fn apply_renewal_resets_state() {
        let mut b: IdentityBond = bond(100, 100, 150);
        assert!(apply_renewal(&mut b, 250).is_ok());
        assert_eq!(b.bond_start, 250);
        assert_eq!(b.withdrawal_requested_at, 0);
    }

    #[test]
    fn apply_renewal_is_idempotent() {
        let mut b: IdentityBond = bond(100, 100, 150);
        assert!(apply_renewal(&mut b, 250).is_ok());
        let first = b.clone();
        assert!(apply_renewal(&mut b, 250).is_ok());
        assert_eq!(b.bond_start, first.bond_start);
        assert_eq!(b.withdrawal_requested_at, first.withdrawal_requested_at);
    }

    #[test]
    fn apply_renewal_rejects_zero_duration_and_preserves_state() {
        let mut b: IdentityBond = bond(100, 0, 150);
        assert_eq!(
            apply_renewal(&mut b, 250),
            Err(RollingBondError::ZeroDuration)
        );
        // State must be unchanged on failure.
        assert_eq!(b.bond_start, 100);
        assert_eq!(b.withdrawal_requested_at, 150);
    }

    #[test]
    fn apply_renewal_rejects_overflow_and_preserves_state() {
        let mut b: IdentityBond = bond(100, 2, 150);
        assert_eq!(
            apply_renewal(&mut b, u64::MAX),
            Err(RollingBondError::DurationOverflow)
        );
        assert_eq!(b.bond_start, 100);
        assert_eq!(b.withdrawal_requested_at, 150);
    }

    #[test]
    fn apply_renewal_at_max_boundary() {
        // now + duration == u64::MAX is allowed.
        let mut b: IdentityBond = bond(100, 1, 150);
        assert!(apply_renewal(&mut b, u64::MAX - 1).is_ok());
        assert_eq!(b.bond_start, u64::MAX - 1);
        assert_eq!(b.withdrawal_requested_at, 0);
    }

    #[test]
    fn renewal_recovery_after_failure() {
        // A failed renewal must not block a later valid renewal.
        let mut b: IdentityBond = bond(100, 2, 150);
        assert!(apply_renewal(&mut b, u64::MAX).is_err());
        assert!(apply_renewal(&mut b, 500).is_ok());
        assert_eq!(b.bond_start, 500);
        assert_eq!(b.withdrawal_requested_at, 0);
    }
}
