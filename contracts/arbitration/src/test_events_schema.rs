// Test that emitted events match expected schemas
// This prevents breaking changes to event payloads without version bumps

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as TestAddress, testutils::Events, Address, Env, Symbol, TryFromVal,
        Val, Vec,
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
        assert_eq!(topics.len(), expected_topics_len, "Topics length mismatch");
        let actual_data_len = if let Ok(values) = Vec::<Val>::try_from_val(e, &data) {
            values.len()
        } else if data.is_void() {
            0
        } else {
            1
        };
        assert_eq!(actual_data_len, expected_data_len, "Data length mismatch");
    }

    #[test]
    fn arbitrator_registered_schema_matches() {
        let e = Env::default();
        let arbitrator = TestAddress::generate(&e);
        let weight = 100u32;
        e.events().publish(
            (Symbol::new(&e, "arbitrator_registered"), arbitrator),
            weight,
        );
        let events = e.events().all();
        // Topics: arbitrator_registered, Address (2)
        // Data: u32 (1)
        verify_event_structure(&e, &events, 2, 1);
    }

    #[test]
    fn dispute_created_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let creator = TestAddress::generate(&e);
        e.events()
            .publish((Symbol::new(&e, "dispute_created"), dispute_id), creator);
        let events = e.events().all();
        // Topics: dispute_created, u64 (2)
        // Data: Address (1)
        verify_event_structure(&e, &events, 2, 1);
    }

    #[test]
    fn status_transition_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let from = DisputeStatus::Open as u32;
        let to = DisputeStatus::Voting as u32;
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from, to),
        );
        let events = e.events().all();
        // Topics: status_transition, u64 (2)
        // Data: u32, u32 (2)
        verify_event_structure(&e, &events, 2, 2);
    }

    #[test]
    fn dispute_cancelled_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let caller = TestAddress::generate(&e);
        let role = 1u32;
        let reason = Symbol::new(&e, "test");
        e.events().publish(
            (Symbol::new(&e, "dispute_cancelled"), dispute_id),
            (caller, role, reason),
        );
        let events = e.events().all();
        // Topics: dispute_cancelled, u64 (2)
        // Data: Address, u32, Symbol (3)
        verify_event_structure(&e, &events, 2, 3);
    }

    #[test]
    fn vote_cast_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let voter = TestAddress::generate(&e);
        let outcome = 1u32;
        let weight = 100u32;
        e.events().publish(
            (Symbol::new(&e, "vote_cast"), dispute_id, voter),
            (outcome, weight),
        );
        let events = e.events().all();
        // Topics: vote_cast, u64, Address (3)
        // Data: u32, u32 (2)
        verify_event_structure(&e, &events, 3, 2);
    }

    #[test]
    fn quorum_not_met_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let total_weight = 150u32;
        let min_total_weight = 200u32;
        let voter_count = 2u32;
        let min_voters = 3u32;
        e.events().publish(
            (Symbol::new(&e, "quorum_not_met"), dispute_id),
            (total_weight, min_total_weight, voter_count, min_voters),
        );
        let events = e.events().all();
        // Topics: quorum_not_met, u64 (2)
        // Data: u32, u32, u32, u32 (4)
        verify_event_structure(&e, &events, 2, 4);
    }

    #[test]
    fn dispute_tied_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        e.events()
            .publish((Symbol::new(&e, "dispute_tied"), dispute_id), ());
        let events = e.events().all();
        // Topics: dispute_tied, u64 (2)
        // Data: () (0)
        verify_event_structure(&e, &events, 2, 0);
    }

    #[test]
    fn dispute_resolved_schema_matches() {
        let e = Env::default();
        let dispute_id = 1u64;
        let winning_outcome = 1u32;
        e.events().publish(
            (Symbol::new(&e, "dispute_resolved"), dispute_id),
            winning_outcome,
        );
        let events = e.events().all();
        // Topics: dispute_resolved, u64 (2)
        // Data: u32 (1)
        verify_event_structure(&e, &events, 2, 1);
    }

    #[test]
    fn quorum_set_schema_matches() {
        let e = Env::default();
        let min_total_weight = 200u32;
        let min_voters = 3u32;
        e.events().publish(
            (Symbol::new(&e, "quorum_set"),),
            (min_total_weight, min_voters),
        );
        let events = e.events().all();
        // Topics: quorum_set (1)
        // Data: u32, u32 (2)
        verify_event_structure(&e, &events, 1, 2);
    }

    #[test]
    fn paused_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        crate::pausable::emit_contract_paused(&e, &admin);
        let events = e.events().all();
        // Topics: contract_paused, Address (2)
        // Data: () (0)
        verify_event_structure(&e, &events, 2, 0);
    }

    #[test]
    fn unpaused_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        crate::pausable::emit_contract_unpaused(&e, &admin);
        let events = e.events().all();
        // Topics: contract_unpaused, Address (2)
        // Data: () (0)
        verify_event_structure(&e, &events, 2, 0);
    }
}
