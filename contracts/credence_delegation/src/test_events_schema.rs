// Test that emitted events match expected schemas
// This prevents breaking changes to event payloads without version bumps

#[cfg(test)]
mod tests {
    use super::*;
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
    fn verifier_registered_schema_matches() {
        let e = Env::default();
        let scheme = 1u32;
        let verifier_id = TestAddress::generate(&e);
        let admin = TestAddress::generate(&e);
        crate::verifier::emit_verifier_registered(&e, scheme, &verifier_id, &admin);
        let events = e.events().get_all();
        // Topics: verifier_registered, u32, Address, Address (4)
        // Data: () (0)
        verify_event_structure(&events, 4, 0);
    }

    #[test]
    fn contract_paused_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        crate::pausable::emit_contract_paused(&e, &admin);
        let events = e.events().get_all();
        // Topics: contract_paused, Address (2)
        // Data: () (0)
        verify_event_structure(&events, 2, 0);
    }

    #[test]
    fn contract_unpaused_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        crate::pausable::emit_contract_unpaused(&e, &admin);
        let events = e.events().get_all();
        // Topics: contract_unpaused, Address (2)
        // Data: () (0)
        verify_event_structure(&events, 2, 0);
    }
}
