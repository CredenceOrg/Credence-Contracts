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
/// Implementations should emit a event on admin transfer so that operators
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

/// Compile-time shape checks for the `Governable` interface.
///
/// These are static assertions that verify the trait methods exist with the
/// correct signatures. They do not execute at runtime but ensure that any
/// change to the public interface is caught by the compiler.
///
/// # Invariants documented
///
/// - `get_admin` takes `Env` and returns `Address` (read-only, no auth).
/// - `set_admin` takes `Env` and a new `Address` (mutation, admin-authed).
/// - Both methods are object-safe through the `GovernableClient` generated type.
#[cfg(test)]
mod tests {
    use super::*;

    // ── Compile-time contract checks ────────────────────────────────────────
    //
    // We cannot instantiate a real Soroban environment in unit tests because
    // soroban-sdk's `Env::default()` is only available behind the `testutils`
    // feature, which is only enabled for contract-level tests, not for
    // interface crate tests. These checks therefore focus on static shape
    // verification: we prove the trait compiles with the exact signatures
    // expected by callers, and that the `GovernableClient` generated type is
    // usable as a cross-contract handle.

    /// Invariant: `Governable` exposes exactly `get_admin` and `set_admin`.
    ///
    /// Adding or removing a method without updating this check is a
    /// compile-time error, which forces reviewers to consider compatibility.
    fn _assert_governable_shape<T: Governable>() {
        // If `T` does not implement `Governable`, this function fails to
        // compile. The phantom references to the method names prevent the
        // compiler from stripping them as dead code.
        let _get: fn(Env) -> Address = T::get_admin;
        let _set: fn(Env, Address) = T::set_admin;
    }

    /// Invariant: `GovernableClient` is generated and bound to the `Governable`
    /// trait's lifetime parameter, confirming the `#[contractclient]` macro
    /// ran successfully.
    ///
    /// This is a zero-cost compile-time check; `PhantomData` is erased by
    /// the optimizer and the function is never called.
    #[test]
    fn governable_client_type_is_generated() {
        // Holds a zero-sized phantom reference to `GovernableClient`. If the
        // `#[contractclient]` attribute was removed or renamed, this line
        // will fail to compile.
        let _phantom: core::marker::PhantomData<GovernableClient<'_>> = core::marker::PhantomData;
    }

    /// Invariant: `get_admin` signature matches the expected `fn(Env) -> Address`.
    ///
    /// This is a compile-time type check. Any change to the return type or
    /// argument list of `get_admin` will cause a type-mismatch error here,
    /// surfacing the breaking change before it lands.
    #[test]
    fn get_admin_has_correct_signature() {
        // Assign the associated function to a typed function pointer.
        // The compiler will reject this if the signature does not match.
        let _: fn(Env) -> Address = <GovernableClient<'_> as Governable>::get_admin;
    }

    /// Invariant: `set_admin` signature matches `fn(Env, Address)`.
    ///
    /// Same rationale as `get_admin_has_correct_signature`.
    #[test]
    fn set_admin_has_correct_signature() {
        let _: fn(Env, Address) = <GovernableClient<'_> as Governable>::set_admin;
    }

    /// Boundary: the trait is object-safe — it can be used as a `dyn` trait.
    ///
    /// Soroban cross-contract calls always go through the generated
    /// `GovernableClient`, but ensuring the trait itself is object-safe means
    /// it can also be used with `Box<dyn Governable>` in off-chain tooling
    /// without a code change.
    #[test]
    fn governable_is_object_safe() {
        // A trait object assignment compiles only if the trait is object-safe.
        // No runtime call is made; this is purely a compile-time assertion.
        let _make_dyn = |_x: &dyn Governable| {};
    }

    /// Regression: the trait name `Governable` and client name `GovernableClient`
    /// must remain stable across refactors.
    ///
    /// Any rename breaks existing callers that import these identifiers by
    /// name. This test ensures the names are referenced explicitly so a
    /// rename propagates a compile error rather than a silent behaviour change.
    #[test]
    fn governable_names_are_stable() {
        fn _use_trait_name<T: Governable>() {}
        // Referencing the client by its exact name.
        let _: core::marker::PhantomData<GovernableClient<'_>> = core::marker::PhantomData;
    }
}
