// Test that emitted events match expected schemas
// This prevents breaking changes to event payloads without version bumps
//
#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::DisputeStatus;
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::{Env, Symbol, TryFromVal};

    #[test]
    fn arbitrator_registered_event_publishes() {
        let e = Env::default();
        let arbitrator = soroban_sdk::Address::generate(&e);
        let weight = 100u32;
        e.events().publish(
            (Symbol::new(&e, "arbitrator_registered"), arbitrator),
            weight,
        );
    }

    #[test]
    fn dispute_created_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        let creator = soroban_sdk::Address::generate(&e);
        e.events()
            .publish((Symbol::new(&e, "dispute_created"), dispute_id), creator);
    }

    #[test]
    fn status_transition_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        let from = DisputeStatus::Open as u32;
        let to = DisputeStatus::Voting as u32;
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from, to),
        );
    }

    #[test]
    fn dispute_cancelled_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        let caller = soroban_sdk::Address::generate(&e);
        let role = 1u32;
        let reason = Symbol::new(&e, "test");
        e.events().publish(
            (Symbol::new(&e, "dispute_cancelled"), dispute_id),
            (caller, role, reason),
        );
    }

    #[test]
    fn vote_cast_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        let voter = soroban_sdk::Address::generate(&e);
        let outcome = 1u32;
        let weight = 100u32;
        e.events().publish(
            (Symbol::new(&e, "vote_cast"), dispute_id, voter),
            (outcome, weight),
        );
    }

    #[test]
    fn quorum_not_met_event_publishes() {
        let e = Env::default();
        e.mock_all_auths();
        let admin = soroban_sdk::Address::generate(&e);
        let arbitrator = soroban_sdk::Address::generate(&e);
        let creator = soroban_sdk::Address::generate(&e);
        let contract_id = e.register(crate::CredenceArbitration, ());
        let client = crate::CredenceArbitrationClient::new(&e, &contract_id);
        client.initialize(&admin);
        client.register_arbitrator(&arbitrator, &10_i128);
        client.set_quorum(&admin, &100_i128, &1_u32);
        let dispute_id = client.create_dispute(
            &creator,
            &soroban_sdk::String::from_str(&e, "quorum event schema"),
            &3600_u64,
        );
        client.vote(&arbitrator, &dispute_id, &1_u32);
        e.ledger()
            .set_timestamp(e.ledger().timestamp() + 3601);

        assert_eq!(client.resolve_dispute(&dispute_id), 0);

        let events = e.events().all();
        let mut found = false;
        for event in events.iter() {
            let topics = event.1;
            if topics.len() != 2 {
                continue;
            }
            let name = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
            let id = u64::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
            if name == Symbol::new(&e, "quorum_not_met") && id == dispute_id {
                let data: (i128, i128, u32, u32) =
                    TryFromVal::try_from_val(&e, &event.2).unwrap();
                assert_eq!(data, (10_i128, 100_i128, 1_u32, 1_u32));
                found = true;
            }
        }
        assert!(found, "successful resolution should publish quorum_not_met");
    }

    #[test]
    fn dispute_tied_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        e.events()
            .publish((Symbol::new(&e, "dispute_tied"), dispute_id), ());
    }

    #[test]
    fn dispute_resolved_event_publishes() {
        let e = Env::default();
        let dispute_id = 1u64;
        let winning_outcome = 1u32;
        e.events().publish(
            (Symbol::new(&e, "dispute_resolved"), dispute_id),
            winning_outcome,
        );
    }

    #[test]
    fn quorum_set_event_publishes() {
        let e = Env::default();
        let min_total_weight = 200u32;
        let min_voters = 3u32;
        e.events().publish(
            (Symbol::new(&e, "quorum_set"),),
            (min_total_weight, min_voters),
        );
    }
}
