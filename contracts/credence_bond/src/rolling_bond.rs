use crate::IdentityBond;

pub fn is_period_ended(now: u64, bond_start: u64, bond_duration: u64) -> bool {
    bond_start
        .checked_add(bond_duration)
        .is_some_and(|end| now >= end)
}

pub fn apply_renewal(bond: &mut IdentityBond, now: u64) {
    bond.bond_start = now;
}
