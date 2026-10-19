//! Deterministic failure-boundary coverage for `bump_instance_ttl`.
//!
//! `bump_instance_ttl` (private, `lib.rs`) is the single helper every pause
//! entry point calls before touching state:
//!
//! ```ignore
//! fn bump_instance_ttl(e: &Env) {
//!     e.storage()
//!         .instance()
//!         .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
//! }
//! ```
//!
//! Because it runs on every privileged call it must be:
//!
//! * **safe**       — never panic, on a fresh instance or after ledger advances;
//! * **monotonic**  — never shrink the instance TTL (`extend_ttl` only grows it);
//! * **idempotent** — a second call in the same state changes nothing;
//! * **side-effect free** — no storage entries added/removed, no event, and no
//!                   config-epoch advance (the retry contract in `lib.rs`).

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

fn stored_signer_count(e: &Env, client: &AdminContractClient) -> u32 {
    e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0)
    })
}

// ---------------------------------------------------------------------------
// TTL monotonicity / idempotency
// ---------------------------------------------------------------------------

/// A bump never shrinks the instance TTL, and a second consecutive bump is a
/// no-op (the TTL is already above the extend threshold).
#[test]
fn bump_never_shrinks_ttl_and_is_idempotent() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);

    e.as_contract(&contract_id, || {
        let before = e.storage().instance().get_ttl();

        crate::bump_instance_ttl(&e);
        let after_first = e.storage().instance().get_ttl();
        assert!(
            after_first >= before,
            "a bump must never shrink the instance TTL"
        );

        crate::bump_instance_ttl(&e);
        let after_second = e.storage().instance().get_ttl();
        assert_eq!(after_second, after_first, "a second bump is a no-op");
    });
}

/// The helper only extends the TTL: it does not add or remove instance entries.
#[test]
fn bump_does_not_add_or_remove_instance_entries() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);

    e.as_contract(&contract_id, || {
        let before = e.storage().instance().all().len();
        crate::bump_instance_ttl(&e);
        crate::bump_instance_ttl(&e);
        let after = e.storage().instance().all().len();
        assert_eq!(after, before, "the bump must not mutate instance entries");
    });
}

/// The instance stays live across ledger advances because each call re-extends
/// it (this is the whole point of bumping before every privileged operation).
#[test]
fn bump_keeps_instance_live_across_ledger_advances() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);

    e.as_contract(&contract_id, || {
        crate::bump_instance_ttl(&e);
        e.ledger().with_mut(|l| l.sequence_number += 10);
        crate::bump_instance_ttl(&e);
        assert!(
            e.storage().instance().get_ttl() > 0,
            "the instance must remain live after re-bumping"
        );
    });
}

// ---------------------------------------------------------------------------
// Side-effect freedom
// ---------------------------------------------------------------------------

/// Bumping preserves all stored state, emits no events, and does not advance the
/// config epoch — so it can never desynchronise indexers or the retry contract.
#[test]
fn bump_preserves_state_epoch_and_events() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);
    client.set_pause_signer(&super_admin, &signer, &true);
    client.set_pause_threshold(&super_admin, &1u32);
    client.pause(&super_admin);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();
    let paused_before = client.is_paused();

    e.as_contract(&client.address, || {
        crate::bump_instance_ttl(&e);
        crate::bump_instance_ttl(&e);
    });

    assert_eq!(client.get_config_epoch(), epoch_before, "no epoch advance");
    assert_eq!(e.events().all().len(), events_before, "no events emitted");
    assert_eq!(client.is_paused(), paused_before, "pause state unchanged");
    assert_eq!(
        stored_signer_count(&e, &client),
        1,
        "the registered signer is untouched"
    );
}

// ---------------------------------------------------------------------------
// Integration: the entry points that call the helper never shrink the TTL
// ---------------------------------------------------------------------------

/// Every pause entry point routes through `bump_instance_ttl`; running a mix of
/// read and write entry points must leave the instance TTL no smaller than it
/// started.
#[test]
fn pause_entrypoints_do_not_shrink_instance_ttl() {
    let (e, client, super_admin) = setup();

    // Seed a healthy TTL, then take a baseline inside the contract context.
    e.as_contract(&client.address, || crate::bump_instance_ttl(&e));
    let baseline = e.as_contract(&client.address, || e.storage().instance().get_ttl());

    client.is_paused();
    client.pause(&super_admin);
    client.unpause(&super_admin);
    client.set_pause_signer(&super_admin, &Address::generate(&e), &true);
    client.set_pause_threshold(&super_admin, &1u32);

    let after = e.as_contract(&client.address, || e.storage().instance().get_ttl());
    assert!(
        after >= baseline,
        "entry points must not shrink the instance TTL"
    );
}
