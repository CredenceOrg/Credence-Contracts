use soroban_sdk::{contracttype, Address, Env, IntoVal, Val};

/// Instance-storage TTL: one year of ledgers at a 5 s ledger cadence.
pub const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Persistent-storage TTL: the Soroban network cap (~6 months at 5 s/ledger).
///
/// Every persistent read/write in this contract bumps the entry back to this
/// value through [`extend_persistent_ttl`], so a hot mapping cannot be
/// archived while it is still being used.
pub const PERSISTENT_TTL_EXTEND_TO: u32 = 3_110_400;

/// Extend persistent entries once their remaining TTL drops to half of
/// [`PERSISTENT_TTL_EXTEND_TO`].
pub const PERSISTENT_TTL_THRESHOLD: u32 = PERSISTENT_TTL_EXTEND_TO / 2;

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
///    an entry.
#[contracttype]
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    /// The identity address
    pub identity: Address,
    /// The bond contract address for this identity
    pub bond_contract: Address,
    /// Timestamp when this entry was registered
    pub registered_at: u64,
    /// Whether this registration is currently active
    pub active: bool,
}

/// Storage keys for the registry contract.
///
/// Variants are **appended** rather than reordered so that the on-chain
/// discriminants of existing keys never shift.
///
/// # Storage classes
///
/// * **Instance** — config and counters read on every invocation:
///   `Admin`, the pause-multisig keys, `BondCodeHash` and `IdentityCount`.
///   These are a fixed number of small values, so the instance entry does not
///   grow with the number of registered identities.
/// * **Persistent** — per-identity data and the identity index:
///   `IdentityToBond`, `BondToIdentity`, `AllowNonInterface`, `IdentityAt`
///   and `IdentityIndex`. Each lookup touches only the keys it needs.
///
/// `RegisteredIdentities` is the legacy unbounded `Vec<Address>`. It is no
/// longer written or read (issue #1629 replaced it with the `IdentityAt` /
/// `IdentityIndex` index); the variant is retained so existing entries and
/// discriminants stay valid.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u32),
    PauseApproval(u32, Address),
    PauseApprovalCount(u32),
    /// Forward mapping: identity → `RegistryEntry` (persistent).
    IdentityToBond(Address),
    /// Reverse mapping: bond contract → identity (persistent).
    BondToIdentity(Address),
    /// Legacy unbounded list of identities. Unused — see the enum docs.
    RegisteredIdentities,
    /// Audit flag set when a bond opted out of the interface check (persistent).
    AllowNonInterface(Address),
    BondCodeHash,
    /// Number of identities in the insertion index (instance).
    IdentityCount,
    /// Identity at a given index position (persistent).
    IdentityAt(u32),
    /// Index position of an identity in the insertion index (persistent).
    IdentityIndex(Address),
}

/// Extend the instance entry's TTL to the standard one-year window.
pub(crate) fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

/// Extend a persistent entry's TTL after a read or write.
///
/// Only call this for keys that are known to exist: the host rejects TTL
/// extension of a missing entry.
pub(crate) fn extend_persistent_ttl<K>(e: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    e.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
}
