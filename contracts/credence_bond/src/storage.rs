use crate::Bond;
use credence_errors::ContractError;
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Bond(Address),
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
    Locked,
}

pub fn get_admin(e: &Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::Admin)
}

pub fn set_admin(e: &Env, admin: &Address) {
    e.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Token)
        .expect("token not initialized")
}

pub fn set_token(e: &Env, token: &Address) {
    e.storage().instance().set(&DataKey::Token, token);
}

pub fn has_bond(e: &Env, identity: &Address) -> bool {
    e.storage().instance().has(&DataKey::Bond(identity.clone()))
}

pub fn get_bond(e: &Env, identity: &Address) -> Result<Bond, ContractError> {
    e.storage()
        .instance()
        .get(&DataKey::Bond(identity.clone()))
        .ok_or(ContractError::BondNotFound)
}

pub fn set_bond(e: &Env, identity: &Address, bond: &Bond) {
    e.storage()
        .instance()
        .set(&DataKey::Bond(identity.clone()), bond);
}

pub fn is_locked(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Locked)
        .unwrap_or(false)
}

pub fn set_lock(e: &Env, locked: bool) {
    e.storage().instance().set(&DataKey::Locked, &locked);
}
