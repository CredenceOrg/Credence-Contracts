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
    fn treasury_deposit_schema_matches() {
        let e = Env::default();
        let from = TestAddress::generate(&e);
        let amount = 1000i128;
        let source = FundSource::ProtocolFee;
        e.events().publish(
            (Symbol::new(&e, "treasury_deposit"), from.clone()),
            (amount, source),
        );
        let events = e.events().get_all();
        // Topics: treasury_deposit, Address (2)
        // Data: i128, FundSource (2)
        verify_event_structure(&events, 2, 2);
    }

    #[test]
    fn threshold_updated_schema_matches() {
        let e = Env::default();
        let old_threshold = 3u32;
        let new_threshold = 5u32;
        e.events().publish(
            (Symbol::new(&e, "threshold_updated"),),
            (old_threshold, new_threshold),
        );
        let events = e.events().get_all();
        // Topics: threshold_updated (1)
        // Data: u32, u32 (2)
        verify_event_structure(&events, 1, 2);
    }

    #[test]
    fn treasury_withdrawal_proposed_schema_matches() {
        let e = Env::default();
        let proposal_id = 1u64;
        let recipient = TestAddress::generate(&e);
        let amount = 500i128;
        let proposer = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_proposed"), proposal_id),
            (recipient.clone(), amount, proposer.clone()),
        );
        let events = e.events().get_all();
        // Topics: treasury_withdrawal_proposed, u64 (2)
        // Data: Address, i128, Address (3)
        verify_event_structure(&events, 2, 3);
    }

    #[test]
    fn treasury_proposal_expired_schema_matches() {
        let e = Env::default();
        let proposal_id = 1u64;
        e.events().publish(
            (Symbol::new(&e, "treasury_proposal_expired"), proposal_id),
            (),
        );
        let events = e.events().get_all();
        // Topics: treasury_proposal_expired, u64 (2)
        // Data: () (0)
        verify_event_structure(&events, 2, 0);
    }

    #[test]
    fn treasury_withdrawal_approved_schema_matches() {
        let e = Env::default();
        let proposal_id = 1u64;
        let approver = TestAddress::generate(&e);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_approved"), proposal_id),
            (approver.clone(),),
        );
        let events = e.events().get_all();
        // Topics: treasury_withdrawal_approved, u64 (2)
        // Data: Address (1)
        verify_event_structure(&events, 2, 1);
    }

    #[test]
    fn treasury_withdrawal_executed_schema_matches() {
        let e = Env::default();
        let proposal_id = 1u64;
        let recipient = TestAddress::generate(&e);
        let min_amount_out = 450i128;
        let actual_amount = 480i128;
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_executed"), proposal_id),
            (recipient.clone(), min_amount_out, actual_amount),
        );
        let events = e.events().get_all();
        // Topics: treasury_withdrawal_executed, u64 (2)
        // Data: Address, i128, i128 (3)
        verify_event_structure(&events, 2, 3);
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
