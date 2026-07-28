use crate::IdentityBond;

pub fn is_period_ended(now: u64, bond_start: u64, bond_duration: u64) -> bool {
    let end = bond_start.checked_add(bond_duration).expect("overflow");
    now >= end
}

pub fn apply_renewal(bond: &mut IdentityBond, now: u64) {
    bond.bond_start = now;
    bond.withdrawal_requested_at = 0;
}
