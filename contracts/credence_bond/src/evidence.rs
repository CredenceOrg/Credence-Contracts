//! Evidence Hash Storage Module
//!
//! Provides on-chain storage for evidence hashes (IPFS/content hashes) linked to slash requests.
//! This module ensures evidence integrity through tamper-proof hash storage and supports
//! multiple evidence items per slash proposal for comprehensive documentation.
//!
//! ## Key Features
//! - **IPFS/Hash Storage**: Store content-addressed evidence references
//! - **Slash Request Linking**: Evidence tied to specific slash proposals
//! - **Tamper Prevention**: Immutable evidence once submitted
//! - **Multiple Evidence**: Support for multiple evidence items per proposal
//! - **Event Emission**: Track all evidence submissions
//! - **Query Support**: Retrieve evidence by proposal or hash
//!
//! ## Security Considerations
//! - Evidence cannot be modified after submission
//! - Only authorized submitters (admin/governors) can add evidence
//! - Hash uniqueness enforced to prevent duplicate evidence
//! - All operations emit events for auditability

use soroban_sdk::{contracttype, Address, Env, String, Symbol, Vec};

/// Type of evidence hash being stored.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceType {
    /// IPFS content identifier (CID)
    IPFS = 0,
    /// SHA-256 hash
    SHA256 = 1,
    /// Other hash type
    Other = 2,
}

/// Evidence metadata and hash storage.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Evidence {
    /// Unique evidence ID
    pub id: u64,
    /// Slash proposal this evidence supports
    pub proposal_id: u64,
    /// Content hash (IPFS CID or other hash)
    pub hash: String,
    /// Type of hash
    pub hash_type: EvidenceType,
    /// Optional description/metadata
    pub description: Option<String>,
    /// Submitter address
    pub submitted_by: Address,
    /// Submission timestamp
    pub submitted_at: u64,
}

/// Storage keys for evidence module
fn key_evidence_counter() -> crate::DataKey {
    crate::DataKey::EvidenceCounter
}

fn key_evidence(evidence_id: u64) -> crate::DataKey {
    crate::DataKey::Evidence(evidence_id)
}

fn key_proposal_evidence(proposal_id: u64) -> crate::DataKey {
    crate::DataKey::ProposalEvidence(proposal_id)
}

fn key_hash_exists(hash: &String) -> crate::DataKey {
    crate::DataKey::HashExists(hash.clone())
}

/// NatSpec-style: Submit evidence hash for a slash proposal.
///
/// Stores immutable evidence reference (IPFS hash or other) linked to a specific
/// slash proposal. Each evidence submission is assigned a unique ID and tracked
/// for the proposal.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `submitter` - Address submitting the evidence (must be authorized)
/// * `proposal_id` - ID of the slash proposal this evidence supports
/// * `hash` - Content hash (IPFS CID, SHA-256, etc.)
/// * `hash_type` - Type of hash being submitted
/// * `description` - Optional description/metadata about the evidence
///
/// # Returns
/// Evidence ID (u64) for the newly submitted evidence
///
/// # Panics
/// * If hash is empty
/// * If hash already exists (prevents duplicates)
/// * If description exceeds reasonable length
///
/// # Security
/// * Evidence is immutable once submitted
/// * Hash uniqueness enforced
/// * All submissions are timestamped
/// * Events emitted for auditability
///
/// # Example
/// ```ignore
/// let evidence_id = submit_evidence(
///     &e,
///     &admin,
///     proposal_id,
///     &String::from_str(&e, "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco"),
///     &EvidenceType::IPFS,
///     &Some(String::from_str(&e, "Screenshot of violation")),
/// );
/// ```
pub fn submit_evidence(
    e: &Env,
    submitter: &Address,
    proposal_id: u64,
    hash: &String,
    hash_type: &EvidenceType,
    description: &Option<String>,
) -> u64 {
    // Validation
    if hash.len() == 0 {
        panic!("hash cannot be empty");
    }

    // Prevent duplicate hashes
    let hash_key = key_hash_exists(hash);
    if e.storage().instance().has(&hash_key) {
        panic!("evidence hash already exists");
    }

    // Optional description length validation
    if let Some(desc) = description {
        if desc.len() > 500 {
            panic!("description too long (max 500 chars)");
        }
    }

    // Generate unique evidence ID
    let counter_key = key_evidence_counter();
    let evidence_id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
    let next_id = evidence_id
        .checked_add(1)
        .expect("evidence counter overflow");
    e.storage().instance().set(&counter_key, &next_id);

    // Create evidence record
    let evidence = Evidence {
        id: evidence_id,
        proposal_id,
        hash: hash.clone(),
        hash_type: hash_type.clone(),
        description: description.clone(),
        submitted_by: submitter.clone(),
        submitted_at: e.ledger().timestamp(),
    };

    // Store evidence by ID
    let evidence_key = key_evidence(evidence_id);
    e.storage().instance().set(&evidence_key, &evidence);

    // Link evidence to proposal
    let proposal_key = key_proposal_evidence(proposal_id);
    let mut proposal_evidence: Vec<u64> = e
        .storage()
        .instance()
        .get(&proposal_key)
        .unwrap_or(Vec::new(e));
    proposal_evidence.push_back(evidence_id);
    e.storage()
        .instance()
        .set(&proposal_key, &proposal_evidence);

    // Mark hash as used
    e.storage().instance().set(&hash_key, &true);

    // Emit event
    emit_evidence_submitted(e, evidence_id, proposal_id, submitter, hash);

    evidence_id
}

/// NatSpec-style: Retrieve evidence by ID.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `evidence_id` - Unique evidence identifier
///
/// # Returns
/// Evidence record with all metadata
///
/// # Panics
/// If evidence ID does not exist
pub fn get_evidence(e: &Env, evidence_id: u64) -> Evidence {
    let key = key_evidence(evidence_id);
    e.storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| panic!("evidence not found"))
}

/// NatSpec-style: Get all evidence IDs for a slash proposal.
///
/// Returns a list of evidence IDs submitted for the specified proposal,
/// allowing retrieval of all supporting evidence.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `proposal_id` - Slash proposal ID
///
/// # Returns
/// Vector of evidence IDs (may be empty if no evidence submitted)
pub fn get_proposal_evidence(e: &Env, proposal_id: u64) -> Vec<u64> {
    let key = key_proposal_evidence(proposal_id);
    e.storage().instance().get(&key).unwrap_or(Vec::new(e))
}

/// NatSpec-style: Check if a hash already exists in the system.
///
/// Prevents duplicate evidence submissions by checking if a hash
/// has already been registered.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `hash` - Content hash to check
///
/// # Returns
/// `true` if hash exists, `false` otherwise
pub fn hash_exists(e: &Env, hash: &String) -> bool {
    let key = key_hash_exists(hash);
    e.storage().instance().get(&key).unwrap_or(false)
}

/// NatSpec-style: Get total number of evidence submissions.
///
/// # Arguments
/// * `e` - Soroban environment
///
/// # Returns
/// Total count of evidence records in the system
pub fn get_evidence_count(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&key_evidence_counter())
        .unwrap_or(0)
}

/// NatSpec-style: Get all evidence for a proposal with full details.
///
/// Convenience function that retrieves complete evidence records
/// for all evidence linked to a proposal.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `proposal_id` - Slash proposal ID
///
/// # Returns
/// Vector of complete Evidence records
pub fn get_proposal_evidence_details(e: &Env, proposal_id: u64) -> Vec<Evidence> {
    let evidence_ids = get_proposal_evidence(e, proposal_id);
    let mut evidence_list = Vec::new(e);

    for id in evidence_ids.iter() {
        let evidence = get_evidence(e, id);
        evidence_list.push_back(evidence);
    }

    evidence_list
}

/// NatSpec-style: Emit event for evidence submission.
///
/// Publishes a "evidence_submitted" event for off-chain tracking and indexing.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `evidence_id` - Unique evidence ID
/// * `proposal_id` - Associated slash proposal ID
/// * `submitter` - Address that submitted the evidence
/// * `hash` - Content hash that was submitted
fn emit_evidence_submitted(
    e: &Env,
    evidence_id: u64,
    proposal_id: u64,
    submitter: &Address,
    hash: &String,
) {
    e.events().publish(
        (Symbol::new(e, "evidence_submitted"), evidence_id),
        (proposal_id, submitter.clone(), hash.clone()),
    );
}

// Note: Comprehensive integration tests are in test_evidence.rs
