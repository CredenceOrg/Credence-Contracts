use soroban_sdk::{contracttype, Address, Env};

pub const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Represents a registry entry mapping an identity to their bond contract
///
///  Invariants:
///  - An entry is either active or inactive; the active flag is the
///    authoritative source of truth for whether a lookup should succeed.
///  - `registered_at` is immutable once set and must never be zero for a
///    z  successfully persisted entry.
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