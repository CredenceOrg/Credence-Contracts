use soroban_sdk::{contracttype, Address, Env, Vec};

/// Instance-storage keys owned by this module.
///
/// This enum shares a Rust name (`DataKey`) with the crate-root enum in
/// `lib.rs`, which also writes to instance storage. Soroban keys
/// `#[contracttype]` enums by variant name + field shape, not by Rust type
/// name, so a future unit variant added to `lib.rs::DataKey` with the exact
/// name `AcceptedTokens` would silently alias this key. See
/// `docs/STORAGE_KEY_LAYOUT.md` for the full collision-safety rules before
/// adding variants to either enum.
#[contracttype]
pub enum DataKey {
    AcceptedTokens,
}

pub fn get_accepted_tokens(e: &Env) -> Vec<Address> {
    e.storage()
        .instance()
        .get(&DataKey::AcceptedTokens)
        .unwrap_or_else(|| Vec::new(e))
}

pub fn set_accepted_tokens(e: &Env, tokens: &Vec<Address>) {
    e.storage().instance().set(&DataKey::AcceptedTokens, tokens);
}

pub fn is_token_accepted(e: &Env, token: &Address) -> bool {
    let accepted = get_accepted_tokens(e);
    accepted.iter().any(|t| t == *token)
}

pub fn is_locked(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::SettlingFlag)
        .unwrap_or(false)
}

pub fn set_lock(e: &Env, value: bool) {
    e.storage()
        .instance()
        .set(&DataKey::SettlingFlag, &value);
}

pub fn get_admin(e: &Env) -> Option<Address> {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
}
