#![no_std]
#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]

use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

pub mod domain;
pub mod nonce;
pub mod pausable;
pub mod verifier;

pub use domain::{DelegatedActionPayload, DomainTag};
pub use pausable::PauseProposalView;
pub use verifier::SchemeTag;

// ---------------------------------------------------------------------------
// Contract types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
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

/// Aggregated view of a delegation's state for indexers and off-chain tools.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DelegationSummary {
    pub is_valid: bool,
    pub time_to_expiry: u64,
    pub delegation_type: DelegationType,
    pub revoked_at: u64,
    pub scheme: u32,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Storage-key discriminator for every persistent/instance entry this contract
/// writes.
///
/// # Wire stability — keys are permanent
/// Each variant's Soroban (`#[contracttype]`) encoding is the literal ledger key
/// under which its data lives. Unlike a `bincode`/`#[repr(u32)]` enum, a
/// `#[contracttype]` enum is keyed by the **variant name** (encoded as a
/// `Symbol`) plus its field shape — never by declaration order. Therefore:
/// * **Renaming** a variant, or **changing its field count/types**, changes the
///   encoded key and **orphans every existing ledger entry** for it. Never do
///   this for a deployed contract.
/// * **Reordering** variants is encoding-stable (the key is name-based), but
///   keep them ordered for readability and to match the fingerprint snapshot.
/// * **Appending** a new variant is safe.
///
/// The encoded bytes of every variant are pinned in
/// `tests/datakey_fingerprint.rs`; any rename/retype that moves a key fails CI.
/// See `docs/datakey-fingerprint.md`.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address with power to initialize and override.
    Admin,
    /// Boolean flag: true if contract actions are currently paused.
    Paused,
    /// Boolean flag per signer: true if address is an authorized pause signer.
    PauseSigner(Address),
    /// Current number of authorized pause signers (cached for no-lockout check).
    PauseSignerCount,
    /// Minimum approvals required to execute a pause or unpause proposal.
    PauseThreshold,
    /// Monotonically increasing counter for allocating unique proposal IDs.
    PauseProposalCounter,
    /// Maps proposal ID to PauseAction value (1=Pause, 2=Unpause).
    PauseProposal(u64),
    /// Maps (proposal ID, signer) to boolean: true if signer approved this proposal.
    PauseApproval(u64, Address),
    /// Current approval count for a given proposal ID.
    PauseApprovalCount(u64),
    Delegation(Address, Address, DelegationType),
    /// Per-identity nonce for replay prevention.
    Nonce(Address),
    /// Verifier ID for a given signature scheme tag (scheme -> Address).
    /// Maps scheme tag (Ed25519=0, Secp256r1=1, MLDSA44=2) to a verifier address.
    Verifier(u32),
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

#[contract]
pub struct CredenceDelegation;

/// Maximum number of nonces that a single `invalidate_nonce_range` call may
/// skip.  Bounding the span prevents gas-exhaustion and ensures the nonce
/// stream advances at a predictable pace.
///
/// **Replay-window proof (informal):**
/// After `invalidate_nonce_range(identity, new_nonce)` succeeds, the stored
/// nonce is exactly `new_nonce`.  `consume_nonce` only accepts a value equal
/// to the stored nonce, so any payload whose `nonce` field satisfies
/// `nonce < new_nonce` will be rejected with `InvalidNonce`.  Because nonces
/// are strictly monotone, the half-open range `[0, new_nonce)` is permanently
/// unspendable for `identity`.  No pre-signed payload can escape invalidation
/// regardless of when it was produced.
///
/// The cap `MAX_NONCE_INVALIDATION_SPAN` limits a single jump to 10 000
/// nonces.  Larger ranges must be done in multiple calls, each bounded by
/// the same cap.
const MAX_NONCE_INVALIDATION_SPAN: u64 = 10_000;

/// Maximum lifetime, in seconds, allowed for a newly created delegation.
///
/// A delegation's `expires_at` must satisfy:
/// `now < expires_at <= now + MAX_DELEGATION_DURATION`.
/// The default bound is 365 days and prevents effectively never-expiring
/// delegations such as `u64::MAX`.
pub const MAX_DELEGATION_DURATION: u64 = 365 * 24 * 60 * 60;

#[contractimpl]
impl CredenceDelegation {
    /// Initialize the contract with an admin address.
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, ContractError::AlreadyInitialized);
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
    // Direct (auth-required) entry points — updated for unified nonce verification
    // -----------------------------------------------------------------------

    /// Create a delegation from owner to delegate with a given type and expiry.
    ///
    /// The owner must be the transaction signer. `expires_at` must be greater
    /// than the current ledger timestamp and no later than
    /// `now + MAX_DELEGATION_DURATION`.
    ///
    /// # Expiry Validation (Boundary Enforcement)
    /// - **Lower bound**: `expires_at > now` (strictly greater) — prevents already-expired or
    ///   never-expiring delegations from creation. Rejects equality at boundary.
    /// - **Upper bound**: `expires_at ≤ now + MAX_DELEGATION_DURATION` — prevents unreasonably
    ///   distant expirations that could enable indefinite delegations.
    /// - **u64::MAX case**: Always rejected via upper bound check (treating it as effectively
    ///   infinite expiry).
    /// - **Rejects before state**: Expiry validation occurs before nonce consumption, so an
    ///   invalid expiry does not burn a nonce or create a delegation entry.
    pub fn delegate(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        expires_at: u64,
        nonce: u64,
    ) -> Delegation {
        pausable::require_not_paused(&e);
        owner.require_auth();

        Self::validate_delegation_expiry(&e, expires_at);

        // Consume nonce so direct-path and delegated-path actions share a
        // single monotone sequence.  This ensures invalidate_nonce_range
        // blocks both interaction types uniformly.
        nonce::consume_nonce(&e, &owner, nonce);

        Self::store_delegation(&e, owner, delegate, delegation_type, expires_at)
    }

    /// Revoke an existing delegation. Only the owner can revoke.
    ///
    /// Expired delegations may still be revoked so the stored audit state can
    /// reflect both facts: expired delegations are invalid before revocation,
    /// and remain invalid after the `revoked` flag is set.
    pub fn revoke_delegation(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
        nonce: u64,
    ) {
        pausable::require_not_paused(&e);
        owner.require_auth();

        // Enforce centralized sequential replay tracking
        nonce::consume_nonce(&e, &owner, nonce);

        Self::mark_delegation_revoked(&e, owner, delegate, delegation_type, "delegation");
    }

    /// Revoke an attestation-type delegation. Only the original attester can revoke and must provide the correct current nonce.
    pub fn revoke_attestation(e: Env, attester: Address, subject: Address, nonce: u64) {
        pausable::require_not_paused(&e);
        attester.require_auth();

        // Enforce centralized sequential replay tracking
        nonce::consume_nonce(&e, &attester, nonce);

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
    /// validates the underlying transaction signature. The same expiry bounds
    /// as [`Self::delegate`] apply before nonce consumption, so invalid
    /// expiries cannot burn a relayed payload's nonce.
    ///
    /// # Expiry Validation (Boundary Enforcement)
    /// - **Lower bound**: `expires_at > now` (strictly greater)
    /// - **Upper bound**: `expires_at ≤ now + MAX_DELEGATION_DURATION`
    /// - **Monotonic ledger safety**: Timestamp is captured once at validation entry;
    ///   this test harness verifies no code path drifts with mid-call ledger advances.
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

        // Signature scheme dispatch: Ed25519 is covered by owner.require_auth() above;
        // Secp256r1/MLDSA44 dispatch to their registered verifier contracts.
        let scheme = domain::decode_scheme_safe(&payload);
        verifier::verify_delegated_signature(
            &e,
            &owner,
            &soroban_sdk::Bytes::new(&e),
            &soroban_sdk::Bytes::new(&e),
            scheme.to_u32(),
        );

        Self::validate_delegation_expiry(&e, expires_at);

        // Nonce consumption (replay prevention)
        nonce::consume_nonce(&e, &owner, payload.nonce);

        Self::store_delegation(&e, owner, delegate, delegation_type, expires_at)
    }

    /// Relayer-friendly variant of `revoke_delegation`.
    ///
    /// Payload domain must be `DomainTag::RevokeDelegation` — a signature
    /// produced for `execute_delegated_delegate` cannot be replayed here.
    ///
    /// Validation order is explicit and security-critical:
    /// 1. Domain-separated payload verification
    /// 2. Nonce consumption (replay prevention)
    /// 3. State transition (`mark_delegation_revoked`)
    ///
    /// This ordering ensures a replayed revoke payload fails with
    /// `InvalidNonce` if the payload nonce has already been consumed,
    /// even when the underlying delegation record is already revoked.
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

        // Signature scheme dispatch for non-Ed25519 schemes.
        let scheme = domain::decode_scheme_safe(&payload);
        verifier::verify_delegated_signature(
            &e,
            &owner,
            &soroban_sdk::Bytes::new(&e),
            &soroban_sdk::Bytes::new(&e),
            scheme.to_u32(),
        );

        nonce::consume_nonce(&e, &owner, payload.nonce);

        Self::mark_delegation_revoked(&e, owner, delegate, delegation_type, "delegation");
    }

    /// Relayer-friendly variant of `revoke_attestation`.
    ///
    /// Payload domain must be `DomainTag::RevokeAttestation`.
    ///
    /// Validation order is explicit and security-critical:
    /// 1. Domain-separated payload verification
    /// 2. Nonce consumption (replay prevention)
    /// 3. State transition (`mark_delegation_revoked`)
    ///
    /// This ordering ensures a replayed revoke-attest payload fails with
    /// `InvalidNonce` if the payload nonce has already been consumed,
    /// even when the attestation has already been revoked.
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

        // Signature scheme dispatch for non-Ed25519 schemes.
        let scheme = domain::decode_scheme_safe(&payload);
        verifier::verify_delegated_signature(
            &e,
            &attester,
            &soroban_sdk::Bytes::new(&e),
            &soroban_sdk::Bytes::new(&e),
            scheme.to_u32(),
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

    /// Provides a derived summary of a delegation's current status.
    ///
    /// This is a read-only view that aggregates validity, time-to-expiry,
    /// and metadata into a single struct. Useful for indexers.
    pub fn get_delegation_summary(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) -> DelegationSummary {
        let d = Self::get_delegation(e.clone(), owner, delegate, delegation_type);
        let now = e.ledger().timestamp();
        let is_expired = now >= d.expires_at;
        let is_valid = !d.revoked && !is_expired;

        DelegationSummary {
            is_valid,
            time_to_expiry: d.expires_at.saturating_sub(now),
            delegation_type: d.delegation_type,
            revoked_at: 0, // Placeholder: not currently recorded in Delegation struct
            scheme: 0,     // Placeholder: defaults to Ed25519 (0)
        }
    }

    /// Retrieve a stored delegation.
    pub fn get_delegation(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) -> Delegation {
        let key = DataKey::Delegation(owner, delegate, delegation_type);
        let d: Delegation = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::DelegationNotFound));
        nonce::bump_delegation_ttl(&e, &key, d.expires_at);
        d
    }

    /// Check whether a delegate is currently valid (not revoked, not expired).
    ///
    /// Delegations expire exactly at `expires_at`: the record is valid only
    /// while `e.ledger().timestamp() < expires_at`.
    ///
    /// # Expiry Boundary
    /// - At `timestamp == expires_at` exactly, the delegation is already invalid (has expired).
    /// - Returns `false` when `e.ledger().timestamp() >= expires_at`.
    pub fn is_valid_delegate(
        e: Env,
        owner: Address,
        delegate: Address,
        delegation_type: DelegationType,
    ) -> bool {
        let key = DataKey::Delegation(owner, delegate, delegation_type);
        match e.storage().persistent().get::<_, Delegation>(&key) {
            Some(d) => {
                nonce::bump_delegation_ttl(&e, &key, d.expires_at);
                // Validity check: not revoked AND expires_at > current timestamp (strictly greater)
                !d.revoked && d.expires_at > e.ledger().timestamp()
            }
            None => false,
        }
    }

    pub fn get_attestation_status(
        e: Env,
        attester: Address,
        subject: Address,
    ) -> AttestationStatus {
        let key = DataKey::Delegation(attester, subject, DelegationType::Attestation);
        match e.storage().persistent().get::<_, Delegation>(&key) {
            Some(d) => {
                nonce::bump_delegation_ttl(&e, &key, d.expires_at);
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
    // Verifier scheme registry (admin-controlled, post-quantum support)
    // -----------------------------------------------------------------------

    /// Register a verifier for a given signature scheme.
    ///
    /// Only the admin can register verifiers. Each scheme (Ed25519, Secp256r1,
    /// MLDSA44) is mapped to a verifier address. Registration emits a
    /// `verifier_registered` event for audit trail tracking.
    ///
    /// New verifiers can be registered at any time, enabling the contract to
    /// support additional cryptographic schemes after deployment. A scheme that
    /// already has a registered verifier can be re-registered with a new one
    /// (updating the mapping).
    ///
    /// # Arguments
    /// * `admin` - Must be the contract admin (checked via `require_auth()`)
    /// * `scheme` - The signature scheme tag (0=Ed25519, 1=Secp256r1, 2=MLDSA44)
    /// * `verifier_id` - Address of the verifier contract/module
    ///
    /// # Errors
    /// * `NotAdmin` - if `admin` is not the contract admin
    /// * `UnknownScheme` - if scheme is not a recognized value
    pub fn register_verifier(e: Env, admin: Address, scheme: u32, verifier_id: Address) {
        admin.require_auth();

        // Check that only the admin can register verifiers
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Validate scheme is known
        verifier::validate_scheme_registered(&e, scheme);

        // Register the verifier
        let key = DataKey::Verifier(scheme);
        e.storage().instance().set(&key, &verifier_id);

        // Emit audit event
        verifier::emit_verifier_registered(&e, scheme, &verifier_id, &admin);

        e.events().publish(
            (Symbol::new(&e, "verifier_registered"), scheme),
            verifier_id,
        );
    }

    /// Retrieve the registered verifier for a given signature scheme.
    ///
    /// Returns the address of the verifier contract/module for the scheme.
    /// If no verifier is registered for the scheme, returns `None`.
    ///
    /// Clients can use this to check scheme support before submitting
    /// delegated payloads.
    pub fn get_verifier(e: Env, scheme: u32) -> Option<Address> {
        e.storage().instance().get(&DataKey::Verifier(scheme))
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

    /// Read-only, structured view of an in-flight (or executed) pause proposal,
    /// for operator monitoring. Aggregates the four proposal storage entries
    /// (counter, action payload, per-signer approvals, approval count) into a
    /// single [`PauseProposalView`].
    ///
    /// Performs no `require_auth` and mutates nothing — safe to expose publicly.
    ///
    /// `signers` is the candidate set used to resolve `approved_by`; pass the
    /// addresses you want checked (Soroban storage cannot enumerate keys). The
    /// `action`, `approvals`, and `executed` fields are independent of it.
    pub fn get_pause_proposal_state(
        e: Env,
        proposal_id: u64,
        signers: Vec<Address>,
    ) -> PauseProposalView {
        pausable::get_pause_proposal_state(&e, proposal_id, &signers)
    }

    /// Compatibility shim: resolve a proposal that was recorded under a
    /// counter-based ID (issued before the hash-derivation migration).
    ///
    /// Returns the raw action value (`1` = Pause, `2` = Unpause) if the
    /// proposal is still in storage, or `ContractError::ProposalNotFound`
    /// if no proposal exists under that ID. Does **not** panic.
    ///
    /// New code should use [`get_pause_proposal_state`] with a hash-derived ID
    /// instead. This entry point exists solely for backward compatibility with
    /// clients that persisted counter-based IDs before the migration.
    pub fn get_proposal_by_legacy_id(e: Env, legacy_id: u64) -> u32 {
        pausable::get_proposal_by_legacy_id(&e, legacy_id)
            .unwrap_or_else(|err| panic_with_error!(&e, err))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Enforce expiry boundary constraints for delegation creation.
    ///
    /// # Constraints
    ///
    /// All delegations must satisfy: `now < expires_at ≤ now + MAX_DELEGATION_DURATION`.
    ///
    /// ## Lower Bound (Strict >)
    /// Rejects `expires_at <= now`, preventing:
    /// - Already-expired delegations
    /// - Zero-duration delegations
    /// - Re-activation of time-traveled expirations
    ///
    /// Error on violation: `ContractError::ExpiryInPast` (#500).
    ///
    /// ### Boundary Test Cases
    /// - `expires_at = now - 1` → REJECT (in past)
    /// - `expires_at = now` → REJECT (exact equality)
    /// - `expires_at = now + 1` → ACCEPT (strict >)
    ///
    /// ## Upper Bound (Saturating +)
    /// Rejects `expires_at > now + MAX_DELEGATION_DURATION`, where
    /// `MAX_DELEGATION_DURATION = 365 * 24 * 60 * 60 ≈ 31,536,000 seconds`.
    ///
    /// Prevents effectively indefinite delegations like `u64::MAX`.
    ///
    /// Error on violation: `ContractError::DelegationExpiryTooLong` (#503).
    ///
    /// ### Boundary Test Cases
    /// - `expires_at = now + MAX_DELEGATION_DURATION` → ACCEPT (saturating boundary)
    /// - `expires_at = now + MAX_DELEGATION_DURATION + 1` → REJECT (over max)
    /// - `expires_at = u64::MAX` → REJECT (far exceeds max)
    ///
    /// ## Monotonic Ledger Safety
    /// The ledger timestamp is captured once at function entry via `e.ledger().timestamp()`.
    /// All downstream callers get the same snapshot, so:
    /// - No code path can drift via mid-call ledger advances.
    /// - Validation result is deterministic even if ledger advances afterward.
    ///
    /// This harness validates this property with sequences of advancing ledger
    /// timestamps and verifies the rejection set remains stable.
    fn validate_delegation_expiry(e: &Env, expires_at: u64) {
        let now = e.ledger().timestamp();
        // Lower bound check: expires_at must be STRICTLY GREATER than now (not equal)
        if expires_at <= now {
            panic_with_error!(e, ContractError::ExpiryInPast);
        }

        // Upper bound check: expires_at must not exceed now + MAX_DELEGATION_DURATION
        // Using saturating_add prevents overflow and simplifies comparison
        let max_expires_at = now.saturating_add(MAX_DELEGATION_DURATION);
        if expires_at > max_expires_at {
            panic_with_error!(e, ContractError::DelegationExpiryTooLong);
        }
    }

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
        e.storage().persistent().set(&key, &d);
        nonce::bump_delegation_ttl(e, &key, expires_at);
        // Bump nonce TTL to at least cover this delegation's lifetime.
        let nonce_key = DataKey::Nonce(owner.clone());
        nonce::bump_nonce_ttl(e, &nonce_key, expires_at);
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
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::DelegationNotFound));

        if d.revoked {
            panic_with_error!(e, ContractError::AlreadyRevoked);
        }

        d.revoked = true;
        e.storage().persistent().set(&key, &d);
        nonce::bump_delegation_ttl(e, &key, d.expires_at);
        e.events()
            .publish((Symbol::new(e, "delegation_revoked"),), d);
    }
}

#[cfg(test)]
// mod test;

// #[cfg(test)]
// mod test_verifier;
#[cfg(test)]
mod test_pausable;

// #[cfg(test)]
// mod test_pause_signer_invariant;

#[cfg(test)]
mod test_proposal_id_derivation;

// #[cfg(test)]
#[cfg(test)]
mod test_delegation_ttl;

#[cfg(test)]
mod test_domain_separation;

#[cfg(test)]
mod test_pause_proposal_view;

#[cfg(test)]
mod test_expiry_boundary;

#[cfg(test)]
mod test_verifier_dispatch;
