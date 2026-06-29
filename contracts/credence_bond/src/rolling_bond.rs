use crate::IdentityBond;

pub fn is_period_ended(now: u64, bond_start: u64, bond_duration: u64) -> bool {
    bond_start
        .checked_add(bond_duration)
        .is_some_and(|end| now >= end)
}

pub fn apply_renewal(bond: &mut IdentityBond, now: u64) {
    bond.bond_start = now;
//! Rolling Bond Type
//!
//! Auto-renews at period end unless withdrawal was requested with notice.
//! Tracks withdrawal request and notice period for scoring.

use crate::IdentityBond;

pub fn is_period_ended(now: u64, bond_start: u64, bond_duration: u64) -> bool {
    let end = bond_start.checked_add(bond_duration).expect("overflow");
    now >= end
}

/// Returns true if a withdrawal was requested and the notice period has elapsed.
#[must_use]
#[allow(dead_code)]
pub fn can_withdraw_after_notice(
    now: u64,
    withdrawal_requested_at: u64,
    notice_period_duration: u64,
) -> bool {
    if withdrawal_requested_at == 0 {
        return false;
    }
    let notice_end = withdrawal_requested_at.saturating_add(notice_period_duration);
    now >= notice_end
}

/// Advance bond to a new period (set bond_start to now, keep duration and rolling flag).
/// Call when period has ended and bond is rolling.
pub fn apply_renewal(bond: &mut IdentityBond, new_start: u64) {
    bond.bond_start = new_start;
    bond.withdrawal_requested_at = 0;
}

/// Returns the period end timestamp (start + duration) using saturating add.
#[must_use]
#[allow(dead_code)]
pub fn period_end(start: u64, duration: u64) -> u64 {
    start.saturating_add(duration)
}
