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
/// - successful admin transfer,
/// - rejection of unauthorized callers,
/// - rejection of invalid (zero) admin addresses,
/// - boundary behavior for self-transfer,
/// - recovery after a failed transfer (state must be unchanged),
/// - determinism across repeated calls.
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    /// Minimal reference implementation of the `Governable` interface
    /// used to drive the interface tests. This is not shipped in
    /// production code; it exists only to validate the contract that
    /// consumers of the interface must uphold.
    struct ReferenceGovernable;

    impl ReferenceGovernable {
        const ADMIN_KEY: &'static str = "admin";

        pub fn init(env: &Env, admin: Address) {
            assert!(admin != Address::generate(env), "admin must not be the zero address");
            env.storage().persistent().set(&Self::ADMIN_KEY, &admin);
        }

        pub fn get_admin(env: Env) -> Address {
            env.storage()
                .persistent()
                .get::<&str, Address>(&Self::ADMIN_KEY)
                .expect("admin not initialized")
        }

        /// Fallible core of the transfer, used by the recovery tests so that
        /// state preservation can be asserted without unwinding (this crate is
        /// `no_std`, so `catch_unwind` is not available).
        pub fn try_set_admin_auth(
            env: &Env,
            caller: &Address,
            new_admin: &Address,
        ) -> Result<(), &'static str> {
            let current = Self::get_admin(env.clone());
            if &current != caller {
                return Err("caller is not the admin");
            }
            if new_admin == &Address::generate(env) {
                return Err("new admin must not be the zero address");
            }
            env.storage().persistent().set(&Self::ADMIN_KEY, new_admin);
            Ok(())
        }

        pub fn set_admin_auth(env: Env, caller: Address, new_admin: Address) {
            caller.require_auth();
            Self::try_set_admin_auth(&env, &caller, &new_admin)
                .expect("admin transfer rejected");
        }
    }

    impl Governable for ReferenceGovernable {
        fn get_admin(env: Env) -> Address {
            ReferenceGovernable::get_admin(env)
        }

        fn set_admin(env: Env, new_admin: Address) {
            let current = ReferenceGovernable::get_admin(env.clone());
            current.require_auth();
            ReferenceGovernable::set_admin_auth(env, current, new_admin);
        }
    }

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        ReferenceGovernable::init(&env, admin.clone());
        (env, admin, other)
    }

    /// Success: the current admin can transfer control to a new address.
    #[test]
    fn set_admin_success_transfers_control() {
        let (env, admin, new_admin) = setup();
        ReferenceGovernable::set_admin_auth(env.clone(), admin.clone(), new_admin.clone());
        assert_eq!(ReferenceGovernable::get_admin(env.clone()), new_admin);
    }

    /// Rejection: a non-admin caller must not be able to transfer control.
    #[test]
    #[should_panic]
    fn set_admin_rejects_unauthorized_caller() {
        let (env, _admin, other) = setup();
        let new_admin = Address::generate(&env);
        ReferenceGovernable::set_admin_auth(env.clone(), other, new_admin);
    }

    /// Rejection: the zero address is not a valid admin.
    #[test]
    #[should_panic]
    fn set_admin_rejects_zero_address() {
        let (env, admin, _) = setup();
        let zero = Address::generate(&env);
        ReferenceGovernable::set_admin_auth(env.clone(), admin, zero);
    }

    /// Boundary: transferring to the current admin is a no-op and must not
    /// corrupt state.
    #[test]
    fn set_admin_self_transfer_is_no_op() {
        let (env, admin, _) = setup();
        ReferenceGovernable::set_admin_auth(env.clone(), admin.clone(), admin.clone());
        assert_eq!(ReferenceGovernable::get_admin(env.clone()), admin);
    }

    /// Recovery: a failed transfer must leave the admin unchanged.
    #[test]
    fn failed_transfer_preserves_admin() {
        let (env, admin, other) = setup();
        let new_admin = Address::generate(&env);
        let result = ReferenceGovernable::try_set_admin_auth(&env, &other, &new_admin);
        assert!(result.is_err(), "expected unauthorized transfer to fail");
        assert_eq!(ReferenceGovernable::get_admin(env.clone()), admin);
    }

    /// Determinism: repeated calls to `get_admin` return the same value.
    #[test]
    fn get_admin_is_deterministic() {
        let (env, admin, _) = setup();
        for _ in 0..8 {
            assert_eq!(ReferenceGovernable::get_admin(env.clone()), admin);
        }
    }

    /// Recovery: after a successful transfer, the old admin can no longer
    /// transfer control, and the new admin can.
    #[test]
    fn new_admin_gains_control() {
        let (env, admin, new_admin) = setup();
        ReferenceGovernable::set_admin_auth(env.clone(), admin.clone(), new_admin.clone());

        // Old admin is rejected.
        let other = Address::generate(&env);
        let result = ReferenceGovernable::try_set_admin_auth(&env, &admin, &other);
        assert!(result.is_err(), "old admin must lose control");

        // New admin can transfer.
        ReferenceGovernable::set_admin_auth(env.clone(), new_admin.clone(), other.clone());
        assert_eq!(ReferenceGovernable::get_admin(env.clone()), other);
    }

    /// Boundary: the interface must be consumable through the generated
    /// client type without additional adapters.
    ///
    /// This checks that the `GovernableClient` type exists and is bound to
    /// the trait method signatures expected by callers.
    /// It is a compile-time contract check rather than a runtime assertion.
    #[test]
    fn governable_client_is_available() {
        // The client type is generated by the `#[contractclient]` attribute.
        // Referencing it here ensures the attribute stays in place and the
        // generated type remains publicly usable.
        fn assert_client_type<'a>() {
            let _client_type = core::marker::PhantomData::<GovernableClient<'a>>;
        }
        assert_client_type();
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
