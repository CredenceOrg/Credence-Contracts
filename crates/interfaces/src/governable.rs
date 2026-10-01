use soroban_sdk::{contractclient, Address, Env};

/// Governable defines the minimal administrative control interface
/// for a contract.
///
/// # Invariants
///
/// - There is always exactly one admin address at any time.
/// - `set_admin` must be authorized by the current admin.
/// - A successful `set_admin` fully replaces the previous admin; the
///   previous admin loses all administrative privileges immediately.
/// - The admin address must never be the zero/default address.
///
/// # Failure modes
///
/// - Authorization failure: a caller that is not the current admin must
///   cause `set_admin` to revert with an authorization error.
/// - Invalid input: attempting to transfer to the zero address must revert
///   without mutating state.
/// - Self-transfer: transferring to the current admin is a no-op and must
///   not corrupt state.
///
/// # Observability
///
/// Implementations should emit an event on admin transfer so that operators
/// can diagnose failures without exposing sensitive data. The event must not
/// contain anything other than the new admin address.
#[contractclient(name = "GovernableClient")]
pub trait Governable {
    /// Get the current admin address.
    ///
    /// Returns the admin that is currently authorized to call `set_admin`.
    /// This function is always safe to call and never mutates state.
    fn get_admin(env: Env) -> Address;

    /// Transfer administrative control to a new address.
    ///
    /// # Authorization
    ///
    /// Requires authorization from the current admin. Callers that are not
    /// the current admin must be rejected.
    ///
    /// # Validation
    ///
    /// `new_admin` must not be the zero address. Transferring to the
    /// current admin is a no-op and must not corrupt state.
    ///
    /// # State transition
    ///
    /// On success, the previous admin is replaced atomically; there is no
    /// intermediate state in which neither address holds administrative
    /// control. If the call reverts, administrative control remains with
    /// the original admin.
    fn set_admin(env: Env, new_admin: Address);
}

/// Test module covering boundary and recovery scenarios for the `Governable`
/// interface.
///
/// The trait methods are associated functions (they take `Env`, not `self`),
/// so the documented semantics are pinned here with a small in-memory
/// reference model. Production contracts implement the trait on top of their
/// own on-chain storage; the reference model exists only to check that those
/// implementations uphold the contract below:
///
/// - successful admin transfer,
/// - rejection of unauthorized callers,
/// - rejection of an invalid (zero/default) admin address,
/// - boundary behavior for self-transfer,
/// - recovery after a failed transfer (state must be unchanged),
/// - determinism across repeated calls.
///
/// Admin identities are modelled as opaque integers rather than `Address`
/// values. Distinct integers stand in for distinct addresses, which keeps the
/// model deterministic and free of test-only SDK features. The `try_set_admin`
/// model returns a deterministic error code rather than panicking so that
/// failure *recovery* (state unchanged, retry succeeds) can be asserted
/// directly. On-chain this maps to a reverted contract call.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::{ERR_INVALID_INPUT, ERR_UNAUTHORIZED};

    /// Opaque stand-in for an `Address`; two distinct values are two distinct
    /// administrators.
    type AdminId = u32;

    /// In-memory reference implementation of the `Governable` interface.
    ///
    /// Only the decision logic of `set_admin` is modelled; storage layout,
    /// `require_auth` enforcement, and event emission are the responsibility
    /// of the concrete contract.
    struct ReferenceGovernable {
        admin: AdminId,
    }

    impl ReferenceGovernable {
        fn new(admin: AdminId) -> Self {
            Self { admin }
        }

        fn get_admin(&self) -> AdminId {
            self.admin
        }

        /// Applies the documented `set_admin` decision rules.
        ///
        /// `new_admin` is `None` when the caller supplies the zero/default
        /// address, which the interface must reject without mutating state.
        fn try_set_admin(
            &mut self,
            caller: AdminId,
            new_admin: Option<AdminId>,
        ) -> Result<(), &'static str> {
            // Authorization is checked first: an unauthorized caller must not
            // be able to distinguish an invalid admin from a valid one.
            if caller != self.admin {
                return Err(ERR_UNAUTHORIZED);
            }
            let new_admin = new_admin.ok_or(ERR_INVALID_INPUT)?;
            // Atomic replace: there is no observable intermediate state.
            self.admin = new_admin;
            Ok(())
        }
    }

    const ADMIN: AdminId = 1;
    const OTHER: AdminId = 2;
    const NEW_ADMIN: AdminId = 3;

    // --- Success ---

    #[test]
    fn set_admin_success_transfers_control() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(gov.try_set_admin(ADMIN, Some(NEW_ADMIN)), Ok(()));
        assert_eq!(gov.get_admin(), NEW_ADMIN);
    }

    // --- Rejection: authorization ---

    #[test]
    fn set_admin_rejects_unauthorized_caller() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(
            gov.try_set_admin(OTHER, Some(NEW_ADMIN)),
            Err(ERR_UNAUTHORIZED)
        );
        // Recovery: the rejected call must not have moved control.
        assert_eq!(gov.get_admin(), ADMIN);
    }

    #[test]
    fn authorization_is_checked_before_validation() {
        // An unauthorized caller must not learn whether the new admin is valid.
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(gov.try_set_admin(OTHER, None), Err(ERR_UNAUTHORIZED));
        assert_eq!(gov.get_admin(), ADMIN);
    }

    // --- Rejection: invalid input ---

    #[test]
    fn set_admin_rejects_invalid_admin() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(gov.try_set_admin(ADMIN, None), Err(ERR_INVALID_INPUT));
        // Recovery: invalid input must leave the admin untouched.
        assert_eq!(gov.get_admin(), ADMIN);
    }

    // --- Boundary: self-transfer ---

    #[test]
    fn set_admin_self_transfer_is_no_op() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(gov.try_set_admin(ADMIN, Some(ADMIN)), Ok(()));
        assert_eq!(gov.get_admin(), ADMIN);
    }

    // --- Recovery: failed transfer preserves admin, retry succeeds ---

    #[test]
    fn failed_transfer_preserves_admin_and_allows_retry() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        // Unauthorized attempt fails...
        assert_eq!(
            gov.try_set_admin(OTHER, Some(NEW_ADMIN)),
            Err(ERR_UNAUTHORIZED)
        );
        assert_eq!(gov.get_admin(), ADMIN);

        // ...and recovery succeeds once the real admin acts.
        assert_eq!(gov.try_set_admin(ADMIN, Some(NEW_ADMIN)), Ok(()));
        assert_eq!(gov.get_admin(), NEW_ADMIN);
    }

    // --- Recovery: old admin loses control, new admin gains it ---

    #[test]
    fn new_admin_gains_control_old_admin_loses_it() {
        let mut gov = ReferenceGovernable::new(ADMIN);

        assert_eq!(gov.try_set_admin(ADMIN, Some(NEW_ADMIN)), Ok(()));
        assert_eq!(gov.get_admin(), NEW_ADMIN);

        // The previous admin can no longer transfer control.
        assert_eq!(gov.try_set_admin(ADMIN, Some(OTHER)), Err(ERR_UNAUTHORIZED));
        assert_eq!(gov.get_admin(), NEW_ADMIN);

        // The new admin can.
        assert_eq!(gov.try_set_admin(NEW_ADMIN, Some(OTHER)), Ok(()));
        assert_eq!(gov.get_admin(), OTHER);
    }

    // --- Determinism ---

    #[test]
    fn get_admin_is_deterministic() {
        let gov = ReferenceGovernable::new(ADMIN);

        for _ in 0..8 {
            assert_eq!(gov.get_admin(), ADMIN);
        }
    }

    // --- Boundary: the generated client type stays available ---

    #[test]
    fn governable_client_is_available() {
        // `GovernableClient` is generated by the `#[contractclient]` attribute
        // on `Governable`. Referencing it here fails the build if the attribute
        // is removed, which would break every cross-contract caller.
        let _client: Option<GovernableClient<'_>> = None;
    }
}
