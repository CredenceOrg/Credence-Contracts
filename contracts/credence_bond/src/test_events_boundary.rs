//! Boundary and edge-case tests for event emissions.
//!
//! This module validates that events handle:
//! - Numeric boundary conditions (zero, max values, overflow protection)
//! - Invalid/malformed inputs
//! - Empty and null values
//! - Large collections in event data
//!
//! Intent: Ensure events are deterministic and safe under adverse input conditions.
#![cfg(test)]

use crate::events;
use soroban_sdk::{
    testutils::{Address as TestAddress, Events},
    Address, Env, String, Symbol,
};

/// Helper to verify event was emitted exactly once and return it.
fn assert_single_event(events: &soroban_sdk::Vec<soroban_sdk::ContractEvent>) -> &soroban_sdk::ContractEvent {
    assert_eq!(events.len(), 1, "Expected exactly one event");
    &events[0]
}

mod bond_lifecycle_boundary {
    use super::*;

    #[test]
    fn emit_bond_created_v2_with_zero_amount() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Zero amount should not panic; events emit the value as-is
        events::emit_bond_created_v2(&e, &addr, 0i128, 3600u64, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_with_max_amount() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();
        let max_amount = i128::MAX / 2; // Use a large but reasonable value

        events::emit_bond_created_v2(&e, &addr, max_amount, 3600u64, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_with_negative_amount() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Negative amounts should emit without panic (validation happens at contract level)
        events::emit_bond_created_v2(&e, &addr, -100i128, 3600u64, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_with_zero_duration() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Zero-duration bond (unusual but should emit)
        events::emit_bond_created_v2(&e, &addr, 1000i128, 0u64, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_with_max_duration() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Max duration (many years)
        events::emit_bond_created_v2(&e, &addr, 1000i128, u64::MAX, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_with_zero_timestamp() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);

        // Timestamp = 0 (genesis time, valid Soroban state)
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, 0u64);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_created_v2_rolling_and_fixed() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Both rolling states should emit without error
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, true, timestamp);
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, timestamp);
        let events = e.events().all();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn emit_bond_increased_v2_with_zero_added_amount() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_increased_v2(
            &e,
            &addr,
            0i128,          // added_amount = 0
            5000i128,       // new_total
            timestamp,
            false,
            crate::BondTier::Bronze,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_increased_v2_added_exceeds_total() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Malformed: added > total (validation fails at contract; event emits as-is)
        events::emit_bond_increased_v2(
            &e,
            &addr,
            10000i128,      // added_amount > new_total
            5000i128,       // new_total
            timestamp,
            false,
            crate::BondTier::Bronze,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_increased_v2_all_tier_transitions() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        let tiers = vec![
            crate::BondTier::Bronze,
            crate::BondTier::Silver,
            crate::BondTier::Gold,
            crate::BondTier::Platinum,
        ];

        for tier in tiers {
            events::emit_bond_increased_v2(
                &e,
                &addr,
                1000i128,
                10000i128,
                timestamp,
                true,
                tier,
            );
        }

        let events = e.events().all();
        assert_eq!(events.len(), 4);
    }

    #[test]
    fn emit_bond_withdrawn_v2_with_zero_withdrawn() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // No withdrawal (edge case)
        events::emit_bond_withdrawn_v2(
            &e,
            &addr,
            0i128,      // amount_withdrawn = 0
            5000i128,   // remaining
            timestamp,
            false,
            0i128,      // no penalty
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_withdrawn_v2_full_withdrawal() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Full withdrawal
        events::emit_bond_withdrawn_v2(
            &e,
            &addr,
            5000i128,   // amount_withdrawn = full
            0i128,      // remaining = 0
            timestamp,
            false,
            0i128,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_withdrawn_v2_with_large_penalty() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Early withdrawal with large penalty
        events::emit_bond_withdrawn_v2(
            &e,
            &addr,
            5000i128,
            1000i128,
            timestamp,
            true,
            4000i128,   // penalty = most of withdrawal
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_slashed_v2_with_zero_slash() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_slashed_v2(
            &e,
            &addr,
            0i128,          // slash_amount = 0
            0i128,          // total_slashed = 0
            timestamp,
            &admin,
            String::from_str(&e, ""),
            false,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_slashed_v2_full_slash() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Complete liquidation
        events::emit_bond_slashed_v2(
            &e,
            &addr,
            10000i128,      // slash_amount = full bond
            10000i128,      // total_slashed = cumulative
            timestamp,
            &admin,
            String::from_str(&e, "malicious_behavior"),
            true,           // is_full_slash
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_slashed_v2_with_empty_reason() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_slashed_v2(
            &e,
            &addr,
            100i128,
            100i128,
            timestamp,
            &admin,
            String::from_str(&e, ""),  // empty reason
            false,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_liquidated_with_zero_residual() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_liquidated(
            &e,
            &addr,
            0i128,  // No residual (fully slashed)
            Symbol::new(&e, "fully_slashed"),
            timestamp,
            &admin,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_liquidated_with_large_residual() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_liquidated(
            &e,
            &addr,
            i128::MAX / 2,  // Large residual
            Symbol::new(&e, "expired_unrenewed"),
            timestamp,
            &admin,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod tier_boundary {
    use super::*;

    #[test]
    fn emit_tier_changed_v2_all_transitions() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        let tiers = vec![
            crate::BondTier::Bronze,
            crate::BondTier::Silver,
            crate::BondTier::Gold,
            crate::BondTier::Platinum,
        ];

        // Test all possible transitions
        for old_tier in &tiers {
            for new_tier in &tiers {
                events::emit_tier_changed_v2(
                    &e,
                    &addr,
                    old_tier.clone(),
                    new_tier.clone(),
                    timestamp,
                );
            }
        }

        let events = e.events().all();
        assert_eq!(events.len(), 16); // 4 x 4 transitions
    }

    #[test]
    fn emit_tier_changed_same_tier() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Same tier "transition" (edge case, shouldn't happen but should emit)
        events::emit_tier_changed_v2(
            &e,
            &addr,
            crate::BondTier::Silver,
            crate::BondTier::Silver,
            timestamp,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod claims_boundary {
    use super::*;

    #[test]
    fn emit_claim_added_with_zero_amount() {
        let e = Env::default();
        let user = TestAddress::generate(&e);

        let claim = crate::claims::PendingClaim {
            claim_type: crate::claims::ClaimType::TierReward,
            amount: 0i128,
            source_id: 12345u64,
        };

        events::emit_claim_added(&e, &user, &claim);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_claim_added_with_large_amount() {
        let e = Env::default();
        let user = TestAddress::generate(&e);

        let claim = crate::claims::PendingClaim {
            claim_type: crate::claims::ClaimType::FeatureBonus,
            amount: i128::MAX / 2,
            source_id: u64::MAX,
        };

        events::emit_claim_added(&e, &user, &claim);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_claims_processed_with_zero_count() {
        let e = Env::default();
        let user = TestAddress::generate(&e);

        let result = crate::claims::ClaimResult {
            processed_count: 0,
            total_amount: 0i128,
            claim_types: soroban_sdk::Vec::new(&e),
        };

        events::emit_claims_processed(&e, &user, &result, &soroban_sdk::Vec::new(&e));
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_claims_expired_with_zero_count() {
        let e = Env::default();
        let user = TestAddress::generate(&e);

        events::emit_claims_expired(&e, &user, 0, 0i128);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod admin_governance_boundary {
    use super::*;

    #[test]
    fn emit_upgrade_auth_initialized() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);

        events::emit_upgrade_auth_initialized(&e, &admin);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_upgrade_auth_granted_all_roles() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        let target = TestAddress::generate(&e);

        let roles = vec![
            crate::upgrade_auth::UpgradeRole::ProposalSubmitter,
            crate::upgrade_auth::UpgradeRole::Approver,
            crate::upgrade_auth::UpgradeRole::Executor,
        ];

        for role in roles {
            events::emit_upgrade_auth_granted(&e, &admin, &target, role);
        }

        let events = e.events().all();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn emit_admin_transfer_started() {
        let e = Env::default();
        let current_admin = TestAddress::generate(&e);
        let pending_admin = TestAddress::generate(&e);

        events::emit_admin_transfer_started(&e, &current_admin, &pending_admin);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_admin_transfer_completed() {
        let e = Env::default();
        let old_admin = TestAddress::generate(&e);
        let new_admin = TestAddress::generate(&e);

        events::emit_admin_transfer_completed(&e, &old_admin, &new_admin);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_parameter_updated_with_zero_values() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);

        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "fee_prot"),
            Symbol::new(&e, "fee"),
            &admin,
            0i128,
            0i128,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_parameter_updated_large_values() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);

        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "max_lev"),
            Symbol::new(&e, "risk"),
            &admin,
            i128::MIN / 2,
            i128::MAX / 2,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_fee_config_updated_with_none_old_treasury() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        let new_treasury = TestAddress::generate(&e);

        events::emit_fee_config_updated(&e, &admin, None, &new_treasury, 0u32, 500u32);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_fee_config_updated_with_max_fee_bps() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        let old_treasury = TestAddress::generate(&e);
        let new_treasury = TestAddress::generate(&e);

        // Max fee is 1000 bps (10%)
        events::emit_fee_config_updated(&e, &admin, Some(old_treasury), &new_treasury, 0u32, 1000u32);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod audit_boundary {
    use super::*;

    #[test]
    fn emit_bond_drift_detected() {
        let e = Env::default();
        let subject = TestAddress::generate(&e);

        let details = crate::invariants::BondDriftDetails {
            subject: subject.clone(),
            kind: crate::invariants::BondDriftKind::BondAmountMismatch,
            bonded_amount: 0i128,
            slashed_amount: 0i128,
            attestation_count: 0u32,
            attestation_list_len: 0u32,
        };

        events::emit_bond_drift_detected(&e, &details);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_bond_drift_detected_with_large_values() {
        let e = Env::default();
        let subject = TestAddress::generate(&e);

        let details = crate::invariants::BondDriftDetails {
            subject: subject.clone(),
            kind: crate::invariants::BondDriftKind::AttestationCountMismatch,
            bonded_amount: i128::MAX / 2,
            slashed_amount: i128::MAX / 4,
            attestation_count: u32::MAX,
            attestation_list_len: u32::MAX,
        };

        events::emit_bond_drift_detected(&e, &details);
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod address_boundary {
    use super::*;

    #[test]
    fn emit_with_same_identity_and_admin() {
        let e = Env::default();
        let same_addr = TestAddress::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Edge case: identity and admin are the same
        events::emit_bond_slashed_v2(
            &e,
            &same_addr,
            100i128,
            100i128,
            timestamp,
            &same_addr,
            String::from_str(&e, "test"),
            false,
        );
        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn emit_with_generated_addresses() {
        let e = Env::default();
        
        // Generate many unique addresses to test address diversity
        for _ in 0..5 {
            let addr = TestAddress::generate(&e);
            let timestamp = e.ledger().timestamp();
            events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, timestamp);
        }
        
        let events = e.events().all();
        assert_eq!(events.len(), 5);
    }
}
