//! Complete lifecycle and state-transition test coverage for `DisputeResolutionContract`.
//!
//! State Transition Table:
//! ┌───────────────┬────────────────────────────┬────────────────────────┬──────────────────────┐
//! │ Current State │ Action                     │ Expected Result        │ Resulting State      │
//! ├───────────────┼────────────────────────────┼────────────────────────┼──────────────────────┤
//! │ Nonexistent   │ get_dispute                │ Err(DisputeNotFound)   │ Nonexistent          │
//! │ Nonexistent   │ resolve (resolver)         │ Err(DisputeNotFound)   │ Nonexistent          │
//! │ Nonexistent   │ resolve (non-resolver)     │ Err(DisputeNotFound)   │ Nonexistent          │
//! │ Nonexistent   │ close (resolver)           │ Err(DisputeNotFound)   │ Nonexistent          │
//! │ Nonexistent   │ close (non-resolver)       │ Err(DisputeNotFound)   │ Nonexistent          │
//! ├───────────────┼────────────────────────────┼────────────────────────┼──────────────────────┤
//! │ Open          │ get_dispute                │ Ok(DisputeStatus::Open)│ Open                 │
//! │ Open          │ resolve (resolver)         │ Ok(())                 │ Resolved             │
//! │ Open          │ resolve (non-resolver)     │ Err(Unauthorized)      │ Open                 │
//! │ Open          │ close (resolver)           │ Ok(())                 │ Closed               │
//! │ Open          │ close (non-resolver)       │ Err(Unauthorized)      │ Open                 │
//! ├───────────────┼────────────────────────────┼────────────────────────┼──────────────────────┤
//! │ Resolved      │ get_dispute                │ Ok(Resolved)           │ Resolved             │
//! │ Resolved      │ resolve (resolver)         │ Err(AlreadyClosed)     │ Resolved             │
//! │ Resolved      │ resolve (non-resolver)     │ Err(Unauthorized)      │ Resolved             │
//! │ Resolved      │ close (resolver)           │ Ok(())                 │ Closed               │
//! │ Resolved      │ close (non-resolver)       │ Err(Unauthorized)      │ Resolved             │
//! ├───────────────┼────────────────────────────┼────────────────────────┼──────────────────────┤
//! │ Closed        │ get_dispute                │ Ok(DisputeStatus::Clo.)│ Closed               │
//! │ Closed        │ resolve (resolver)         │ Err(AlreadyClosed)     │ Closed               │
//! │ Closed        │ resolve (non-resolver)     │ Err(Unauthorized)      │ Closed               │
//! │ Closed        │ close (resolver)           │ Err(AlreadyClosed)     │ Closed               │
//! │ Closed        │ close (non-resolver)       │ Err(Unauthorized)      │ Closed               │
//! └───────────────┴────────────────────────────┴────────────────────────┴──────────────────────┘

#![allow(deprecated)]

extern crate std;

use crate::error::DisputeError;
use crate::{DisputeResolutionContract, DisputeStatus};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn register(e: &Env) -> Address {
    e.register_contract(None, DisputeResolutionContract)
}

struct TestFixture {
    contract_id: Address,
    creator: Address,
    resolver: Address,
    id: u64,
}

fn setup_open(e: &Env) -> TestFixture {
    let contract_id = register(e);
    let creator = Address::generate(e);
    let resolver = Address::generate(e);
    let id = e.as_contract(&contract_id, || {
        DisputeResolutionContract::create_dispute(e.clone(), creator.clone(), resolver.clone())
    });
    TestFixture {
        contract_id,
        creator,
        resolver,
        id,
    }
}

fn setup_resolved(e: &Env) -> TestFixture {
    let fixture = setup_open(e);
    e.as_contract(&fixture.contract_id, || {
        DisputeResolutionContract::resolve(e.clone(), fixture.resolver.clone(), fixture.id)
            .expect("resolve failed during setup");
    });
    fixture
}

fn setup_closed(e: &Env) -> TestFixture {
    let fixture = setup_open(e);
    e.as_contract(&fixture.contract_id, || {
        DisputeResolutionContract::close(e.clone(), fixture.resolver.clone(), fixture.id)
            .expect("close failed during setup");
    });
    fixture
}

// ===========================================================================
// Nonexistent State Transitions (Missing IDs)
// ===========================================================================

#[test]
fn test_nonexistent_get_dispute_returns_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let missing_id = 999_u64;
        let res = DisputeResolutionContract::get_dispute(e.clone(), missing_id);
        assert_eq!(res, Err(DisputeError::DisputeNotFound));
    });
}

#[test]
fn test_nonexistent_resolve_by_resolver_returns_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let resolver = Address::generate(&e);
        let missing_id = 999_u64;
        let res = DisputeResolutionContract::resolve(e.clone(), resolver, missing_id);
        assert_eq!(res, Err(DisputeError::DisputeNotFound));
    });
}

#[test]
fn test_nonexistent_resolve_by_non_resolver_returns_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let stranger = Address::generate(&e);
        let missing_id = 999_u64;
        let res = DisputeResolutionContract::resolve(e.clone(), stranger, missing_id);
        assert_eq!(res, Err(DisputeError::DisputeNotFound));
    });
}

#[test]
fn test_nonexistent_close_by_resolver_returns_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let resolver = Address::generate(&e);
        let missing_id = 999_u64;
        let res = DisputeResolutionContract::close(e.clone(), resolver, missing_id);
        assert_eq!(res, Err(DisputeError::DisputeNotFound));
    });
}

#[test]
fn test_nonexistent_close_by_non_resolver_returns_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let stranger = Address::generate(&e);
        let missing_id = 999_u64;
        let res = DisputeResolutionContract::close(e.clone(), stranger, missing_id);
        assert_eq!(res, Err(DisputeError::DisputeNotFound));
    });
}

// ===========================================================================
// Open State Transitions
// ===========================================================================

#[test]
fn test_open_get_dispute_returns_open_dispute() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_open(&e);

    e.as_contract(&fixture.contract_id, || {
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.id, fixture.id);
        assert_eq!(dispute.creator, fixture.creator);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.status, DisputeStatus::Open);
    });
}

#[test]
fn test_open_resolve_by_resolver_transitions_to_resolved() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_open(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::resolve(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Ok(()));

        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.creator, fixture.creator);
    });
}

#[test]
fn test_open_resolve_by_non_resolver_returns_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_open(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::resolve(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State remains unchanged
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Open);
    });
}

#[test]
fn test_open_close_by_resolver_transitions_to_closed() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_open(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::close(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Ok(()));

        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.creator, fixture.creator);
    });
}

#[test]
fn test_open_close_by_non_resolver_returns_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_open(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::close(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State remains unchanged
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Open);
    });
}

// ===========================================================================
// Resolved State Transitions
// ===========================================================================

#[test]
fn test_resolved_get_dispute_returns_resolved_dispute() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_resolved(&e);

    e.as_contract(&fixture.contract_id, || {
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.creator, fixture.creator);
    });
}

#[test]
fn test_resolved_resolve_by_resolver_returns_already_closed() {
    // Resolve-after-resolve invariant: terminal for resolve step
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_resolved(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::resolve(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Err(DisputeError::AlreadyClosed));

        // State preserved as Resolved
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
    });
}

#[test]
fn test_resolved_resolve_by_non_resolver_returns_unauthorized() {
    // Resolver mismatch check precedes status check
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_resolved(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::resolve(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State preserved as Resolved
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
    });
}

#[test]
fn test_resolved_close_by_resolver_transitions_to_closed() {
    // Legal transition: Resolved -> Closed
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_resolved(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::close(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Ok(()));

        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.creator, fixture.creator);
    });
}

#[test]
fn test_resolved_close_by_non_resolver_returns_unauthorized() {
    // Resolver mismatch check precedes status check on Resolved disputes
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_resolved(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::close(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State preserved as Resolved
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Resolved);
    });
}

// ===========================================================================
// Closed State Transitions
// ===========================================================================

#[test]
fn test_closed_get_dispute_returns_closed_dispute() {
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_closed(&e);

    e.as_contract(&fixture.contract_id, || {
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
        assert_eq!(dispute.resolver, fixture.resolver);
        assert_eq!(dispute.creator, fixture.creator);
    });
}

#[test]
fn test_closed_resolve_by_resolver_returns_already_closed() {
    // Resolve on Closed dispute rejected as AlreadyClosed
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_closed(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::resolve(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Err(DisputeError::AlreadyClosed));

        // State preserved as Closed
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
    });
}

#[test]
fn test_closed_resolve_by_non_resolver_returns_unauthorized() {
    // Non-resolver on Closed dispute receives Unauthorized before status check
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_closed(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::resolve(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State preserved as Closed
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
    });
}

#[test]
fn test_closed_close_by_resolver_returns_already_closed() {
    // Close-after-close invariant: no double-close
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_closed(&e);

    e.as_contract(&fixture.contract_id, || {
        let res = DisputeResolutionContract::close(
            e.clone(),
            fixture.resolver.clone(),
            fixture.id,
        );
        assert_eq!(res, Err(DisputeError::AlreadyClosed));

        // State preserved as Closed
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
    });
}

#[test]
fn test_closed_close_by_non_resolver_returns_unauthorized() {
    // Non-resolver on Closed dispute receives Unauthorized before status check
    let e = Env::default();
    e.mock_all_auths();
    let fixture = setup_closed(&e);

    e.as_contract(&fixture.contract_id, || {
        let stranger = Address::generate(&e);
        let res = DisputeResolutionContract::close(e.clone(), stranger, fixture.id);
        assert_eq!(res, Err(DisputeError::Unauthorized));

        // State preserved as Closed
        let dispute = DisputeResolutionContract::get_dispute(e.clone(), fixture.id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Closed);
    });
}

// ===========================================================================
// Full Lifecycle Flows (Round-trip sequences)
// ===========================================================================

#[test]
fn test_full_lifecycle_open_resolved_closed() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let creator = Address::generate(&e);
        let resolver = Address::generate(&e);
        let stranger = Address::generate(&e);

        // 1. Initial creation -> Open
        let id = DisputeResolutionContract::create_dispute(
            e.clone(),
            creator.clone(),
            resolver.clone(),
        );
        let d0 = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(d0.status, DisputeStatus::Open);

        // 2. Illegal operations in Open state
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );

        // 3. Open -> Resolved
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), resolver.clone(), id),
            Ok(())
        );
        let d1 = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(d1.status, DisputeStatus::Resolved);

        // 4. Illegal operations in Resolved state (resolve-after-resolve & mismatch)
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );

        // 5. Resolved -> Closed
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), resolver.clone(), id),
            Ok(())
        );
        let d2 = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(d2.status, DisputeStatus::Closed);

        // 6. Illegal operations in Closed state (double-close, re-resolve & mismatches)
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );

        // Terminal state preserved
        let d3 = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(d3.status, DisputeStatus::Closed);
    });
}

#[test]
fn test_direct_lifecycle_open_to_closed() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let creator = Address::generate(&e);
        let resolver = Address::generate(&e);

        // 1. Creation -> Open
        let id = DisputeResolutionContract::create_dispute(
            e.clone(),
            creator.clone(),
            resolver.clone(),
        );

        // 2. Direct close from Open -> Closed (skipping Resolved)
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), resolver.clone(), id),
            Ok(())
        );
        let d = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(d.status, DisputeStatus::Closed);

        // 3. Re-close and resolve fail
        assert_eq!(
            DisputeResolutionContract::close(e.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(
            DisputeResolutionContract::resolve(e.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
    });
}
