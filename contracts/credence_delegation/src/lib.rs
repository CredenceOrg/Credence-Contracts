#![allow(deprecated)]
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

pub mod domain;
pub mod nonce;
pub mod pausable;

pub use domain::{DelegatedActionPayload, DomainTag};

// ---------------------------------------------------------------------------
// Contract types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

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
    /// Per-identity nonce for replay prevention.
    Nonce(Address),
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

#[contract]
pub struct CredenceDelegation;

const MAX_NONCE_INVALIDATION_SPAN: u64 = 10_000;

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

    // -----------------------------------------------------------------------
    // Direct (auth-required) entry points — no off-chain signature needed
    // -----------------------------------------------------------------------

    /// Create a delegation from owner to delegate with a given type and expiry.
    /// The owner must be the transaction signer.
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

        Self::store_delegation(&e, owner, delegate, delegation_type, expires_at)
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

        Self::mark_delegation_revoked(&e, owner, delegate, delegation_type, "delegation");
    }

    /// Revoke an attestation-type delegation.  Only the original attester can revoke.
    pub fn revoke_attestation(e: Env, attester: Address, subject: Address) {
        pausable::require_not_paused(&e);
        attester.require_auth();

        Self::mark_delegation_revoked(
            &e,
            attester,
            subject,
            DelegationType::Attestation,
            "attestation",
        );
    }

    // -----------------------------------------------------------------------
    // Delegated (relayer) entry points — explicit domain-separated payload
    // -----------------------------------------------------------------------

    /// Relayer-friendly variant of `delegate`.
    ///
    /// A relayer submits a [`DelegatedActionPayload`] that was produced and
    /// signed off-chain by `owner`.  The payload must carry:
    ///
    /// * `domain = DomainTag::Delegate` — prevents replay in revoke functions
    /// * `owner`      — the actual authority
    /// * `target`     — must equal `delegate`
    /// * `contract_id`— must match this deployment (prevents cross-contract replay)
    /// * `nonce`      — consumed and incremented on success
    ///
    /// `owner.require_auth()` is still called so the Soroban auth engine
    /// validates the underlying transaction signature.
    pub fn execute_delegated_delegate(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        expires_at: u64,
        payload: DelegatedActionPayload,
    ) -> Delegation {
        pausable::require_not_paused(&e);
        owner.require_auth();

        // Domain-separated payload verification
        domain::verify_payload(&e, &payload, DomainTag::Delegate, &owner, &delegate);

        // Nonce consumption (replay prevention)
        nonce::consume_nonce(&e, &owner, payload.nonce);

        if expires_at <= e.ledger().timestamp() {
            panic!("expiry must be in the future");
        }

        Self::store_delegation(&e, owner, delegate, delegation_type, expires_at)
    }

    /// Relayer-friendly variant of `revoke_delegation`.
    ///
    /// Payload domain must be `DomainTag::RevokeDelegation` — a signature
    /// produced for `execute_delegated_delegate` cannot be replayed here.
    pub fn execute_delegated_revoke(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        payload: DelegatedActionPayload,
    ) {
        pausable::require_not_paused(&e);
        owner.require_auth();

        domain::verify_payload(&e, &payload, DomainTag::RevokeDelegation, &owner, &delegate);
        nonce::consume_nonce(&e, &owner, payload.nonce);

        Self::mark_delegation_revoked(&e, owner, delegate, delegation_type, "delegation");
    }

    /// Relayer-friendly variant of `revoke_attestation`.
    ///
    /// Payload domain must be `DomainTag::RevokeAttestation`.
    pub fn execute_delegated_revoke_attest(
        e: Env,
        attester: Address,
        subject: Address,
        payload: DelegatedActionPayload,
    ) {
        pausable::require_not_paused(&e);
        attester.require_auth();

        domain::verify_payload(
            &e,
            &payload,
            DomainTag::RevokeAttestation,
            &attester,
            &subject,
        );
        nonce::consume_nonce(&e, &attester, payload.nonce);

        Self::mark_delegation_revoked(
            &e,
            attester,
            subject,
            DelegationType::Attestation,
            "attestation",
        );
    }

    // -----------------------------------------------------------------------
    // Query entry points
    // -----------------------------------------------------------------------

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

    /// Return the current nonce for `identity`.  Relayers query this before
    /// building the off-chain payload.
    pub fn get_nonce(e: Env, identity: Address) -> u64 {
        nonce::get_nonce(&e, &identity)
    }

    /// Invalidate a bounded range of nonces for compromised-key recovery.
    ///
    /// Advances nonce to `new_nonce`, invalidating all payloads signed with
    /// nonces in `[current_nonce, new_nonce)`.
    ///
    /// Security properties:
    /// - Only `identity` can invalidate its own nonce stream.
    /// - Nonce remains strictly monotonic (`new_nonce` must be greater).
    /// - Range size is capped to keep gas predictable.
    pub fn invalidate_nonce_range(e: Env, identity: Address, new_nonce: u64) {
        pausable::require_not_paused(&e);
        identity.require_auth();
        let (from_nonce, to_nonce) =
            nonce::invalidate_nonce_range(&e, &identity, new_nonce, MAX_NONCE_INVALIDATION_SPAN);
        e.events().publish(
            (Symbol::new(&e, "nonce_invalidated"), identity),
            (from_nonce, to_nonce),
        );
    }

    // -----------------------------------------------------------------------
    // Pausable pass-throughs
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn store_delegation(
        e: &Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        expires_at: u64,
    ) -> Delegation {
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
            .publish((Symbol::new(e, "delegation_created"),), d.clone());
        d
    }

    fn mark_delegation_revoked(
        e: &Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        kind: &'static str,
    ) {
        let key = DataKey::Delegation(owner.clone(), delegate.clone(), delegation_type.clone());
        let mut d: Delegation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("{} not found", kind));

        if d.revoked {
            panic!("{} already revoked", kind);
        }

        d.revoked = true;
        e.storage().instance().set(&key, &d);
        e.events()
            .publish((Symbol::new(e, "delegation_revoked"),), d);
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_domain_separation;
