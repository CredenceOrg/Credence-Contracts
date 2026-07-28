//! Schema regression tests for every event emitted by the Admin contract.
//!
//! Each test publishes a single event in isolation (no contract storage) and
//! then reads it back via `env.events().all()`.  This locks the topic-count
//! and data-type for each event so that breaking payload changes are caught at
//! compile/test time rather than at runtime on-chain.
//!
//! SDK 22 events API:
//!   `env.events().all()` → `Vec<(Address, Vec<Val>, Val)>`
//!   tuple fields: (contract_id, topics, data)

#[cfg(test)]
mod tests {
    use crate::AdminRole;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env, String, Symbol, TryFromVal, TryIntoVal,
    };

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Decode a `Val` into `T` or panic with a helpful message.
    macro_rules! decode {
        ($env:expr, $val:expr, $ty:ty, $msg:literal) => {
            <$ty>::try_from_val($env, &$val).expect($msg)
        };
    }

    /// Retrieve the single event emitted during the test.
    fn only_event(env: &Env) -> (soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val) {
        let all = env.events().all();
        assert_eq!(all.len(), 1, "expected exactly one event");
        let ev = all.iter().next().unwrap();
        (ev.1, ev.2)
    }

    // ── ROLE_ASSIGNED ─────────────────────────────────────────────────────────

    #[test]
    fn role_assigned_topics_and_data_types_match_schema() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let caller = Address::generate(&e);
        let role = AdminRole::Admin;

        // Mirrors the emission in lib.rs add_admin / update_admin_role / reactivate_admin
        e.events().publish(
            (Symbol::new(&e, "ROLE_ASSIGNED"), actor.clone()),
            (role, caller.clone()),
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 2, "ROLE_ASSIGNED must have 2 topics");

        let t0 = decode!(
            &e,
            topics.get(0).unwrap(),
            Symbol,
            "topic[0] must be Symbol"
        );
        assert_eq!(t0, Symbol::new(&e, "ROLE_ASSIGNED"));

        let t1 = decode!(
            &e,
            topics.get(1).unwrap(),
            Address,
            "topic[1] must be Address"
        );
        assert_eq!(t1, actor);

        let (r, c): (AdminRole, Address) = decode!(
            &e,
            data,
            (AdminRole, Address),
            "data must be (AdminRole, Address)"
        );
        assert_eq!(r, role);
        assert_eq!(c, caller);
    }

    // ── ROLE_REVOKED ──────────────────────────────────────────────────────────

    #[test]
    fn role_revoked_topics_and_data_types_match_schema() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let caller = Address::generate(&e);

        // Mirrors the emission in lib.rs remove_admin / deactivate_admin
        e.events().publish(
            (Symbol::new(&e, "ROLE_REVOKED"), actor.clone()),
            (caller.clone(),),
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 2, "ROLE_REVOKED must have 2 topics");

        let t0 = decode!(
            &e,
            topics.get(0).unwrap(),
            Symbol,
            "topic[0] must be Symbol"
        );
        assert_eq!(t0, Symbol::new(&e, "ROLE_REVOKED"));

        let t1 = decode!(
            &e,
            topics.get(1).unwrap(),
            Address,
            "topic[1] must be Address"
        );
        assert_eq!(t1, actor);

        let (c,): (Address,) = decode!(&e, data, (Address,), "data must be (Address,)");
        assert_eq!(c, caller);
    }

    // ── admin_rotated ─────────────────────────────────────────────────────────

    #[test]
    fn admin_rotated_schema_matches() {
        let e = Env::default();
        let prev = Address::generate(&e);
        let next = Address::generate(&e);
        let seq: u32 = e.ledger().sequence();

        e.events().publish(
            (Symbol::new(&e, "admin_rotated"), prev.clone(), next.clone()),
            seq,
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 3, "admin_rotated must have 3 topics");
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "admin_rotated"));
        let t1 = decode!(&e, topics.get(1).unwrap(), Address, "t1 Address");
        assert_eq!(t1, prev);
        let t2 = decode!(&e, topics.get(2).unwrap(), Address, "t2 Address");
        assert_eq!(t2, next);

        let d: u32 = decode!(&e, data, u32, "data must be u32 ledger sequence");
        assert_eq!(d, seq);
    }

    // ── ownership_transfer_initiated ─────────────────────────────────────────

    #[test]
    fn ownership_transfer_initiated_schema_matches() {
        let e = Env::default();
        let current = Address::generate(&e);
        let pending = Address::generate(&e);

        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_initiated"),),
            (current.clone(), pending.clone()),
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 1);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "ownership_transfer_initiated"));

        let (c, p): (Address, Address) = decode!(
            &e,
            data,
            (Address, Address),
            "data must be (Address, Address)"
        );
        assert_eq!(c, current);
        assert_eq!(p, pending);
    }

    // ── ownership_transfer_accepted ──────────────────────────────────────────

    #[test]
    fn ownership_transfer_accepted_schema_matches() {
        let e = Env::default();
        let prev = Address::generate(&e);
        let next = Address::generate(&e);

        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_accepted"),),
            (prev.clone(), next.clone()),
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 1);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "ownership_transfer_accepted"));

        let (p, n): (Address, Address) = decode!(&e, data, (Address, Address), "data (prev, next)");
        assert_eq!(p, prev);
        assert_eq!(n, next);
    }

    // ── paused ────────────────────────────────────────────────────────────────

    #[test]
    fn paused_schema_matches() {
        let e = Env::default();
        let proposal_id: Option<u64> = Some(42u64);
        let reason = String::from_str(&e, "test_reason");

        e.events()
            .publish((Symbol::new(&e, "paused"),), (proposal_id, reason.clone()));

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 1);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "paused"));

        let (pid, rsn): (Option<u64>, String) = decode!(
            &e,
            data,
            (Option<u64>, String),
            "data (Option<u64>, String)"
        );
        assert_eq!(pid, proposal_id);
        assert_eq!(rsn, reason);
    }

    // ── unpaused ──────────────────────────────────────────────────────────────

    #[test]
    fn unpaused_schema_matches() {
        let e = Env::default();
        let proposal_id: Option<u64> = Some(7u64);

        e.events()
            .publish((Symbol::new(&e, "unpaused"),), proposal_id);

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 1);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "unpaused"));

        let d: Option<u64> = decode!(&e, data, Option<u64>, "data Option<u64>");
        assert_eq!(d, proposal_id);
    }

    // ── pause_approved ────────────────────────────────────────────────────────

    #[test]
    fn pause_approved_schema_matches() {
        let e = Env::default();
        let proposal_id = 42u64;
        let signer = Address::generate(&e);

        e.events().publish(
            (Symbol::new(&e, "pause_approved"), proposal_id),
            signer.clone(),
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 2);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "pause_approved"));
        let t1 = decode!(&e, topics.get(1).unwrap(), u64, "t1 u64 proposal_id");
        assert_eq!(t1, proposal_id);

        let s: Address = decode!(&e, data, Address, "data Address signer");
        assert_eq!(s, signer);
    }

    // ── pause_signer_set ──────────────────────────────────────────────────────

    #[test]
    fn pause_signer_set_schema_matches() {
        let e = Env::default();
        let signer = Address::generate(&e);
        let enabled = true;

        e.events().publish(
            (Symbol::new(&e, "pause_signer_set"), signer.clone()),
            enabled,
        );

        let (topics, data) = only_event(&e);

        assert_eq!(topics.len(), 2);
        let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "t0 Symbol");
        assert_eq!(t0, Symbol::new(&e, "pause_signer_set"));
        let t1 = decode!(&e, topics.get(1).unwrap(), Address, "t1 Address signer");
        assert_eq!(t1, signer);

        let b: bool = decode!(&e, data, bool, "data bool enabled");
        assert_eq!(b, enabled);
    }
}
