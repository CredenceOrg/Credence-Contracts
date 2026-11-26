#[macro_export]
macro_rules! require_admin {
    ($env:expr, $caller:expr, $admin_key:expr) => {{
        let stored_admin: ::soroban_sdk::Address = $env
            .storage()
            .instance()
            .get(&$admin_key)
            .unwrap_or_else(|| {
                ::soroban_sdk::panic_with_error!($env, $crate::ContractError::NotInitialized)
            });

        let caller_ref: &::soroban_sdk::Address = ::core::borrow::Borrow::borrow(&$caller);
        if &stored_admin != caller_ref {
            ::soroban_sdk::panic_with_error!($env, $crate::ContractError::NotAdmin);
        }

        caller_ref.require_auth();
    }};
}
