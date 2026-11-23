//! Regression tests for storage TTL bumps added in issue #570.
//!
//! Verifies that persistent storage entries in `credence_bond` survive ledger
//! advancement after the TTL bump fix. For each covered path:
//! 1. Write the entry (slash, claim).
//! 2. Advance the ledger.
//! 3. Read the entry back and assert it still returns the correct value.
//!
//! Note: The `emergency` module is not yet integrated into the public API
//! (no `mod emergency;` in lib.rs), so emergency TTL coverage is omitted here.

use crate::{
    claims::{self, ClaimType},
    slash_history, CredenceBond,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> Address {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = crate::CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    contract_id
}

// ── Slash history ─────────────────────────────────────────────────────────────

/// SlashRecord and SlashCount survive significant ledger advancement.
#[test]
fn test_slash_record_survives_ledger_advancement() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
        li.sequence_number = 1_000;
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    let contract_id = setup(&e);
    let identity = Address::generate(&e);
    let reason = soroban_sdk::Symbol::new(&e, "slashed");

    e.as_contract(&contract_id, || {
        slash_history::append_slash_history(&e, &identity, 500, reason.clone(), 500);
    });

    // Advance ledger by ~1 month worth of ledgers (but less than PERSISTENT_TTL_MAX).
    e.ledger().with_mut(|li| {
        li.sequence_number += 518_400;
        li.timestamp += 518_400 * 5;
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    e.as_contract(&contract_id, || {
        let count = slash_history::get_slash_count(&e, &identity);
        assert_eq!(count, 1, "SlashCount must survive ledger advancement");

        let history = slash_history::get_slash_history(&e, &identity);
        assert_eq!(history.len(), 1);
        let record = history.get(0).unwrap();
        assert_eq!(record.slash_amount, 500);
        assert_eq!(record.reason, reason);
    });
}

/// Multiple slash records all survive ledger advancement.
#[test]
fn test_slash_multiple_records_survive_ledger_advancement() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
        li.sequence_number = 1_000;
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    let contract_id = setup(&e);
    let identity = Address::generate(&e);

    e.as_contract(&contract_id, || {
        slash_history::append_slash_history(
            &e,
            &identity,
            100,
            soroban_sdk::Symbol::new(&e, "r1"),
            100,
        );
        slash_history::append_slash_history(
            &e,
            &identity,
            200,
            soroban_sdk::Symbol::new(&e, "r2"),
            300,
        );
        slash_history::append_slash_history(
            &e,
            &identity,
            300,
            soroban_sdk::Symbol::new(&e, "r3"),
            600,
        );
    });

    e.ledger().with_mut(|li| {
        li.sequence_number += 100_000;
        li.timestamp += 500_000_000;
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    e.as_contract(&contract_id, || {
        assert_eq!(slash_history::get_slash_count(&e, &identity), 3);
        let r0 = slash_history::get_slash_record(&e, &identity, 0);
        assert_eq!(r0.slash_amount, 100);
        let r2 = slash_history::get_slash_record(&e, &identity, 2);
        assert_eq!(r2.slash_amount, 300);
    });
}

// ── Claims ────────────────────────────────────────────────────────────────────

/// ClaimCounter and PendingClaims survive ledger advancement (before claim expiry).
#[test]
fn test_claim_counter_and_pending_survive_ledger_advancement() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
        li.sequence_number = 5_000;
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    let contract_id = setup(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let id = claims::add_pending_claim(
            &e,
            &user,
            ClaimType::VerifierReward,
            500,
            42,
            Some(soroban_sdk::Symbol::new(&e, "meta")),
        );
        assert_eq!(id, 1);
    });

    // Advance by 5 days (well within the 30-day claim window).
    e.ledger().with_mut(|li| {
        li.sequence_number += 86_400; // 5 days at 5 s/ledger
        li.timestamp += 432_000; // 5 days in seconds
        li.max_entry_ttl = crate::PERSISTENT_TTL_MAX;
    });

    e.as_contract(&contract_id, || {
        let pending = claims::get_pending_claims(&e, &user);
        assert_eq!(
            pending.len(),
            1,
            "PendingClaims must survive ledger advancement"
        );

        let claimable = claims::get_claimable_amount(&e, &user);
        assert_eq!(
            claimable, 500,
            "ClaimableAmount must survive ledger advancement"
        );
    });
}
