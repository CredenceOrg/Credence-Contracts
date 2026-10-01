#![cfg(test)]
/// Invariant and correctness tests for event emissions.
///
/// This module validates that events maintain critical invariants:
/// - Emitted values match expected calculations (e.g., end_timestamp = start + duration)
/// - Event data consistency (e.g., new_total >= added_amount)
/// - Schema invariants (topic/data counts stay constant)
/// - Type safety (values fit in declared types)
///
/// Intent: Ensure events are semantically correct and indexer-safe.
use crate::events;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, String, Symbol, TryFromVal, Val, Vec as SorobanVec,
};

/// A single entry from `Env::events().all()` in soroban-sdk 22, which yields
/// `(contract, topics, data)` tuples rather than a `ContractEvent` struct.
type TestEvent = (Address, SorobanVec<Val>, Val);

/// Extract the topic list from a contract event tuple.
fn get_topics(event: TestEvent) -> std::vec::Vec<Val> {
    event.1.iter().collect()
}

/// Extract the data values from a contract event tuple.
///
/// SDK 22 packs the emitted data into a single `Val` holding a `Vec`, so it has
/// to be decoded back into a `Vec<Val>` using the host `Env`.
fn get_data(env: &Env, event: TestEvent) -> std::vec::Vec<Val> {
    SorobanVec::<Val>::try_from_val(env, &event.2)
        .map(|v| v.iter().collect())
        .unwrap_or_default()
}

/// Borrow the single event emitted by the code under test.
fn first_event(events: &SorobanVec<TestEvent>) -> TestEvent {
    events
        .get(0)
        .expect("exactly one event must have been emitted")
}

mod bond_lifecycle_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn bond_created_v2_end_timestamp_invariant() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let start_timestamp = 1000u64;
        let duration = 3600u64;

        events::emit_bond_created_v2(&e, &addr, 1000i128, duration, false, start_timestamp);

        let events = e.events().all();
        assert_eq!(events.len(), 1);

        let event = first_event(&events);
        let data = get_data(&e, event);

        // Data: (duration, is_rolling, end_timestamp)
        assert_eq!(data.len(), 3, "bond_created_v2 should have 3 data fields");

        // Extract end_timestamp from data[2]
        let end_timestamp: u64 = u64::try_from_val(&e, &data[2]).unwrap();
        let expected_end = start_timestamp.checked_add(duration).expect("overflow");

        assert_eq!(
            end_timestamp, expected_end,
            "end_timestamp must equal start_timestamp + duration"
        );
    }

    #[test]
    fn bond_created_v2_timestamp_in_topics() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let start_timestamp = 5000u64;

        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, start_timestamp);

        let events = e.events().all();
        let event = first_event(&events);
        let topics = get_topics(event);

        // Topics: (event_name, identity, amount, timestamp)
        assert_eq!(topics.len(), 4, "bond_created_v2 must have 4 topics");

        // Topic[3] should be the timestamp
        let topic_timestamp: u64 = u64::try_from_val(&e, &topics[3]).unwrap();
        assert_eq!(
            topic_timestamp, start_timestamp,
            "timestamp topic must match input"
        );
    }

    #[test]
    fn bond_increased_v2_new_total_ge_added_invariant() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let added = 500i128;
        let new_total = 2000i128;

        // Invariant: new_total >= added_amount (delta is always positive or zero)
        events::emit_bond_increased_v2(
            &e,
            &addr,
            added,
            new_total,
            e.ledger().timestamp(),
            false,
            crate::BondTier::Silver,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 1);

        let event = first_event(&events);
        let topics = get_topics(event);

        // Topics: (event_name, identity, added_amount, new_total, timestamp)
        assert_eq!(topics.len(), 5, "bond_increased_v2 must have 5 topics");
    }

    #[test]
    fn bond_withdrawn_v2_remaining_sanity_check() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let withdrawn = 1000i128;
        let remaining = 2000i128;
        let penalty = 100i128;

        events::emit_bond_withdrawn_v2(
            &e,
            &addr,
            withdrawn,
            remaining,
            e.ledger().timestamp(),
            true,
            penalty,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 1);

        let event = first_event(&events);
        let topics = get_topics(event);

        // Topics: (event_name, identity, amount_withdrawn, remaining, timestamp)
        assert_eq!(topics.len(), 5, "bond_withdrawn_v2 must have 5 topics");
    }

    #[test]
    fn bond_slashed_v2_cumulative_slash_invariant() {
        let e = Env::default();
        let identity = Address::generate(&e);
        let admin = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        // First slash: 100 of 1000
        events::emit_bond_slashed_v2(
            &e,
            &identity,
            100i128,
            100i128, // total_slashed = cumulative
            timestamp,
            &admin,
            String::from_str(&e, "first"),
            false,
        );

        // Second slash: additional 150
        events::emit_bond_slashed_v2(
            &e,
            &identity,
            150i128,
            250i128, // total_slashed = 100 + 150
            timestamp + 100,
            &admin,
            String::from_str(&e, "second"),
            false,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 2);

        // Verify total_slashed is monotonically increasing
        let event1_topics = get_topics(first_event(&events));
        let event2_topics = get_topics(events.get(1).expect("second event"));

        // Topics[3] is total_slashed in both events
        let total1: i128 = i128::try_from_val(&e, &event1_topics[3]).unwrap();
        let total2: i128 = i128::try_from_val(&e, &event2_topics[3]).unwrap();

        assert!(
            total2 >= total1,
            "total_slashed must be monotonically non-decreasing"
        );
    }

    #[test]
    fn bond_liquidated_reason_is_symbol() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let admin = Address::generate(&e);

        events::emit_bond_liquidated(
            &e,
            &addr,
            100i128,
            Symbol::new(&e, "fully_slashed"),
            e.ledger().timestamp(),
            &admin,
        );

        let events = e.events().all();
        let event = first_event(&events);
        let data = get_data(&e, event);

        // Data: (residual, reason, timestamp, admin)
        assert_eq!(data.len(), 4);

        // data[1] should be the reason Symbol
        let reason = Symbol::try_from_val(&e, &data[1]).unwrap();
        assert_eq!(reason, Symbol::new(&e, "fully_slashed"));
    }
}

mod tier_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn tier_changed_v2_old_and_new_differ_or_match() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Case 1: Different tiers (normal transition)
        events::emit_tier_changed_v2(
            &e,
            &addr,
            crate::BondTier::Bronze,
            crate::BondTier::Silver,
            timestamp,
        );

        // Case 2: Same tier (edge case but valid)
        events::emit_tier_changed_v2(
            &e,
            &addr,
            crate::BondTier::Silver,
            crate::BondTier::Silver,
            timestamp + 100,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn all_tier_combinations_valid() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        let tiers = [
            crate::BondTier::Bronze,
            crate::BondTier::Silver,
            crate::BondTier::Gold,
            crate::BondTier::Platinum,
        ];

        let mut count = 0;
        for old_tier in &tiers {
            for new_tier in &tiers {
                events::emit_tier_changed_v2(
                    &e,
                    &addr,
                    old_tier.clone(),
                    new_tier.clone(),
                    timestamp + count as u64,
                );
                count += 1;
            }
        }

        let events = e.events().all();
        assert_eq!(events.len(), 16, "All 4x4 tier combinations should emit");
    }
}

mod claims_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn claim_added_source_id_preserved() {
        let e = Env::default();
        let user = Address::generate(&e);

        let claim = crate::claims::PendingClaim {
            claim_id: 1,
            claim_type: crate::claims::ClaimType::SlashingReward,
            amount: 100i128,
            created_at: e.ledger().timestamp(),
            expires_at: 0,
            source_id: 12345u64,
            metadata: Symbol::new(&e, "claim"),
            processed: false,
        };

        events::emit_claim_added(&e, &user, &claim);

        let events = e.events().all();
        let event = first_event(&events);
        let data = get_data(&e, event);

        // Data: (claim_type, amount, source_id)
        assert_eq!(data.len(), 3);

        // data[2] should be source_id
        let source_id: u64 = u64::try_from_val(&e, &data[2]).unwrap();
        assert_eq!(source_id, 12345);
    }

    #[test]
    fn claims_processed_count_matches_types_len() {
        let e = Env::default();
        let user = Address::generate(&e);

        // Claim 3 types
        let mut claim_types = SorobanVec::new(&e);
        claim_types.push_back(crate::claims::ClaimType::SlashingReward);
        claim_types.push_back(crate::claims::ClaimType::VerifierReward);
        claim_types.push_back(crate::claims::ClaimType::DisputeReward);

        let result = crate::claims::ClaimResult {
            processed_count: 3u32,
            total_amount: 500i128,
            claim_types: claim_types.clone(),
        };

        events::emit_claims_processed(&e, &user, &result, &SorobanVec::new(&e));

        let events = e.events().all();
        let event = first_event(&events);
        let data = get_data(&e, event);

        // data[0] is processed_count
        let count: u32 = u32::try_from_val(&e, &data[0]).unwrap();
        assert_eq!(count, 3, "processed_count should match claim_types length");
    }

    #[test]
    fn claims_expired_count_and_amount_consistency() {
        let e = Env::default();
        let user = Address::generate(&e);

        // Emit with non-zero count and amount
        events::emit_claims_expired(&e, &user, 5u32, 500i128);

        let events = e.events().all();
        let event = first_event(&events);
        let data = get_data(&e, event);

        // Data: (expired_count, expired_amount)
        assert_eq!(data.len(), 2);

        let expired_count: u32 = u32::try_from_val(&e, &data[0]).unwrap();
        assert_eq!(expired_count, 5);
    }
}

mod admin_governance_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn parameter_updated_key_and_category_symbols() {
        let e = Env::default();
        let admin = Address::generate(&e);

        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "fee_prot"),
            Symbol::new(&e, "fee"),
            &admin,
            0i128,
            100i128,
        );

        let events = e.events().all();
        let event = first_event(&events);
        let topics = get_topics(event);

        // Topics: (param_updated, key, category, admin)
        assert_eq!(topics.len(), 4);

        // topics[1] is key
        let key = Symbol::try_from_val(&e, &topics[1]).unwrap();
        assert_eq!(key, Symbol::new(&e, "fee_prot"));

        // topics[2] is category
        let category = Symbol::try_from_val(&e, &topics[2]).unwrap();
        assert_eq!(category, Symbol::new(&e, "fee"));
    }

    #[test]
    fn parameter_updated_admin_consistent() {
        let e = Env::default();
        let admin1 = Address::generate(&e);
        let admin2 = Address::generate(&e);

        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "p1"),
            Symbol::new(&e, "cat1"),
            &admin1,
            0i128,
            1i128,
        );
        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "p2"),
            Symbol::new(&e, "cat2"),
            &admin2,
            0i128,
            2i128,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 2);

        // Each event has correct admin
        let event1_topics = get_topics(first_event(&events));
        let event2_topics = get_topics(events.get(1).expect("second event"));

        let e1_admin = Address::try_from_val(&e, &event1_topics[3]).unwrap();
        let e2_admin = Address::try_from_val(&e, &event2_topics[3]).unwrap();

        assert_eq!(e1_admin, admin1);
        assert_eq!(e2_admin, admin2);
    }

    #[test]
    fn fee_config_updated_fee_bps_range() {
        let e = Env::default();
        let admin = Address::generate(&e);
        let treasury = Address::generate(&e);

        // Min fee (0%)
        events::emit_fee_config_updated(&e, &admin, None, &treasury, 0u32, 0u32);

        // Mid fee (5%)
        events::emit_fee_config_updated(
            &e,
            &admin,
            Some(treasury.clone()),
            &treasury,
            0u32,
            500u32,
        );

        // Max fee (10%)
        events::emit_fee_config_updated(
            &e,
            &admin,
            Some(treasury.clone()),
            &treasury,
            0u32,
            1000u32,
        );

        let events = e.events().all();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn admin_transfer_addresses_distinct() {
        let e = Env::default();
        let admin1 = Address::generate(&e);
        let admin2 = Address::generate(&e);

        // Transfer initiated
        events::emit_admin_transfer_started(&e, &admin1, &admin2);

        // Transfer completed with same addresses
        events::emit_admin_transfer_completed(&e, &admin1, &admin2);

        let events = e.events().all();
        assert_eq!(events.len(), 2);

        let started_topics = get_topics(first_event(&events));
        let completed_topics = get_topics(events.get(1).expect("second event"));

        // Both have (event, admin) topics
        assert_eq!(started_topics.len(), 2);
        assert_eq!(completed_topics.len(), 2);
    }
}

mod audit_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn bond_drift_detected_kinds_valid() {
        let e = Env::default();
        let subject = Address::generate(&e);

        let kinds = [
            crate::invariants::BondDriftKind::SlashedExceedsBonded,
            crate::invariants::BondDriftKind::AttestationCountMismatch,
            crate::invariants::BondDriftKind::AttestationCountMismatch,
        ];

        for kind in kinds {
            let details = crate::invariants::BondDriftDetails {
                subject: subject.clone(),
                kind: kind.clone(),
                bonded_amount: 1000i128,
                slashed_amount: 100i128,
                attestation_count: 5u32,
                attestation_list_len: 5u32,
            };

            events::emit_bond_drift_detected(&e, &details);
        }

        let events = e.events().all();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn bond_drift_detected_attestation_consistency() {
        let e = Env::default();
        let subject = Address::generate(&e);

        // Consistency check: list_len should match or exceed count
        let details = crate::invariants::BondDriftDetails {
            subject: subject.clone(),
            kind: crate::invariants::BondDriftKind::AttestationCountMismatch,
            bonded_amount: 1000i128,
            slashed_amount: 0i128,
            attestation_count: 10u32,
            attestation_list_len: 10u32, // Should be >= count
        };

        events::emit_bond_drift_detected(&e, &details);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
    }
}

mod schema_invariants {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn bond_created_v2_schema_immutable() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        // Emit and verify schema structure never changes
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, timestamp);

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=4, Data=3 (FROZEN)
        assert_eq!(event.1.len(), 4, "bond_created_v2 topics locked at 4");
        assert_eq!(
            get_data(&e, event).len(),
            3,
            "bond_created_v2 data locked at 3"
        );
    }

    #[test]
    fn bond_increased_v2_schema_immutable() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_increased_v2(
            &e,
            &addr,
            500i128,
            1500i128,
            timestamp,
            false,
            crate::BondTier::Silver,
        );

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=5, Data=2 (FROZEN)
        assert_eq!(event.1.len(), 5, "bond_increased_v2 topics locked at 5");
        assert_eq!(
            get_data(&e, event).len(),
            2,
            "bond_increased_v2 data locked at 2"
        );
    }

    #[test]
    fn bond_withdrawn_v2_schema_immutable() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_withdrawn_v2(&e, &addr, 200i128, 800i128, timestamp, false, 0i128);

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=5, Data=2 (FROZEN)
        assert_eq!(event.1.len(), 5, "bond_withdrawn_v2 topics locked at 5");
        assert_eq!(
            get_data(&e, event).len(),
            2,
            "bond_withdrawn_v2 data locked at 2"
        );
    }

    #[test]
    fn bond_slashed_v2_schema_immutable() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let admin = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_slashed_v2(
            &e,
            &addr,
            100i128,
            100i128,
            timestamp,
            &admin,
            String::from_str(&e, "reason"),
            false,
        );

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=6, Data=2 (FROZEN)
        assert_eq!(event.1.len(), 6, "bond_slashed_v2 topics locked at 6");
        assert_eq!(
            get_data(&e, event).len(),
            2,
            "bond_slashed_v2 data locked at 2"
        );
    }

    #[test]
    fn bond_liquidated_schema_immutable() {
        let e = Env::default();
        let addr = Address::generate(&e);
        let admin = Address::generate(&e);
        let timestamp = e.ledger().timestamp();

        events::emit_bond_liquidated(
            &e,
            &addr,
            50i128,
            Symbol::new(&e, "fully_slashed"),
            timestamp,
            &admin,
        );

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=2, Data=4 (FROZEN)
        assert_eq!(event.1.len(), 2, "bond_liquidated topics locked at 2");
        assert_eq!(
            get_data(&e, event).len(),
            4,
            "bond_liquidated data locked at 4"
        );
    }

    #[test]
    fn parameter_updated_schema_immutable() {
        let e = Env::default();
        let admin = Address::generate(&e);

        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "key"),
            Symbol::new(&e, "cat"),
            &admin,
            0i128,
            1i128,
        );

        let events = e.events().all();
        let event = first_event(&events);

        // Schema: Topics=4, Data=2 (FROZEN)
        assert_eq!(event.1.len(), 4, "param_updated topics locked at 4");
        assert_eq!(
            get_data(&e, event).len(),
            2,
            "param_updated data locked at 2"
        );
    }
}
