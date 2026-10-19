#![cfg(test)]

//! Deterministic failure-boundary coverage for
//! `AdminContract::get_required_role_to_assign` (#1402).
//!
//! The mapping is the privilege gate for every admin-role mutation, so these
//! tests pin the exact table and the invariants callers depend on:
//! * assigning a role always requires a *strictly higher* role, except that
//!   `SuperAdmin` (the ceiling) requires `SuperAdmin`;
//! * `Operator` is not over-privileged (it must not require `SuperAdmin`);
//! * the mapping is total, deterministic, and monotonic with the role order.

use crate::{AdminContract, AdminRole};

const ALL_ROLES: [AdminRole; 3] = [AdminRole::Operator, AdminRole::Admin, AdminRole::SuperAdmin];

fn required(role: AdminRole) -> AdminRole {
    AdminContract::get_required_role_to_assign(role)
}

#[test]
fn operator_requires_admin() {
    assert_eq!(required(AdminRole::Operator), AdminRole::Admin);
}

#[test]
fn admin_requires_super_admin() {
    assert_eq!(required(AdminRole::Admin), AdminRole::SuperAdmin);
}

#[test]
fn super_admin_requires_super_admin() {
    assert_eq!(required(AdminRole::SuperAdmin), AdminRole::SuperAdmin);
}

#[test]
fn mapping_is_deterministic_for_every_role() {
    for role in ALL_ROLES {
        assert_eq!(required(role), required(role));
    }
}

#[test]
fn every_role_has_a_defined_requirement() {
    // Totality: the `match` in the implementation must cover all variants.
    for role in ALL_ROLES {
        let _ = required(role);
    }
}

#[test]
fn required_role_is_never_below_the_assigned_role() {
    for role in ALL_ROLES {
        assert!(
            required(role) >= role,
            "required role must be at least the role being assigned"
        );
    }
}

#[test]
fn assigning_a_non_super_role_requires_strictly_higher_privilege() {
    // Guards against a regression that would let an admin grant the same (or a
    // higher) role they do not themselves hold.
    assert!(required(AdminRole::Admin) > AdminRole::Admin);
    assert!(required(AdminRole::Operator) > AdminRole::Operator);
}

#[test]
fn operator_is_not_over_privileged() {
    // Least privilege: the weakest assignable role must not demand the highest
    // role just to grant it.
    assert_ne!(required(AdminRole::Operator), AdminRole::SuperAdmin);
}

#[test]
fn mapping_is_monotonic_with_role_order() {
    // `AdminRole` is `Ord`: SuperAdmin > Admin > Operator.
    assert!(required(AdminRole::SuperAdmin) >= required(AdminRole::Admin));
    assert!(required(AdminRole::Admin) >= required(AdminRole::Operator));
}

#[test]
fn mapping_table_is_frozen() {
    // Explicit snapshot of the privilege gate. Changing any entry changes the
    // authorization model and must be a deliberate, reviewed decision.
    let expected = [
        (AdminRole::Operator, AdminRole::Admin),
        (AdminRole::Admin, AdminRole::SuperAdmin),
        (AdminRole::SuperAdmin, AdminRole::SuperAdmin),
    ];
    for (role, required_role) in expected {
        assert_eq!(required(role), required_role);
    }
}
