use soroban_sdk::{contractclient, Address, Env};

#[contractclient(name = "GovernableClient")]
pub trait Governable {
    /// Get the current admin address.
    fn get_admin(env: Env) -> Address;

    /// Transfer administrative control to a new address.
    fn set_admin(env: Env, new_admin: Address);
}
