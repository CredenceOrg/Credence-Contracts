//! Lease authorization primitives used by defence-in-depth guards.
//!
//! A lease grants a signer a scoped, time-bounded permission to perform
//! protocol operations. Guards in this module enforce that a lease's scope
//! covers the requested operation (and related expiry checks in follow-ups).

use soroban_sdk::{contracttype, Address, Env};

use crate::ContractError;

/// Well-known lease operation bits. Combine with `|` when issuing a lease.
pub mod lease_op {
    /// Read-only queries (views, balance checks).
    pub const READ: u32 = 1 << 0;
    /// State-mutating writes (create, top-up, transfer-in).
    pub const WRITE: u32 = 1 << 1;
    /// Extend / renew an existing lease or bond window.
    pub const RENEW: u32 = 1 << 2;
    /// Transfer custody or re-assign the lease signer.
    pub const TRANSFER: u32 = 1 << 3;
    /// Convenience mask granting every defined op.
    pub const ALL: u32 = READ | WRITE | RENEW | TRANSFER;
}

/// Time-bounded, scoped authorization grant.
///
/// - `signer` — identity that may exercise the lease
/// - `scope` — bitmask of [`lease_op`] bits the lease authorises
/// - `expires_at` — exclusive upper bound (`now >= expires_at` ⇒ expired)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    pub signer: Address,
    pub scope: u32,
    pub expires_at: u64,
}

/// Rejects a lease whose scope does not cover every bit in `op`.
///
/// Threat mitigated: an attacker reuses a narrowly-scoped lease (e.g. `READ`)
/// to drive a privileged op (e.g. `WRITE` / `TRANSFER`). Without this check,
/// callers that only verify signer identity could escalate privileges.
///
/// # Panics
/// With [`ContractError::LeaseScopeMismatch`] when `(lease.scope & op) != op`.
#[inline]
pub fn require_matching_lease_scope(e: &Env, lease: &Lease, op: u32) {
    if (lease.scope & op) != op {
        soroban_sdk::panic_with_error!(e, ContractError::LeaseScopeMismatch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn lease(e: &Env, scope: u32, expires_at: u64) -> Lease {
        Lease {
            signer: Address::generate(e),
            scope,
            expires_at,
        }
    }

    #[test]
    fn require_matching_lease_scope_allows_exact_match() {
        let e = Env::default();
        let l = lease(&e, lease_op::WRITE, u64::MAX);
        require_matching_lease_scope(&e, &l, lease_op::WRITE);
    }

    #[test]
    fn require_matching_lease_scope_allows_superset_scope() {
        let e = Env::default();
        let l = lease(&e, lease_op::ALL, u64::MAX);
        require_matching_lease_scope(&e, &l, lease_op::READ | lease_op::WRITE);
    }

    #[test]
    #[should_panic]
    fn require_matching_lease_scope_rejects_adjacent_scope() {
        // Lease grants READ; requested op is WRITE (adjacent bit, no overlap).
        let e = Env::default();
        let l = lease(&e, lease_op::READ, u64::MAX);
        require_matching_lease_scope(&e, &l, lease_op::WRITE);
    }

    #[test]
    #[should_panic]
    fn require_matching_lease_scope_rejects_wildly_different_scope() {
        let e = Env::default();
        let l = lease(&e, lease_op::READ, u64::MAX);
        require_matching_lease_scope(&e, &l, lease_op::TRANSFER);
    }

    #[test]
    #[should_panic]
    fn require_matching_lease_scope_rejects_partial_bit_coverage() {
        // Negative test required by #847: WRITE|RENEW is not covered by READ|WRITE.
        let e = Env::default();
        let l = lease(&e, lease_op::READ | lease_op::WRITE, u64::MAX);
        require_matching_lease_scope(&e, &l, lease_op::WRITE | lease_op::RENEW);
    }
}
