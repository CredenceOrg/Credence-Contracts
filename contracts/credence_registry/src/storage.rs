use soroban_sdk::{contracttype, Address, Env};

public const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Represents a registry entry mapping an identity to their bond contract
///
///  Invariants:
///  - An entry is either active or inactive; the active flag is the
///    authoritative source of truth for whether a lookup should succeed.
///  - `registered_at` is immutable once set and must never be zero for a
///    successfully persisted entry.
///  - `identity` and `bond_contract` are never the same address.
///
///  These invariants are enforced by the contract layer and verified by
///  the boundary/recovery tests in this module. The storage layer itself
///  keeps the data model minimal and deterministic so that a corrupted or
///  stale entry can always be detected and recovered from.
///
///  Storage layer guarantees:
///  - Reads of missing keys return `None` (never panic), so callers can
///    implement retry/recovery logic without losing user data.
///  - Writes are idempotent: repeated writes of the same value leave the
///    store in the same state.
///  - TTL bumping is best-effort and must not alter the logical value of
///    the stored entry.
///
///  The data model is deliberately flat: a single `struct` per identity
///  rather than nested maps, so that entries can be inspected and
///  migrated individually.
///
///  See `docs/STORAGE_KEY_LAYOUT.md` for the canonical key layout.
///
///  The `DataKey` enum below is `#[contracttype]` encoded by variant name,
///  so variant names and payload types must not change without a
///  storage migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryEntry {
    pub public identity: Address,
    public bond_contract: Address,
    public registered_at: u64,
    public active: bool,
}

/// Storage keys for the registry contract.
///
///  Variant names and payload types are part of the on-chain encoding and
///  must match `docs/STORAGE_KEY_LAYOUT.md`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Administrator address.
    Admin,
    /// Maps an identity to the address of its bond contract.
    IdentityToBond(Address),
    /// Reverse mapping from a bond contract to its identity.
    BondToIdentity(Address),
    /// Count of registered identities.
    RegisteredIdentities,
    /// Wasm code hash of the bond contract.
    BondCodeHash,
    /// Per-address flag allowing non-interface identities.
    AllowNonInterface(Address),
    /// Global pause flag.
    Paused,
    /// Address authorized to sign pause proposals.
    PauseSigner(Address),
    /// Count of authorized pause signers.
    PauseSignerCount,
    /// Number of signer approvals required to pause.
    PauseThreshold,
    /// Monotonic counter for pause proposal ids.
    PauseProposalCounter,
    /// A pause proposal identified by a u32 id.
    PauseProposal(u32),
    /// A signer's approval of a pause proposal.
    PauseApproval(u32, Address),
    /// Count of approvals for a pause proposal.
    PauseApprovalCount(u32),
}

/// Extend the instance storage TTL using the same halfway threshold
/// pattern as the other contracts in this workspace.
pub fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}
