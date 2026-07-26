// Test that emitted events match the frozen v1 schemas
// This prevents breaking changes to event payloads without version bumps

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events;
    use soroban_sdk::{testutils::Address as TestAddress, testutils::Events, Env, Symbol};

    fn verify_event_structure(
        events: &soroban_sdk::Vec<soroban_sdk::ContractEvent>,
        expected_topics_len: u32,
        expected_data_len: u32,
    ) {
        assert_eq!(events.len(), 1, "Expected exactly one event");
        let ev = &events[0];
        assert_eq!(
            ev.topics.len(),
            expected_topics_len,
            "Topics length mismatch"
        );
        assert_eq!(ev.data.len(), expected_data_len, "Data length mismatch");
    }

    #[test]
    fn bond_created_v2_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, e.ledger().timestamp());
        let events = e.events().get_all();
        // Topics: bond_created_v2, Address, i128, u64 (4)
        // Data: u64, bool, u64 (3)
        verify_event_structure(&events, 4, 3);
    }

    #[test]
    fn bond_created_v1_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_created(&e, &addr, 1000i128, 3600u64, false);
        let events = e.events().get_all();
        // Topics: bond_created, Address (2)
        // Data: i128, u64, bool (3)
        verify_event_structure(&events, 2, 3);
    }

    #[test]
    fn bond_increased_v2_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_increased_v2(
            &e,
            &addr,
            500i128,
            1500i128,
            e.ledger().timestamp(),
            true,
            crate::BondTier::Silver,
        );
        let events = e.events().get_all();
        // Topics: bond_increased_v2, Address, i128, i128, u64 (5)
        // Data: bool, BondTier (2)
        verify_event_structure(&events, 5, 2);
    }

    #[test]
    fn bond_increased_v1_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_increased(&e, &addr, 500i128, 1500i128);
        let events = e.events().get_all();
        // Topics: bond_increased, Address (2)
        // Data: i128, i128 (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn bond_withdrawn_v2_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_withdrawn_v2(
            &e,
            &addr,
            200i128,
            800i128,
            e.ledger().timestamp(),
            true,
            10i128,
        );
        let events = e.events().get_all();
        // Topics: bond_withdrawn_v2, Address, i128, i128, u64 (5)
        // Data: bool, i128 (2)
        verify_event_structure(&events, 5, 2);
    }

    #[test]
    fn bond_withdrawn_v1_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_withdrawn(&e, &addr, 200i128, 800i128);
        let events = e.events().get_all();
        // Topics: bond_withdrawn, Address (2)
        // Data: i128, i128 (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn bond_slashed_v2_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        events::emit_bond_slashed_v2(
            &e,
            &addr,
            100i128,
            100i128,
            e.ledger().timestamp(),
            &admin,
            soroban_sdk::String::from_str(&e, "test"),
            true,
        );
        let events = e.events().get_all();
        // Topics: bond_slashed_v2, Address, i128, i128, u64, Address (6)
        // Data: String, bool (2)
        verify_event_structure(&events, 6, 2);
    }

    #[test]
    fn bond_slashed_v1_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_slashed(&e, &addr, 100i128, 100i128);
        let events = e.events().get_all();
        // Topics: bond_slashed, Address (2)
        // Data: i128, i128 (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn bond_liquidated_schema_matches() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        events::emit_bond_liquidated(
            &e,
            &addr,
            50i128,
            Symbol::new(&e, "fully_slashed"),
            e.ledger().timestamp(),
            &admin,
        );
        let events = e.events().get_all();
        // Topics: bond_liquidated, Address (2)
        // Data: i128, Symbol, u64, Address (4)
        verify_event_structure(&events, 2, 4);
    }

    #[test]
    fn param_updated_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "leverage"),
            Symbol::new(&e, "risk"),
            &admin,
            10i128,
            15i128,
        );
        let events = e.events().get_all();
        // Topics: param_updated, Symbol, Symbol, Address (4)
        // Data: i128, i128 (2)
        verify_event_structure(&events, 4, 2);
    }

    #[test]
    fn upgrade_executed_schema_matches() {
        let e = Env::default();
        let executor = TestAddress::generate(&e);
        let new_impl = TestAddress::generate(&e);
        events::emit_upgrade_executed(&e, &executor, &new_impl, Some(42u64));
        let events = e.events().get_all();
        // Topics: upgrade_executed, Address (2)
        // Data: Address, Option<u64> (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn bond_drift_detected_schema_matches() {
        let e = Env::default();
        let subject = TestAddress::generate(&e);
        let details = crate::invariants::BondDriftDetails {
            subject: subject.clone(),
            kind: crate::invariants::BondDriftKind::BondAmountMismatch,
            bonded_amount: 1000i128,
            slashed_amount: 0i128,
            attestation_count: 5u32,
            attestation_list_len: 5u32,
        };
        events::emit_bond_drift_detected(&e, &details);
        let events = e.events().get_all();
        // Topics: bond_drift_detected, Address (2)
        // Data: BondDriftKind, i128, i128, u32, u32 (5)
        verify_event_structure(&events, 2, 5);
    }

    #[test]
    #[should_panic(expected = "Topics length mismatch")]
    fn schema_change_detection_topics_fails() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, e.ledger().timestamp());
        let events = e.events().get_all();
        // This should fail because we're asserting wrong topic length
        verify_event_structure(&events, 99, 3);
    }

    #[test]
    #[should_panic(expected = "Data length mismatch")]
    fn schema_change_detection_data_fails() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        events::emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, e.ledger().timestamp());
        let events = e.events().get_all();
        // This should fail because we're asserting wrong data length
        verify_event_structure(&events, 4, 99);
    }
}
