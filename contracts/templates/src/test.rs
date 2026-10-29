use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{Env, TryFromVal};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(e: &Env) -> (Address, Address, TemplateContractClient<'_>) {
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

fn record_is_stored(e: &Env, contract_id: &Address, owner: &Address) -> bool {
    e.as_contract(contract_id, || {
        e.storage().instance().has(&DataKey::Record(owner.clone()))
    })
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
fn test_initialize_requires_proposed_admin_auth() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TemplateContract, ());
    let client = TemplateContractClient::new(&e, &contract_id);

    client.initialize(&admin);

    assert!(e.auths().iter().any(|(address, _)| address == &admin));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
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

    client.set_record(&owner, &42, &0);

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
    client.set_record(&owner, &10, &0);
    advance_time(&e, 50);
    client.set_record(&owner, &99, &0);

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
    client.set_record(&owner, &1, &0);

    let auths = e.auths();
    // At least one auth entry must be for the admin address.
    assert!(auths.iter().any(|(addr, _)| addr == &admin));
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
    client.set_record(&owner, &7, &0);
    assert!(client.has_record(&owner));
}

#[test]
fn test_get_record_returns_typed_error_when_missing() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    let err = client.try_get_record(&owner).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::BondNotFound as u32)
    );
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
    client.set_record(&owner, &5, &0);
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
fn test_get_admin_returns_typed_error_before_init() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(TemplateContract, ());
    let client = TemplateContractClient::new(&e, &contract_id);
    let err = client.try_get_admin().unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::NotInitialized as u32)
    );
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
    client.set_record(&owner, &1, &0);
    let t1 = client.get_record(&owner).updated_at;

    advance_time(&e, 500);
    client.set_record(&owner, &2, &0);
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
    client.set_record(&owner, &-100, &0);
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

    client.set_record(&a, &10, &0);
    client.set_record(&b, &20, &0);

    assert_eq!(client.get_record(&a).value, 10);
    assert_eq!(client.get_record(&b).value, 20);

    client.remove_record(&a);
    assert!(!client.has_record(&a));
    assert!(client.has_record(&b));
}

// ---------------------------------------------------------------------------
// Expiry
// ---------------------------------------------------------------------------

#[test]
fn test_expiry_pattern() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, contract_id, client) = setup(&e);

    let owner = Address::generate(&e);
    let now = e.ledger().timestamp();

    // Set a record that expires in 100 seconds
    client.set_record(&owner, &100, &(now + 100));

    assert!(client.has_record(&owner));
    assert!(!client.is_expired(&owner));

    // Advance exactly to expiry
    advance_time(&e, 100);

    assert!(client.is_expired(&owner));
    assert!(!client.has_record(&owner)); // read-only; expired records are not active
    assert!(record_is_stored(&e, &contract_id, &owner));

    client.purge_expired(&owner);

    // Now it's truly gone
    assert!(!client.is_expired(&owner));
    assert!(!record_is_stored(&e, &contract_id, &owner));
}

#[test]
fn test_get_expired_record_returns_typed_error_without_purging() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, contract_id, client) = setup(&e);

    let owner = Address::generate(&e);
    let now = e.ledger().timestamp();
    client.set_record(&owner, &100, &(now + 10));

    advance_time(&e, 10);
    let err = client.try_get_record(&owner).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::SignatureExpired as u32)
    );
    assert!(record_is_stored(&e, &contract_id, &owner));
}

// ===========================================================================
// Boundary + recovery coverage (issue #1389)
// ===========================================================================

// ---------------------------------------------------------------------------
// Value boundaries (i128)
// ---------------------------------------------------------------------------

#[test]
fn test_set_record_accepts_zero_value() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &0, &0);
    assert_eq!(client.get_record(&owner).value, 0);
    assert!(client.has_record(&owner));
}

#[test]
fn test_set_record_accepts_i128_max() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &i128::MAX, &0);
    assert_eq!(client.get_record(&owner).value, i128::MAX);
}

#[test]
fn test_set_record_accepts_i128_min() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &i128::MIN, &0);
    assert_eq!(client.get_record(&owner).value, i128::MIN);
}

// ---------------------------------------------------------------------------
// expires_at boundaries
// ---------------------------------------------------------------------------

#[test]
fn test_expires_at_zero_never_expires() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    client.set_record(&owner, &1, &0); // 0 = never expires

    advance_time(&e, 1_000_000);
    assert!(!client.is_expired(&owner));
    assert!(client.has_record(&owner));
    assert_eq!(client.get_record(&owner).value, 1);
}

#[test]
fn test_record_not_expired_one_second_before_deadline() {
    let e = Env::default();
    e.mock_all_auths();
    let (_, _, client) = setup(&e);

    let owner = Address::generate(&e);
    let now = e.ledger().timestamp();
    client.set_record(&owner, &7, &(now + 10));

    advance_time(&e, 9);
    assert!(!client.is_expired(&owner));
    assert!(client.has_record(&owner));
    assert_eq!(client.get_record(&owner).value, 7);
}
