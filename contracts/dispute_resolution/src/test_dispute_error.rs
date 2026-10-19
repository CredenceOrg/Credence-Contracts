//! Boundary and recovery coverage for `contracts/dispute_resolution/src/error.rs` (#1385).

extern crate std;

use crate::error::DisputeError;
use crate::{DisputeResolutionContract, DisputeStatus};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn wire_codes_are_stable() {
    assert_eq!(DisputeError::DisputeNotFound as u32, 1);
    assert_eq!(DisputeError::AlreadyClosed as u32, 2);
    assert_eq!(DisputeError::Unauthorized as u32, 3);
}

#[test]
fn wire_codes_are_unique_nonzero_and_contiguous() {
    let codes = [
        DisputeError::DisputeNotFound as u32,
        DisputeError::AlreadyClosed as u32,
        DisputeError::Unauthorized as u32,
    ];
    for code in codes {
        assert_ne!(code, 0);
    }
    assert!(codes[0] != codes[1] && codes[1] != codes[2] && codes[0] != codes[2]);
    assert_eq!(codes, [1, 2, 3]);
}

#[test]
fn variants_are_copy_and_clone_preserving_value() {
    let original = DisputeError::Unauthorized;
    let copied = original;
    #[allow(clippy::clone_on_copy)]
    let cloned = original.clone();
    assert_eq!(original, copied);
    assert_eq!(original, cloned);
    assert_ne!(DisputeError::Unauthorized, DisputeError::AlreadyClosed);
}

#[test]
fn debug_renders_the_variant_name() {
    assert!(std::format!("{:?}", DisputeError::DisputeNotFound).contains("DisputeNotFound"));
    assert!(std::format!("{:?}", DisputeError::AlreadyClosed).contains("AlreadyClosed"));
    assert!(std::format!("{:?}", DisputeError::Unauthorized).contains("Unauthorized"));
}

fn register(e: &Env) -> Address {
    e.register(DisputeResolutionContract, ())
}

#[test]
fn get_dispute_returns_not_found_for_an_unknown_id() {
    let e = Env::default();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        let err = DisputeResolutionContract::get_dispute(e.clone(), 42).unwrap_err();
        assert_eq!(err, DisputeError::DisputeNotFound);
    });
}

#[test]
fn missing_lookup_is_deterministic_and_side_effect_free() {
    let e = Env::default();
    let contract_id = register(&e);

    e.as_contract(&contract_id, || {
        for _ in 0..3 {
            assert_eq!(
                DisputeResolutionContract::get_dispute(e.clone(), 7).unwrap_err(),
                DisputeError::DisputeNotFound
            );
        }
    });
}

#[test]
fn created_dispute_is_returned_and_the_error_is_only_for_absent_ids() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = register(&e);
    let creator = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let resolver = contract_id.clone();
        let id =
            DisputeResolutionContract::create_dispute(e.clone(), creator.clone(), resolver.clone());

        let dispute = DisputeResolutionContract::get_dispute(e.clone(), id).unwrap();
        assert_eq!(dispute.id, id);
        assert_eq!(dispute.creator, creator);
        assert_eq!(dispute.status, DisputeStatus::Open);
        assert_eq!(dispute.resolver, resolver);

        assert_eq!(
            DisputeResolutionContract::get_dispute(e.clone(), id.wrapping_add(1)).unwrap_err(),
            DisputeError::DisputeNotFound
        );
        assert_eq!(
            DisputeResolutionContract::get_dispute(e.clone(), id)
                .unwrap()
                .id,
            id
        );
    });
}
