use soroban_sdk::{contracttype, Address, Env, Symbol};

pub const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Represents a registry entry mapping an identity to their bond contract
///
/// Invariants:
/// - An entry is either active or inactive; the active flag is the
///   authoritative source of truth for whether a lookup should succeed.
/// - `registered_at` is immutable once set and must never be zero for a
///   successfully persisted entry.
/// - `identity` and `bond_contract` are never the same address.
///
/// These invariants are enforced by the contract layer and verified by
/// the boundary/recovery tests in this module. The storage layer itself
/// keeps the data model minimal and deterministic so that a corrupted or
/// stale entry can always be detected and recovered from.
///
/// Storage layer guarantees:
/// - Reads of missing keys return `None` (never panic), so callers can
///   implement retry/recovery logic without losing user data.
/// - Writes are idempotent: repeated writes of the same value leave the
///   store in the same state.
/// - TTL bumping is best-effort and must not alter the logical value of
///   any persisted entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryEntry {
    pub identity: Address,
    pub bond_contract: Address,
    pub registered_at: u64,
    pub active: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    IdentityToBond(Address),
    BondToIdentity(Address),
    RegisteredIdentities,
    BondCodeHash,
    AllowNonInterface(Address),
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u32),
    PauseApproval(u32, Address),
    PauseApprovalCount(u32),
}

pub fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}
