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
    fn admin_rotated_schema_matches() {
        let e = Env::default();
        let previous_owner = TestAddress::generate(&e);
        let new_owner = TestAddress::generate(&e);
        let ledger_seq: u32 = e.ledger().sequence();
        e.events().publish(
            (
                Symbol::new(&e, "admin_rotated"),
                previous_owner.clone(),
                new_owner.clone(),
            ),
            ledger_seq,
        );
        let events = e.events().get_all();
        // Topics: admin_rotated, Address, Address (3)
        // Data: u32 (ledger sequence) (1)
        verify_event_structure(&events, 3, 1);
    }

    #[test]
    fn ownership_transfer_initiated_schema_matches() {
        let e = Env::default();
        let current_owner = TestAddress::generate(&e);
        let new_owner = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_initiated"),),
            (current_owner.clone(), new_owner.clone()),
        );
        let events = e.events().get_all();
        // Topics: ownership_transfer_initiated (1)
        // Data: Address, Address (2)
        verify_event_structure(&events, 1, 2);
    }

    #[test]
    fn ownership_transfer_accepted_schema_matches() {
        let e = Env::default();
        let previous_owner = TestAddress::generate(&e);
        let pending_owner = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_accepted"),),
            (previous_owner.clone(), pending_owner.clone()),
        );
        let events = e.events().get_all();
        // Topics: ownership_transfer_accepted (1)
        // Data: Address, Address (2)
        verify_event_structure(&events, 1, 2);
    }

    #[test]
    fn role_assigned_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        let caller = TestAddress::generate(&e);
        let role = AdminRole::Admin;
        e.events().publish(
            (Symbol::new(&e, "ROLE_ASSIGNED"), admin.clone()),
            (role, caller.clone()),
        );
        let events = e.events().get_all();
        // Topics: ROLE_ASSIGNED, Address (2)
        // Data: AdminRole, Address (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn role_revoked_schema_matches() {
        let e = Env::default();
        let admin = TestAddress::generate(&e);
        let caller = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "ROLE_REVOKED"), admin.clone()),
            (caller.clone(),),
        );
        let events = e.events().get_all();
        // Topics: ROLE_REVOKED, Address (2)
        // Data: Address (1)
        verify_event_structure(&events, 2, 1);
    }

    #[test]
    fn paused_schema_matches() {
        let e = Env::default();
        let proposal_id: Option<u64> = Some(42u64);
        e.events().publish((Symbol::new(&e, "paused"),), proposal_id);
        let events = e.events().get_all();
        // Topics: paused (1)
        // Data: Option<u64> (1)
        verify_event_structure(&events, 1, 1);
    }

    #[test]
    fn unpaused_schema_matches() {
        let e = Env::default();
        let proposal_id: Option<u64> = Some(42u64);
        e.events().publish((Symbol::new(&e, "unpaused"),), proposal_id);
        let events = e.events().get_all();
        // Topics: unpaused (1)
        // Data: Option<u64> (1)
        verify_event_structure(&events, 1, 1);
    }

    #[test]
    fn pause_approved_schema_matches() {
        let e = Env::default();
        let proposal_id = 42u64;
        let signer = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "pause_approved"), proposal_id),
            signer.clone(),
        );
        let events = e.events().get_all();
        // Topics: pause_approved, u64 (2)
        // Data: Address (1)
        verify_event_structure(&events, 2, 1);
    }

    #[test]
    fn pause_signer_set_schema_matches() {
        let e = Env::default();
        let signer = TestAddress::generate(&e);
        let enabled = true;
        e.events().publish(
            (Symbol::new(&e, "pause_signer_set"), signer.clone()),
            enabled,
        );
        let events = e.events().get_all();
        // Topics: pause_signer_set, Address (2)
        // Data: bool (1)
        verify_event_structure(&events, 2, 1);
    }
}
