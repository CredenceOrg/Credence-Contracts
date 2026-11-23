//! Event schema regression tests for the Credence Registry contract.
//!
//! Each test pins, for one event:
//!   * the topic count, the topic symbol and the type of every extra topic;
//!   * the payload type (and the field values that identify the call);
//!   * that the call emits exactly that one event.
//!
//! The atomicity tests at the bottom assert that a rejected call publishes no
//! event at all. Any change to these layouts is a breaking change for indexers
//! and must be reflected in `docs/EVENTS.md`.
//!
//! SDK 22 events API:
//!   `env.events().all()` → `Vec<(Address, Vec<Val>, Val)>` for the most recent
//!   top-level invocation; tuple fields are (contract_id, topics, data).

use crate::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Bytes, Env, String, Symbol, TryFromVal, Val, Vec,
};

// ── helpers ──────────────────────────────────────────────────────────────

/// Code hash returned by [`MockBond`] and pinned via `set_bond_code_hash`.
const BOND_HASH: [u8; 32] = [7; 32];

/// Minimal bond contract that reports a fixed code hash so
/// `register_trustless` can be exercised.
#[contract]
struct MockBond;

#[contractimpl]
impl MockBond {
    pub fn get_contract_code_hash(e: Env) -> Bytes {
        Bytes::from_array(&e, &BOND_HASH)
    }
}

fn setup() -> (Env, Address, CredenceRegistryClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let contract_id = e.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, contract_id, client, admin)
}

/// Return the topics and data of the single event emitted by the last call,
/// asserting that it was published by the registry itself.
fn only_event(e: &Env, contract_id: &Address) -> (Vec<Val>, Val) {
    let all = e.events().all();
    assert_eq!(all.len(), 1, "expected exactly one event");
    let (emitter, topics, data) = all.get(0).unwrap();
    assert_eq!(
        &emitter, contract_id,
        "event must be emitted by the registry"
    );
    (topics, data)
}

fn topic_symbol(e: &Env, topics: &Vec<Val>) -> Symbol {
    Symbol::try_from_val(e, &topics.get(0).unwrap()).expect("topic[0] must be a Symbol")
}

fn decode<T: TryFromVal<Env, Val>>(e: &Env, val: Val, what: &str) -> T {
    T::try_from_val(e, &val).unwrap_or_else(|_| panic!("{what} has the wrong type"))
}

fn assert_entry(entry: &RegistryEntry, identity: &Address, bond: &Address, active: bool) {
    assert_eq!(&entry.identity, identity);
    assert_eq!(&entry.bond_contract, bond);
    assert_eq!(entry.registered_at, 1_000);
    assert_eq!(entry.active, active);
}

/// Register `identity -> bond` (interface check skipped) and return both.
fn register_one(e: &Env, client: &CredenceRegistryClient) -> (Address, Address) {
    let identity = Address::generate(e);
    let bond = Address::generate(e);
    client.register(&identity, &bond, &true);
    (identity, bond)
}

// ── registry_initialized ─────────────────────────────────────────────────

#[test]
fn registry_initialized_event_schema() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&e, &contract_id);
    let admin = Address::generate(&e);

    client.initialize(&admin);

    // Topics: ("registry_initialized",)  Data: admin (Address)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "registry_initialized must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "registry_initialized")
    );
    let payload: Address = decode(&e, data, "registry_initialized data");
    assert_eq!(payload, admin);
}

// ── identity_registered ──────────────────────────────────────────────────

#[test]
fn identity_registered_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);

    // Topics: ("identity_registered",)  Data: (RegistryEntry, allow_non_interface: bool)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "identity_registered must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "identity_registered")
    );
    let (entry, allow_non_interface): (RegistryEntry, bool) =
        decode(&e, data, "identity_registered data");
    assert_entry(&entry, &identity, &bond, true);
    assert!(allow_non_interface);
}

// ── identity_deactivated / identity_reactivated / identity_removed ───────

#[test]
fn identity_deactivated_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let (identity, bond) = register_one(&e, &client);

    client.deactivate(&identity);

    // Topics: ("identity_deactivated",)  Data: RegistryEntry (active = false)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "identity_deactivated must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "identity_deactivated")
    );
    let entry: RegistryEntry = decode(&e, data, "identity_deactivated data");
    assert_entry(&entry, &identity, &bond, false);
}

#[test]
fn identity_reactivated_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let (identity, bond) = register_one(&e, &client);
    client.deactivate(&identity);

    client.reactivate(&identity);

    // Topics: ("identity_reactivated",)  Data: RegistryEntry (active = true)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "identity_reactivated must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "identity_reactivated")
    );
    let entry: RegistryEntry = decode(&e, data, "identity_reactivated data");
    assert_entry(&entry, &identity, &bond, true);
}

#[test]
fn identity_removed_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let (identity, bond) = register_one(&e, &client);

    client.remove(&identity);

    // Topics: ("identity_removed",)  Data: the removed RegistryEntry
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "identity_removed must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "identity_removed")
    );
    let entry: RegistryEntry = decode(&e, data, "identity_removed data");
    assert_entry(&entry, &identity, &bond, true);
}

// ── admin_transferred ────────────────────────────────────────────────────

#[test]
fn admin_transferred_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let new_admin = Address::generate(&e);

    client.transfer_admin(&new_admin);

    // Topics: ("admin_transferred",)  Data: new_admin (Address)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "admin_transferred must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "admin_transferred")
    );
    let payload: Address = decode(&e, data, "admin_transferred data");
    assert_eq!(payload, new_admin);
}

// ── bond_code_hash_updated / bond_registered ─────────────────────────────

#[test]
fn bond_code_hash_updated_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let hash = Bytes::from_array(&e, &BOND_HASH);

    client.set_bond_code_hash(&hash);

    // Topics: ("bond_code_hash_updated",)  Data: code_hash (Bytes)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "bond_code_hash_updated must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "bond_code_hash_updated")
    );
    let payload: Bytes = decode(&e, data, "bond_code_hash_updated data");
    assert_eq!(payload, hash);
}

#[test]
fn bond_registered_event_schema() {
    let (e, contract_id, client, _admin) = setup();
    let bond = e.register(MockBond, ());
    let identity = Address::generate(&e);
    client.set_bond_code_hash(&Bytes::from_array(&e, &BOND_HASH));

    client.register_trustless(&bond, &identity);

    // Topics: ("bond_registered",)  Data: RegistryEntry
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "bond_registered must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "bond_registered")
    );
    let entry: RegistryEntry = decode(&e, data, "bond_registered data");
    assert_entry(&entry, &identity, &bond, true);

    // The idempotent re-registration path emits the same schema.
    client.register_trustless(&bond, &identity);
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "bond_registered")
    );
    let entry: RegistryEntry = decode(&e, data, "bond_registered (idempotent) data");
    assert_entry(&entry, &identity, &bond, true);
}

// ── paused / unpaused (admin-direct path) ────────────────────────────────

#[test]
fn paused_event_schema_admin_direct() {
    let (e, contract_id, client, admin) = setup();

    assert_eq!(client.pause(&admin), None);

    // Topics: ("paused",)  Data: (proposal_id: Option<u64>, reason: String)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "paused must have 1 topic");
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "paused"));
    let (proposal_id, reason): (Option<u64>, String) = decode(&e, data, "paused data");
    assert_eq!(proposal_id, None);
    assert_eq!(reason, admin.to_string());
}

#[test]
fn unpaused_event_schema_admin_direct() {
    let (e, contract_id, client, admin) = setup();
    client.pause(&admin);

    assert_eq!(client.unpause(&admin), None);

    // Topics: ("unpaused",)  Data: proposal_id (Option<u32>)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "unpaused must have 1 topic");
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "unpaused"));
    let proposal_id: Option<u32> = decode(&e, data, "unpaused data");
    assert_eq!(proposal_id, None);
}

// ── pause multisig: signer, threshold, proposal, approval, execution ─────

#[test]
fn pause_signer_set_event_schema() {
    let (e, contract_id, client, admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&admin, &signer, &true);

    // Topics: ("pause_signer_set", signer: Address)  Data: enabled (bool)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 2, "pause_signer_set must have 2 topics");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "pause_signer_set")
    );
    let t1: Address = decode(&e, topics.get(1).unwrap(), "pause_signer_set topic[1]");
    assert_eq!(t1, signer);
    let enabled: bool = decode(&e, data, "pause_signer_set data");
    assert!(enabled);

    client.set_pause_signer(&admin, &signer, &false);
    let (_, data) = only_event(&e, &contract_id);
    let enabled: bool = decode(&e, data, "pause_signer_set data (disable)");
    assert!(!enabled);
}

#[test]
fn pause_threshold_set_event_schema() {
    let (e, contract_id, client, admin) = setup();
    client.set_pause_signer(&admin, &Address::generate(&e), &true);
    client.set_pause_signer(&admin, &Address::generate(&e), &true);

    client.set_pause_threshold(&admin, &2);

    // Topics: ("pause_threshold_set",)  Data: threshold (u32)
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 1, "pause_threshold_set must have 1 topic");
    assert_eq!(
        topic_symbol(&e, &topics),
        Symbol::new(&e, "pause_threshold_set")
    );
    let threshold: u32 = decode(&e, data, "pause_threshold_set data");
    assert_eq!(threshold, 2);
}

#[test]
fn pause_proposal_lifecycle_event_schemas() {
    let (e, contract_id, client, admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2);

    // pause_proposed — Topics: ("pause_proposed", proposal_id: u32)  Data: action (Symbol)
    let id = client
        .pause(&s1)
        .expect("threshold > 0 must create a proposal");
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 2, "pause_proposed must have 2 topics");
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "pause_proposed"));
    let t1: u32 = decode(&e, topics.get(1).unwrap(), "pause_proposed topic[1]");
    assert_eq!(t1, id);
    let action: Symbol = decode(&e, data, "pause_proposed data");
    assert_eq!(action, Symbol::new(&e, "pause"));

    // pause_approved — Topics: ("pause_approved", proposal_id: u32)  Data: signer (Address)
    client.approve_pause_proposal(&s2, &id);
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topics.len(), 2, "pause_approved must have 2 topics");
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "pause_approved"));
    let t1: u32 = decode(&e, topics.get(1).unwrap(), "pause_approved topic[1]");
    assert_eq!(t1, id);
    let signer: Address = decode(&e, data, "pause_approved data");
    assert_eq!(signer, s2);

    // paused via proposal — Data: (Some(proposal_id as u64), "")
    client.execute_pause_proposal(&id);
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "paused"));
    let (proposal_id, reason): (Option<u64>, String) = decode(&e, data, "paused data");
    assert_eq!(proposal_id, Some(u64::from(id)));
    assert_eq!(reason, String::from_str(&e, ""));

    // unpause proposal from a non-admin signer, then unpaused via proposal.
    let unpause_id = client
        .unpause(&s1)
        .expect("signer unpause must create a proposal");
    let (_, data) = only_event(&e, &contract_id);
    let action: Symbol = decode(&e, data, "pause_proposed data");
    assert_eq!(action, Symbol::new(&e, "unpause"));
    client.approve_pause_proposal(&s2, &unpause_id);
    client.execute_pause_proposal(&unpause_id);
    let (topics, data) = only_event(&e, &contract_id);
    assert_eq!(topic_symbol(&e, &topics), Symbol::new(&e, "unpaused"));
    let proposal_id: Option<u32> = decode(&e, data, "unpaused data");
    assert_eq!(proposal_id, Some(unpause_id));
}

// ── atomicity: a rejected call publishes nothing ─────────────────────────

#[test]
fn failed_initialize_emits_no_event() {
    let (e, _contract_id, client, admin) = setup();
    assert!(client.try_initialize(&admin).is_err());
    assert_eq!(e.events().all().len(), 0);
}

#[test]
fn failed_register_emits_no_event() {
    let (e, _contract_id, client, _admin) = setup();
    let (identity, _bond) = register_one(&e, &client);

    // Duplicate identity.
    assert!(client
        .try_register(&identity, &Address::generate(&e), &true)
        .is_err());
    assert_eq!(e.events().all().len(), 0);

    // Bond without the Credence interface and no opt-out.
    assert!(client
        .try_register(&Address::generate(&e), &Address::generate(&e), &false)
        .is_err());
    assert_eq!(e.events().all().len(), 0);
}

#[test]
fn failed_lifecycle_calls_emit_no_event() {
    let (e, _contract_id, client, _admin) = setup();
    let unknown = Address::generate(&e);
    let (identity, _bond) = register_one(&e, &client);

    assert!(client.try_deactivate(&unknown).is_err());
    assert_eq!(e.events().all().len(), 0);

    assert!(client.try_reactivate(&identity).is_err()); // already active
    assert_eq!(e.events().all().len(), 0);

    assert!(client.try_remove(&unknown).is_err());
    assert_eq!(e.events().all().len(), 0);

    client.deactivate(&identity);
    assert!(client.try_deactivate(&identity).is_err()); // already deactivated
    assert_eq!(e.events().all().len(), 0);
}

#[test]
fn calls_rejected_while_paused_emit_no_event() {
    let (e, _contract_id, client, admin) = setup();
    let (identity, _bond) = register_one(&e, &client);
    client.pause(&admin);

    assert!(client
        .try_register(&Address::generate(&e), &Address::generate(&e), &true)
        .is_err());
    assert_eq!(e.events().all().len(), 0);

    assert!(client.try_deactivate(&identity).is_err());
    assert_eq!(e.events().all().len(), 0);

    assert!(client.try_transfer_admin(&Address::generate(&e)).is_err());
    assert_eq!(e.events().all().len(), 0);
}

#[test]
fn failed_pause_multisig_calls_emit_no_event() {
    let (e, _contract_id, client, admin) = setup();

    // No signers configured: threshold 2 exceeds signer count.
    assert!(client.try_set_pause_threshold(&admin, &2).is_err());
    assert_eq!(e.events().all().len(), 0);

    // Approving a proposal that does not exist.
    let signer = Address::generate(&e);
    client.set_pause_signer(&admin, &signer, &true);
    assert!(client.try_approve_pause_proposal(&signer, &42).is_err());
    assert_eq!(e.events().all().len(), 0);

    // Executing a proposal without enough approvals.
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2);
    let id = client.pause(&signer).unwrap();
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert_eq!(e.events().all().len(), 0);
}

#[test]
fn failed_register_trustless_emits_no_event() {
    let (e, _contract_id, client, _admin) = setup();
    let bond = e.register(MockBond, ());

    // Bond code hash not configured yet.
    assert!(client
        .try_register_trustless(&bond, &Address::generate(&e))
        .is_err());
    assert_eq!(e.events().all().len(), 0);

    // Configured hash does not match the caller's.
    client.set_bond_code_hash(&Bytes::from_array(&e, &[9; 32]));
    assert!(client
        .try_register_trustless(&bond, &Address::generate(&e))
        .is_err());
    assert_eq!(e.events().all().len(), 0);
}
