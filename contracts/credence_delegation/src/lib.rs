#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

pub mod pausable;

#[contracttype]
#[derive(Clone, Debug)]
pub enum DelegationType {
    Attestation,
    Management,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum AttestationStatus {
    Active,
    Revoked,
    NotFound,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Delegation {
    pub owner: Address,
    pub delegate: Address,
    pub delegation_type: DelegationType,
    pub expires_at: u64,
    pub revoked: bool,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    Delegation(Address, Address, DelegationType),
}

#[contract]
pub struct CredenceDelegation;

#[contractimpl]
impl CredenceDelegation {
    /// Initialize the contract with an admin address.
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);
    }

    /// Create a delegation from owner to delegate with a given type and expiry.
    pub fn delegate(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        expires_at: u64,
    ) -> Delegation {
        pausable::require_not_paused(&e);
        owner.require_auth();

        if expires_at <= e.ledger().timestamp() {
            panic!("expiry must be in the future");
        }

        let key = DataKey::Delegation(owner.clone(), delegate.clone(), delegation_type.clone());

        let d = Delegation {
            owner: owner.clone(),
            delegate: delegate.clone(),
            delegation_type,
            expires_at,
            revoked: false,
        };

        e.storage().instance().set(&key, &d);
        e.events()
            .publish((Symbol::new(&e, "delegation_created"),), d.clone());

        d
    }

    /// Revoke an existing delegation. Only the owner can revoke.
    pub fn revoke_delegation(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) {
        pausable::require_not_paused(&e);
        owner.require_auth();

        let key = DataKey::Delegation(owner.clone(), delegate.clone(), delegation_type.clone());

        let mut d: Delegation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("delegation not found"));

        if d.revoked {
            panic!("already revoked");
        }

        d.revoked = true;
        e.storage().instance().set(&key, &d);
        e.events()
            .publish((Symbol::new(&e, "delegation_revoked"),), d);
    }

    pub fn revoke_attestation(e: Env, attester: Address, subject: Address) {
        pausable::require_not_paused(&e);
        attester.require_auth();

        let key = DataKey::Delegation(
            attester.clone(),
            subject.clone(),
            DelegationType::Attestation,
        );

        let mut d: Delegation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("attestation not found"));

        if d.revoked {
            panic!("attestation already revoked");
        }

        d.revoked = true;
        e.storage().instance().set(&key, &d);

        e.events()
            .publish((Symbol::new(&e, "attestation_revoked"),), d);
    }

    /// Retrieve a stored delegation.
    pub fn get_delegation(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) -> Delegation {
        let key = DataKey::Delegation(owner, delegate, delegation_type);
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("delegation not found"))
    }

    /// Check whether a delegate is currently valid (not revoked, not expired).
    pub fn is_valid_delegate(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) -> bool {
        let key = DataKey::Delegation(owner, delegate, delegation_type);
        match e.storage().instance().get::<_, Delegation>(&key) {
            Some(d) => !d.revoked && d.expires_at > e.ledger().timestamp(),
            None => false,
        }
    }

    pub fn get_attestation_status(
        e: Env,
        attester: Address,
        subject: Address,
    ) -> AttestationStatus {
        let key = DataKey::Delegation(attester, subject, DelegationType::Attestation);
        match e.storage().instance().get::<_, Delegation>(&key) {
            Some(d) => {
                if d.revoked {
                    AttestationStatus::Revoked
                } else {
                    AttestationStatus::Active
                }
            }
            None => AttestationStatus::NotFound,
        }
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        pausable::is_paused(&e)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        pausable::execute_pause_proposal(&e, proposal_id)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_pausable;
