use crate::DataKey;
use soroban_sdk::{Address, Env, Vec};

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
