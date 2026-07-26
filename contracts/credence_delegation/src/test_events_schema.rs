// Test that emitted events match expected schemas
// This prevents breaking changes to event payloads without version bumps

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Env, TryFromVal, Val, Vec,
    };

    type ContractEvent = (Address, Vec<Val>, Val);

    fn verify_event_structure(
        e: &Env,
        events: &Vec<ContractEvent>,
        expected_topics_len: u32,
        expected_data_len: u32,
    ) {
        assert_eq!(events.len(), 1, "Expected exactly one event");
        let (_, topics, data) = events.get(0).unwrap();
        assert_eq!(
            topics.len(),
            expected_topics_len,
            "Topics length mismatch"
        );
        let actual_data_len = if let Ok(values) = Vec::<Val>::try_from_val(e, &data) {
            values.len()
        } else if data.is_void() {
            0
        } else {
            1
        };
        assert_eq!(
            actual_data_len, expected_data_len,
            "Data length mismatch"
        );
    }

    #[test]
    fn verifier_registered_schema_matches() {
        let e = Env::default();
        let scheme = 1u32;
        let verifier_id = Address::generate(&e);
        let admin = Address::generate(&e);
        crate::verifier::emit_verifier_registered(&e, scheme, &verifier_id, &admin);
        let events = e.events().all();
        // Topics: verifier_registered, u32, Address, Address (4)
        // Data: () (0)
        verify_event_structure(&e, &events, 4, 0);
    }

    #[test]
    fn contract_paused_schema_matches() {
        let e = Env::default();
        let admin = Address::generate(&e);
        crate::pausable::emit_contract_paused(&e, &admin);
        let events = e.events().all();
        // Topics: contract_paused, Address (2)
        // Data: () (0)
        verify_event_structure(&e, &events, 2, 0);
    }

    #[test]
    fn contract_unpaused_schema_matches() {
        let e = Env::default();
        let admin = Address::generate(&e);
        crate::pausable::emit_contract_unpaused(&e, &admin);
        let events = e.events().all();
        // Topics: contract_unpaused, Address (2)
        // Data: () (0)
        verify_event_structure(&e, &events, 2, 0);
    }
}
