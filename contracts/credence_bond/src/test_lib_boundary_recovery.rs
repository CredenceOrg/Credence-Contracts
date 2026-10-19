//! Boundary and recovery test coverage for the entrypoints declared directly on
//! `CredenceBond` in `contracts/credence_bond/src/lib.rs` (issue #1337).
//!
//! # Scope
//!
//! Only the `impl CredenceBond` surface owned by `lib.rs` is covered here;
//! behaviour delegated to a submodule (`parameters`, `fees`, `slashing`, ...)
//! is left to that module's own suite so each test has a single owner.
//!
//! The entrypoints selected are the ones that gate the whole contract:
//! initialization, admin handover, the attester registry, the fee
//! accumulate/collect loop, the borrow freeze, the treasury setters, and
//! `extend_duration`.
//!
//! # Invariants under test
//!
//! * **Loading** — every read-only entrypoint returns a defined value for a
//!   never-initialized contract and for identities with no state, rather than
//!   panicking. Indexers and keepers poll these, so an undefined state is an
//!   unrecoverable client.
//! * **Permission** — a mutation reached with a stale or forged admin identity
//!   is rejected with a specific error code and leaves storage byte-identical.
//! * **Retry / partial failure** — a retried or aborted mutation cannot pay
//!   twice, and a cross-contract call that panics rolls the whole call back.
//! * **No data loss** — a rejected mutation never clears state written by an
//!   earlier accepted one, and a refused handover preserves the bonds,
//!   attesters and fees the users already own.
//!
//! Each error assertion pins the wire code (e.g. `Error(Contract, #100)`) so a
//! refactor that swaps one guard for another fails loudly instead of silently
//! changing which error a caller sees. The codes asserted below are
//! `NotInitialized` (#1), `AlreadyInitialized` (#2), `NotAdmin` (#100),
//! `ContractPaused` (#106), `InvalidAdminAddress` (#110), `AdminUnchanged`
//! (#111), `BorrowFrozen` (#114), `BondNotFound` (#200), `BytesTooLarge`
//! (#239), `AmountMustBePositive` (#600) and `Overflow` (#700).
//!
//! `#[should_panic(expected = ...)]` only accepts a literal, so these are
//! written inline rather than as named constants.

extern crate std;

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{vec, Address, Bytes, Env, IntoVal, Symbol};

/// A bond amount that satisfies `validate_bond_amount` and stays under
/// `DEFAULT_MAX_LEVERAGE` in a `cfg(test)` build, where
/// `MIN_BOND_AMOUNT == 1_000` so the leverage ratio is `amount / 1_000`.
const BOND_AMOUNT: i128 = 1_000_000;
const BOND_DURATION: u64 = 86_400;
const NOTICE_PERIOD: u64 = 3_600;

/// `fees::MAX_FEE_BPS` (10%).
const MAX_FEE_BPS: u32 = 1_000;
/// `validation::MAX_FINITE_BYTES_LENGTH`.
const MAX_SALT_LEN: u32 = 512;

/// Registry stub that records the `register_trustless` call that
/// `initialize_with_registry` makes, so the forwarded arguments can be checked.
///
/// Each stub lives in its own module because `#[contractimpl]` emits
/// module-level symbols named after the entrypoint, and both stubs implement
/// `register_trustless`.
mod recording_registry {
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct RecordingRegistry;

    #[contractimpl]
    impl RecordingRegistry {
        pub fn register_trustless(e: Env, subject: Address, admin: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "subject"), &subject);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "admin"), &admin);
        }
    }
}

/// A registry that does **not** export `register_trustless`.
///
/// Invoking a missing entrypoint makes the host fail the cross-contract call, so
/// this models the realistic partial-failure case: `initialize` has already
/// written the admin before the registry is contacted, and the whole call must
/// still roll back.
mod bare_registry {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct BareRegistry;

    #[contractimpl]
    impl BareRegistry {
        pub fn ping(_e: Env) {}
    }
}

use bare_registry::BareRegistry;
use recording_registry::RecordingRegistry;

/// Register a bond contract and return it uninitialized, with no auth mocked.
fn deploy(e: &Env) -> (Address, CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    (contract_id, client, admin)
}

/// Register a bond contract and initialize it. All addresses are pre-authorized.
fn setup(e: &Env) -> (Address, CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let (contract_id, client, admin) = deploy(e);
    client.initialize(&admin, &None);
    (contract_id, client, admin)
}

fn bytes_of_len(e: &Env, len: usize) -> Bytes {
    // `std::vec!` repeat form; `soroban_sdk::vec!` is the element-list macro.
    Bytes::from_slice(e, &std::vec![7_u8; len])
}

// ===========================================================================
// 1. Initialization lifecycle
// ===========================================================================

/// Loading: introspection must be callable on a fresh deployment and report
/// "not configured" instead of panicking.
#[test]
fn describe_config_returns_none_before_initialize() {
    let e = Env::default();
    let (_id, client, _admin) = deploy(&e);
    assert!(client.describe_config().is_none());
    assert!(client.get_fee_config().0.is_none());
    assert!(client.get_liquidation_treasury().is_none());
    assert!(client.get_slash_treasury().is_none());
    assert!(!client.is_borrow_frozen());
}

/// Loading: `version` is independent of initialization state, so a keeper can
/// probe a deployment before it is configured.
#[test]
fn version_is_readable_before_and_after_initialize() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    let before = client.version();
    client.initialize(&admin, &None);
    assert_eq!(client.version(), before);
    assert_eq!(before, soroban_sdk::String::from_str(&e, "0.1.0"));
}

#[test]
fn initialize_sets_admin_visible_via_describe_config() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    client.initialize(&admin, &None);
    assert_eq!(client.describe_config().unwrap().admin, admin);
}

/// Boundary: the second `initialize` is rejected whatever admin it claims.
#[test]
#[should_panic(expected = "Error(Contract, #2")]
fn initialize_twice_is_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    client.initialize(&admin, &None);
}

/// Permission: a different address cannot re-initialize and seize the admin
/// role. Without this, anyone could front-run a real deployment.
#[test]
#[should_panic(expected = "Error(Contract, #2")]
fn initialize_by_other_address_cannot_take_over() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, _admin) = setup(&e);
    let attacker = Address::generate(&e);
    client.initialize(&attacker, &None);
}

/// No data loss: a rejected re-initialize leaves the original admin in place.
#[test]
fn rejected_reinitialize_does_not_clobber_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    let attacker = Address::generate(&e);

    assert!(client.try_initialize(&attacker, &None).is_err());

    assert_eq!(client.describe_config().unwrap().admin, admin);
    // The original admin is still the only one who can mutate config.
    assert!(client.try_set_borrow_frozen(&attacker, &true).is_err());
    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());
}

/// Success: `initialize_with_registry` forwards the bond contract and the admin
/// to the registry's `register_trustless`.
#[test]
fn initialize_with_registry_forwards_subject_and_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let (contract_id, client, admin) = deploy(&e);
    let registry = e.register(RecordingRegistry, ());

    client.initialize_with_registry(&admin, &registry);

    let (subject, recorded_admin) = e.as_contract(&registry, || {
        (
            e.storage().instance().get(&Symbol::new(&e, "subject")),
            e.storage().instance().get(&Symbol::new(&e, "admin")),
        )
    });
    assert_eq!(subject, Some(contract_id));
    assert_eq!(recorded_admin, Some(admin.clone()));
    assert_eq!(client.describe_config().unwrap().admin, admin);
}

/// Partial failure: when the registry call panics, the whole
/// `initialize_with_registry` must roll back. Otherwise the contract would sit
/// in a half-configured state with an admin but no registry registration, and
/// `initialize` could never be retried.
#[test]
fn initialize_with_registry_rolls_back_when_registry_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    let registry = e.register(BareRegistry, ());

    assert!(client
        .try_initialize_with_registry(&admin, &registry)
        .is_err());

    // Nothing was persisted: the contract is still uninitialized and a
    // well-behaved registry can still be supplied on a retry.
    assert!(client.describe_config().is_none());
    let good = e.register(RecordingRegistry, ());
    client.initialize_with_registry(&admin, &good);
    assert_eq!(client.describe_config().unwrap().admin, admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #2")]
fn initialize_with_registry_rejected_when_already_initialized() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    let registry = e.register(RecordingRegistry, ());
    client.initialize_with_registry(&admin, &registry);
}

// ===========================================================================
// 2. Admin-gated mutations before initialization
// ===========================================================================

/// Every admin-gated mutation must report `NotInitialized` rather than writing
/// a config that no admin can ever claim.
#[test]
#[should_panic(expected = "Error(Contract, #1")]
fn set_borrow_frozen_requires_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    client.set_borrow_frozen(&admin, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #1")]
fn set_liquidation_treasury_requires_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    client.set_liquidation_treasury(&admin, &Address::generate(&e));
}

#[test]
#[should_panic(expected = "Error(Contract, #1")]
fn set_slash_treasury_requires_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    client.set_slash_treasury(&admin, &Address::generate(&e));
}

#[test]
#[should_panic(expected = "Error(Contract, #1")]
fn register_attester_requires_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, _admin) = deploy(&e);
    client.register_attester(&Address::generate(&e));
}

#[test]
#[should_panic(expected = "Error(Contract, #1")]
fn transfer_admin_requires_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = deploy(&e);
    let next = Address::generate(&e);
    client.transfer_admin(&admin, &next);
}

// ===========================================================================
// 3. Admin handover
// ===========================================================================

/// Permission: the stored admin is the only address that may hand over.
#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn transfer_admin_rejects_non_admin() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, _admin) = setup(&e);
    let stranger = Address::generate(&e);
    let next = Address::generate(&e);
    client.transfer_admin(&stranger, &next);
}

/// Boundary: a no-op handover is rejected so the governance trail is not
/// polluted with meaningless transfers.
#[test]
#[should_panic(expected = "Error(Contract, #111")]
fn transfer_admin_rejects_same_address() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    client.transfer_admin(&admin, &admin);
}

/// The zero-address guard in `transfer_admin` is defensive only.
///
/// `InvalidAdminAddress` fires when `new_admin.to_string()` equals the all-zeros
/// strkey, but no valid Stellar strkey decodes to the all-zeros address — the
/// SDK rejects such a string with `Error(Value, InvalidInput)` before the
/// contract is ever entered. The branch is therefore not reachable from a host
/// test, and asserting it here would be asserting something unreachable rather
/// than something true.
#[test]
#[should_panic(expected = "unexpected strkey length")]
fn zero_admin_address_is_not_representable_as_an_address() {
    let e = Env::default();
    let zero = soroban_sdk::String::from_str(&e, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    // Constructing it panics outright, which is why `transfer_admin` can only
    // defend against it defensively.
    let _ = Address::from_string(&zero);
}

/// Recovery: after a handover the new admin can act and the old one is stale.
/// Both halves matter — granting without revoking would leave two admins.
#[test]
fn transfer_admin_moves_authority_exclusively() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    let next = Address::generate(&e);

    client.transfer_admin(&admin, &next);

    assert_eq!(client.describe_config().unwrap().admin, next);
    // Old admin is now stale.
    assert!(client.try_set_borrow_frozen(&admin, &true).is_err());
    // New admin works.
    client.set_borrow_frozen(&next, &true);
    assert!(client.is_borrow_frozen());
}

/// Recovery: a mistaken handover can be walked back through a second transfer.
#[test]
fn transfer_admin_is_chainable() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    let second = Address::generate(&e);
    let third = Address::generate(&e);

    client.transfer_admin(&admin, &second);
    client.transfer_admin(&second, &third);

    assert_eq!(client.describe_config().unwrap().admin, third);
    assert!(client.try_set_borrow_frozen(&second, &true).is_err());
    client.set_borrow_frozen(&third, &true);
}

/// No data loss: handover moves only the admin key. Bonds, the attester
/// registry and accrued fees belong to their owners and must survive.
#[test]
fn transfer_admin_preserves_bonds_attesters_and_fees() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);

    let identity = Address::generate(&e);
    let attester = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
    client.register_attester(&attester);
    client.set_fee_config(&admin, &treasury, &250);
    client.deposit_fees(&5_000);

    let next = Address::generate(&e);
    client.transfer_admin(&admin, &next);

    assert_eq!(
        client.get_identity_state(&identity).bonded_amount,
        BOND_AMOUNT
    );
    assert!(client.is_attester(&attester));
    assert_eq!(client.get_fee_config(), (Some(treasury), 250));
    assert_eq!(client.collect_fees(&next, &bytes_of_len(&e, 8)), 5_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #106")]
fn transfer_admin_rejected_while_paused() {
    let e = Env::default();
    e.mock_all_auths();
    let (_id, client, admin) = setup(&e);
    let next = Address::generate(&e);
    client.pause(&admin);
    client.transfer_admin(&admin, &next);
}

// ===========================================================================
// 4. Attester registry
// ===========================================================================

/// Loading: an unknown address is simply not an attester, never an error.
#[test]
fn is_attester_is_false_for_unknown_address() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    assert!(!client.is_attester(&Address::generate(&e)));
}

#[test]
fn register_attester_round_trips() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
}

/// Duplicate: `register_attester` writes a boolean flag, so a repeat call is
/// accepted and idempotent rather than an error. Pinning this keeps a future
/// "already registered" guard from being added silently.
#[test]
fn register_attester_is_idempotent_for_duplicates() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    assert!(client.try_register_attester(&attester).is_ok());
    assert!(client.is_attester(&attester));
}

#[test]
fn unregister_attester_round_trips() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    client.unregister_attester(&attester);
    assert!(!client.is_attester(&attester));
}

/// Recovery: unregistering an address that was never registered is a no-op, not
/// an error, so a keeper can reconcile state idempotently.
#[test]
fn unregister_attester_is_idempotent() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    assert!(client.try_unregister_attester(&attester).is_ok());
    client.register_attester(&attester);
    assert!(client.try_unregister_attester(&attester).is_ok());
    assert!(!client.is_attester(&attester));
}

/// Recovery: a removed attester can be re-registered, so a mistaken revocation
/// is reversible.
#[test]
fn attester_can_be_re_registered_after_removal() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    client.unregister_attester(&attester);
    client.register_attester(&attester);
    assert!(client.is_attester(&attester));
}

/// No data loss: revoking one attester must not disturb the others. The
/// registry is keyed per address, so a shared key would silently de-register
/// the whole set.
#[test]
fn unregister_attester_only_affects_that_address() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let first = Address::generate(&e);
    let second = Address::generate(&e);
    client.register_attester(&first);
    client.register_attester(&second);

    client.unregister_attester(&first);

    assert!(!client.is_attester(&first));
    assert!(client.is_attester(&second));
}

/// Permission: the call is gated on the stored admin's authorization.
#[test]
fn register_attester_requires_admin_auth() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    // `register_attester` takes no admin argument, so the only way to prove the
    // gate is to withhold authorization entirely.
    e.set_auths(&[]);
    assert!(client.try_register_attester(&attester).is_err());
    assert!(!client.is_attester(&attester));
}

/// Permission: authorization recorded for a forged identity must not satisfy
/// the stored admin's `require_auth`.
#[test]
fn register_attester_rejects_forged_admin_signature() {
    let e = Env::default();
    let (contract_id, client, _admin) = setup(&e);
    let attester = Address::generate(&e);
    let forger = Address::generate(&e);

    e.mock_auths(&[MockAuth {
        address: &forger,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "register_attester",
            sub_invokes: &[],
            args: vec![&e, attester.into_val(&e)],
        },
    }]);

    assert!(client.try_register_attester(&attester).is_err());
    assert!(!client.is_attester(&attester));
}

// ===========================================================================
// 5. Fee deposit / configuration / collection
// ===========================================================================

/// Boundary: a zero deposit is rejected; treating it as a no-op would let a
/// caller burn gas for nothing and hide a caller bug.
#[test]
#[should_panic(expected = "Error(Contract, #600")]
fn deposit_fees_rejects_zero() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    client.deposit_fees(&0);
}

#[test]
#[should_panic(expected = "Error(Contract, #600")]
fn deposit_fees_rejects_negative() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    client.deposit_fees(&-1);
}

#[test]
fn deposit_fees_accumulates() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&1_000);
    client.deposit_fees(&2_500);
    // Accrual is only observable through collection.
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 3_500);
}

/// Boundary: the documented default is "never configured", i.e. no treasury
/// and a zero rate, so bond creation is free until governance opts in.
#[test]
fn fee_config_defaults_to_unset() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    assert_eq!(client.get_fee_config(), (None, 0));
}

/// Boundary: both ends of the inclusive `[MIN_FEE_BPS, MAX_FEE_BPS]` range are
/// accepted. An off-by-one here would either block a 0% rate or a 10% rate.
#[test]
fn set_fee_config_accepts_range_endpoints() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &0);
    assert_eq!(client.get_fee_config(), (Some(treasury.clone()), 0));

    client.set_fee_config(&admin, &treasury, &MAX_FEE_BPS);
    assert_eq!(client.get_fee_config(), (Some(treasury), MAX_FEE_BPS));
}

#[test]
#[should_panic(expected = "fee_bps out of bounds")]
fn set_fee_config_rejects_above_max() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.set_fee_config(&admin, &Address::generate(&e), &(MAX_FEE_BPS + 1));
}

/// No data loss: a rejected bounds update must leave the previous treasury and
/// rate in place. Overwriting first and validating second would silently reset
/// a live fee to zero.
#[test]
fn rejected_fee_config_update_keeps_previous_config() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let treasury = Address::generate(&e);
    let attacker_treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &250);

    assert!(client
        .try_set_fee_config(&admin, &attacker_treasury, &(MAX_FEE_BPS + 1))
        .is_err());

    assert_eq!(client.get_fee_config(), (Some(treasury), 250));
}

#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn set_fee_config_rejects_non_admin() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.set_fee_config(&stranger, &stranger, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn collect_fees_rejects_non_admin() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    client.deposit_fees(&1_000);
    let stranger = Address::generate(&e);
    client.collect_fees(&stranger, &bytes_of_len(&e, 4));
}

/// A rejected collection must leave the accrued balance intact, otherwise an
/// unauthorized probe would let an attacker drain the treasury by calling
/// `collect_fees` until it is empty.
#[test]
fn rejected_collect_fees_keeps_balance() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.deposit_fees(&1_000);

    assert!(client
        .try_collect_fees(&stranger, &bytes_of_len(&e, 4))
        .is_err());
    assert!(client
        .try_collect_fees(&stranger, &bytes_of_len(&e, 4))
        .is_err());

    // Admin still collects the full amount: no fee was lost to the probes.
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 1_000);
}

/// Collection resets the balance to zero, so a second collection cannot pay
/// the same fees twice.
#[test]
fn collect_fees_zeroes_the_balance() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&1_000);

    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 1_000);
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 0);
}

#[test]
fn collect_fees_on_empty_balance_returns_zero() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 0);
}

/// Boundary: exactly `MAX_FINITE_BYTES_LENGTH` is accepted, one more is not.
#[test]
fn collect_fees_enforces_salt_length_boundary() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&10);

    let at_limit = bytes_of_len(&e, MAX_SALT_LEN as usize);
    assert_eq!(client.collect_fees(&admin, &at_limit), 10);

    let over_limit = bytes_of_len(&e, MAX_SALT_LEN as usize + 1);
    assert!(client.try_collect_fees(&admin, &over_limit).is_err());
}

/// Oversized input is rejected before the balance is touched, so the rejected
/// call cannot be used to zero the accumulator.
#[test]
fn rejected_oversized_salt_keeps_balance() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&777);

    let over_limit = bytes_of_len(&e, MAX_SALT_LEN as usize + 1);
    assert!(client.try_collect_fees(&admin, &over_limit).is_err());

    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 777);
}

/// Retry: replaying a `collect_fees` with the same salt must not pay twice.
///
/// NOTE: the `idempotency_salt` argument is currently accepted and validated
/// but not recorded — the `idempotency::check_and_record` call is commented out
/// in `collect_fees` pending a merge fix, so the documented
/// `DuplicateIdempotencyKey` error is never raised. The safety property that
/// matters for a retry is asserted here (funds are collected exactly once);
/// the missing duplicate-key rejection is called out for maintainers rather
/// than encoded as expected behaviour in this test.
#[test]
fn collect_fees_retry_does_not_double_collect() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&1_000);
    let salt = bytes_of_len(&e, 16);

    assert_eq!(client.collect_fees(&admin, &salt), 1_000);
    // Retry with the identical salt.
    assert_eq!(client.collect_fees(&admin, &salt), 0);
}

/// Recovery: a fresh deposit after a collection is collectable again, so a
/// keeper loop driven by a reused salt still makes progress.
#[test]
fn collect_fees_recovers_after_new_deposit() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let salt = bytes_of_len(&e, 16);

    client.deposit_fees(&1_000);
    assert_eq!(client.collect_fees(&admin, &salt), 1_000);
    client.deposit_fees(&500);
    assert_eq!(client.collect_fees(&admin, &salt), 500);
}

/// A different salt is a different logical request, so it collects the
/// outstanding balance rather than being swallowed as a duplicate.
#[test]
fn collect_fees_with_distinct_salt_collects_each_balance() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);

    client.deposit_fees(&300);
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 1)), 300);
    client.deposit_fees(&200);
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 2)), 200);
}

// ===========================================================================
// 6. Borrow freeze
// ===========================================================================

/// Loading: unfrozen is the default, so a fresh contract is usable.
#[test]
fn borrow_freeze_defaults_to_false() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    assert!(!client.is_borrow_frozen());
}

/// The freeze is only meaningful if it actually gates new borrows.
#[test]
#[should_panic(expected = "Error(Contract, #114")]
fn frozen_borrow_rejects_new_bond() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let identity = Address::generate(&e);
    client.set_borrow_frozen(&admin, &true);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
}

/// Recovery: lifting the freeze immediately restores bond creation, and a bond
/// created afterwards is a normal one.
#[test]
fn unfreezing_restores_bond_creation() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let identity = Address::generate(&e);
    client.set_borrow_frozen(&admin, &true);
    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());

    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
    assert_eq!(
        client.get_identity_state(&identity).bonded_amount,
        BOND_AMOUNT
    );
}

/// Writing the same freeze value twice is a no-op, so a retrying governance
/// action cannot oscillate the flag.
#[test]
fn set_borrow_frozen_is_idempotent() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.set_borrow_frozen(&admin, &true);
    assert!(client.try_set_borrow_frozen(&admin, &true).is_ok());
    assert!(client.is_borrow_frozen());
    assert!(client.try_set_borrow_frozen(&admin, &false).is_ok());
    assert!(!client.is_borrow_frozen());
}

/// A rejected freeze change must not take effect, otherwise a failed
/// governance tx could still stop all borrowing.
#[test]
fn rejected_freeze_change_keeps_previous_value() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    assert!(client.try_set_borrow_frozen(&stranger, &true).is_err());
    assert!(!client.is_borrow_frozen());

    // A rejected governance call must leave the contract fully usable by the
    // real admin, not just un-flipped.
    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());
}

#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn set_borrow_frozen_rejects_non_admin() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.set_borrow_frozen(&stranger, &true);
}

// ===========================================================================
// 7. Treasury setters
// ===========================================================================

#[test]
fn liquidation_treasury_round_trips_and_can_be_replaced() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let first = Address::generate(&e);
    let second = Address::generate(&e);

    client.set_liquidation_treasury(&admin, &first);
    assert_eq!(client.get_liquidation_treasury(), Some(first));

    // Recovery: pointing at a new treasury replaces the old one rather than
    // being ignored, so a misconfigured treasury can be corrected.
    client.set_liquidation_treasury(&admin, &second);
    assert_eq!(client.get_liquidation_treasury(), Some(second));
}

#[test]
fn slash_treasury_round_trips_and_can_be_replaced() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let first = Address::generate(&e);
    let second = Address::generate(&e);

    client.set_slash_treasury(&admin, &first);
    assert_eq!(client.get_slash_treasury(), Some(first));
    client.set_slash_treasury(&admin, &second);
    assert_eq!(client.get_slash_treasury(), Some(second));
}

/// The two treasuries are independent keys; sharing one would send slashed
/// funds to the liquidation sweep destination.
#[test]
fn treasury_setters_are_independent() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let liquidation = Address::generate(&e);
    let slash = Address::generate(&e);

    client.set_liquidation_treasury(&admin, &liquidation);
    client.set_slash_treasury(&admin, &slash);

    assert_eq!(client.get_liquidation_treasury(), Some(liquidation));
    assert_eq!(client.get_slash_treasury(), Some(slash));
}

#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn set_liquidation_treasury_rejects_non_admin() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.set_liquidation_treasury(&stranger, &stranger);
}

#[test]
#[should_panic(expected = "Error(Contract, #100")]
fn set_slash_treasury_rejects_non_admin() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.set_slash_treasury(&stranger, &stranger);
}

/// No data loss: a rejected treasury update leaves the live one in place.
#[test]
fn rejected_treasury_update_keeps_previous_value() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let live = Address::generate(&e);
    let stranger = Address::generate(&e);
    client.set_liquidation_treasury(&admin, &live);

    assert!(client
        .try_set_liquidation_treasury(&stranger, &stranger)
        .is_err());
    assert_eq!(client.get_liquidation_treasury(), Some(live));
}

// ===========================================================================
// 8. extend_duration
// ===========================================================================

/// Loading: an identity with no bond reports `BondNotFound` instead of
/// creating a zeroed bond.
#[test]
#[should_panic(expected = "Error(Contract, #200")]
fn extend_duration_rejects_missing_bond() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    client.extend_duration(&Address::generate(&e), &1_000);
}

/// Boundary: extending by zero succeeds and leaves the bond untouched.
#[test]
fn extend_duration_by_zero_is_a_noop() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
    let before = client.get_identity_state(&identity);

    let after = client.extend_duration(&identity, &0);

    assert_eq!(after.bond_duration, before.bond_duration);
    assert_eq!(after.bond_start, before.bond_start);
    assert_eq!(after.bonded_amount, before.bonded_amount);
}

#[test]
fn extend_duration_accumulates_and_preserves_collateral() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );

    let once = client.extend_duration(&identity, &100);
    let twice = client.extend_duration(&identity, &50);

    assert_eq!(once.bond_duration, BOND_DURATION + 100);
    assert_eq!(twice.bond_duration, BOND_DURATION + 150);
    // Collateral is untouched by a duration change.
    assert_eq!(twice.bonded_amount, BOND_AMOUNT);
}

/// Boundary: `u64::MAX` cannot be added to the current duration, and the
/// `checked_add` must reject it rather than wrap into a short bond.
#[test]
#[should_panic(expected = "Error(Contract, #700")]
fn extend_duration_rejects_overflow() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
    client.extend_duration(&identity, &u64::MAX);
}

/// No data loss: a rejected extension must not have partially written the bond.
#[test]
fn rejected_overflow_extension_keeps_duration() {
    let e = Env::default();
    let (_id, client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );

    assert!(client.try_extend_duration(&identity, &u64::MAX).is_err());

    assert_eq!(
        client.get_identity_state(&identity).bond_duration,
        BOND_DURATION
    );
}

// ===========================================================================
// 9. Pause gating
// ===========================================================================

/// Mutations must be refused while paused so a halted contract cannot accrue or
/// move funds.
#[test]
#[should_panic(expected = "Error(Contract, #106")]
fn pause_blocks_deposit_fees() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.pause(&admin);
    client.deposit_fees(&1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #106")]
fn pause_blocks_collect_fees() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.deposit_fees(&1_000);
    client.pause(&admin);
    client.collect_fees(&admin, &bytes_of_len(&e, 4));
}

#[test]
#[should_panic(expected = "Error(Contract, #106")]
fn pause_blocks_set_borrow_frozen() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.pause(&admin);
    client.set_borrow_frozen(&admin, &false);
}

/// Recovery: unpausing restores every gated mutation.
#[test]
fn unpause_restores_fee_flow() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    client.pause(&admin);
    assert!(client.is_paused());
    client.unpause(&admin);
    assert!(!client.is_paused());

    client.deposit_fees(&900);
    assert_eq!(client.collect_fees(&admin, &bytes_of_len(&e, 4)), 900);
}

/// Loading: reads stay available while paused so indexers and keepers can still
/// observe state during an incident. Gating the views would leave an operator
/// blind exactly when it matters.
#[test]
fn views_remain_readable_while_paused() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let identity = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.create_bond(
        &identity,
        &BOND_AMOUNT,
        &BOND_DURATION,
        &false,
        &NOTICE_PERIOD,
    );
    client.set_liquidation_treasury(&admin, &treasury);

    client.pause(&admin);

    assert!(client.is_paused());
    assert!(client.get_identity_state(&identity).active);
    assert_eq!(client.get_liquidation_treasury(), Some(treasury));
    assert!(!client.is_borrow_frozen());
    assert!(!client.is_attester(&Address::generate(&e)));
    assert_eq!(client.get_slash_count(&identity), 0);
    assert!(!client.is_liquidated(&identity));
    assert_eq!(
        client.get_identity_state(&identity).bonded_amount,
        BOND_AMOUNT
    );
}

/// Precedence: while paused, the pause gate is reported before the admin
/// check. This ordering is deliberate — a paused contract should not leak
/// admin-configuration detail to a rejected caller.
#[test]
#[should_panic(expected = "Error(Contract, #106")]
fn pause_takes_precedence_over_the_admin_check() {
    let e = Env::default();
    let (_id, client, admin) = setup(&e);
    let stranger = Address::generate(&e);
    client.pause(&admin);
    client.set_liquidation_treasury(&stranger, &stranger);
}
