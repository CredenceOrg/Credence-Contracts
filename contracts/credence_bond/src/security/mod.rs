//! Boundary and recovery coverage for security-sensitive bond state.

#[cfg(test)]
mod recovery_tests {
    use super::super::test_helpers;
    use soroban_sdk::Env;

    #[test]
    fn duplicate_creation_failure_preserves_the_existing_bond() {
        let env = Env::default();
        let (client, _admin, identity, ..) = test_helpers::setup_with_token(&env);
        let duration = credence_math::SECONDS_PER_DAY;

        // Amounts must be >= `validation::MIN_BOND_AMOUNT` (1000 in test mode)
        // for the initial creation to succeed; the retry differs so the revert
        // is unambiguously the duplicate-bond guard.
        let original = client.create_bond(&identity, &1_000, &duration, &false, &0);
        let retry = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_bond(&identity, &1_700, &duration, &false, &0);
        }));
        assert!(retry.is_err(), "duplicate creation must be rejected");

        let recovered = client.get_identity_state(&identity);
        assert_eq!(recovered.bonded_amount, original.bonded_amount);
        assert_eq!(recovered.bond_start, original.bond_start);
        assert_eq!(recovered.bond_duration, original.bond_duration);
        assert!(recovered.active);
    }
}
