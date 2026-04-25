use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(e: &Env) -> (Address, Address, TemplateContractClient) {
    let admin = Address::generate(e);
    let contract_id = e.register(TemplateContract, ());
    let client = TemplateContractClient::new(e, &contract_id);
    client.initialize(&admin);
    (admin, contract_id, client)
}

fn advance_time(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: e.ledger().sequence() + 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_sets_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let (admin, _, client) = setup(&e);
    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let (admin, _, client) = setup(&e);
    client.initialize(&admin); // second call must panic
}

// ---------------------------------------------------------------------------
// set_record
// ---------------------------------------------------------------------------

#[test]
fn test_set_record_stores_value_and_timestamp() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    advance_time(&e, 100);

    client.set_record(&owner, &42);

    let rec = client.get_record(&owner);
    assert_eq!(rec.value, 42);
    assert_eq!(rec.updated_at, e.ledger().timestamp());
}

#[test]
fn test_set_record_overwrites_previous() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &10);
    advance_time(&e, 50);
    client.set_record(&owner, &99);

    assert_eq!(client.get_record(&owner).value, 99);
}

#[test]
fn test_set_record_requires_admin_auth() {
    let e = Env::default();
    // Do NOT mock_all_auths — let the SDK enforce auth.
    let admin = Address::generate(&e);
    let contract_id = e.register(TemplateContract, ());
    let client = TemplateContractClient::new(&e, &contract_id);

    // initialize needs auth for the event publish path; mock just for setup.
    e.mock_all_auths();
    client.initialize(&admin);

    // Now clear mocks and verify that set_record checks admin auth.
    // The easiest way in the Soroban test harness is to confirm the auth
    // requirement is present by inspecting auths after a mocked call.
    let owner = Address::generate(&e);
    client.set_record(&owner, &1);

    let auths = e.auths();
    // At least one auth entry must be for the admin address.
    assert!(auths.iter().any(|(addr, _)| addr == admin));
}

// ---------------------------------------------------------------------------
// has_record / get_record
// ---------------------------------------------------------------------------

#[test]
fn test_has_record_false_before_set() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    assert!(!client.has_record(&owner));
}

#[test]
fn test_has_record_true_after_set() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &7);
    assert!(client.has_record(&owner));
}

#[test]
#[should_panic(expected = "record not found")]
fn test_get_record_panics_when_missing() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.get_record(&owner); // must panic
}

// ---------------------------------------------------------------------------
// remove_record
// ---------------------------------------------------------------------------

#[test]
fn test_remove_record_clears_entry() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &5);
    assert!(client.has_record(&owner));

    client.remove_record(&owner);
    assert!(!client.has_record(&owner));
}

#[test]
fn test_remove_nonexistent_record_is_noop() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    // Should not panic — removing a key that doesn't exist is a no-op.
    client.remove_record(&owner);
    assert!(!client.has_record(&owner));
}

// ---------------------------------------------------------------------------
// get_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "not initialized")]
fn test_get_admin_panics_before_init() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(TemplateContract, ());
    let client = TemplateContractClient::new(&e, &contract_id);
    client.get_admin(); // must panic
}

// ---------------------------------------------------------------------------
// Timestamp advancement
// ---------------------------------------------------------------------------

#[test]
fn test_updated_at_reflects_ledger_time() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);

    advance_time(&e, 1_000);
    client.set_record(&owner, &1);
    let t1 = client.get_record(&owner).updated_at;

    advance_time(&e, 500);
    client.set_record(&owner, &2);
    let t2 = client.get_record(&owner).updated_at;

    assert!(t2 > t1);
}

// ---------------------------------------------------------------------------
// Negative values
// ---------------------------------------------------------------------------

#[test]
fn test_set_record_accepts_negative_value() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &-100);
    assert_eq!(client.get_record(&owner).value, -100);
}

// ---------------------------------------------------------------------------
// Multiple independent owners
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_owners_are_independent() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let a = Address::generate(&e);
    let b = Address::generate(&e);

    client.set_record(&a, &10);
    client.set_record(&b, &20);

    assert_eq!(client.get_record(&a).value, 10);
    assert_eq!(client.get_record(&b).value, 20);

    client.remove_record(&a);
    assert!(!client.has_record(&a));
    assert!(client.has_record(&b));
}
