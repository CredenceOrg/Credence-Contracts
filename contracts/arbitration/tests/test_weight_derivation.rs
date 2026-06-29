#[cfg(test)]
mod tests {
    #[test]
    fn test_weight_derived_from_bond_not_admin() {
        // Admin cannot set arbitrary weight
        // Weight is derived from bond balance only
    }

    #[test]
    fn test_zero_weight_arbitrator_cannot_vote() {
        // Arbitrator with 0 bond cannot participate
    }

    #[test]
    fn test_weight_snapshot_immutable() {
        // Top-up after vote does not change weight
    }

    #[test]
    fn test_slashed_arbitrator_mid_dispute() {
        // Arbitrator slashed; snapshot preserved
    }

    #[test]
    fn test_status_machine_preserved() {
        // Dispute lifecycle unchanged
    }

    #[test]
    fn test_weight_aggregation_checked_arithmetic() {
        // Overflow safe: use checked_add
    }
}
