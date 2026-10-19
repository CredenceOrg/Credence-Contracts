# Storage Key Naming Convention

This document describes the canonical naming scheme for storage keys in Credence Soroban contracts. Following this convention ensures consistency across the codebase and helps reviewers verify behavior against documented intent.

## Convention

Storage key enums and their variants must follow these rules:

- **Enum names**: Use `snake_case` (e.g., `DataKey`, `SlashStorageKey`, `EmergencyDataKey`)
- **Variant names**: Use `snake_case` with **singular nouns** (e.g., `Admin`, `Token`, `Bond`, `AttestationCounter`)
- **Parameterized variants**: Use descriptive singular names with type parameters (e.g., `Bond(Address)`, `Attestation(u64)`, `SlashRecord(Address, u32)`)

## Examples

### Basic storage key enum

```rust
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Contract administrator
    Admin,
    /// Token address for bond deposits
    Token,
    /// Bond data for a specific identity
    Bond(Address),
    /// Individual attestation record
    Attestation(u64),
    /// Monotonic counter for attestation IDs
    AttestationCounter,
}
```

### Namespace-specific storage keys

```rust
#[contracttype]
#[derive(Clone)]
pub enum SlashStorageKey {
    /// Number of slashes for an identity
    SlashCount(Address),
    /// Individual slash record by (identity, index)
    SlashRecord(Address, u32),
}
```

### Emergency audit storage keys

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmergencyDataKey {
    /// Emergency record by ID
    Record(u64),
    /// State transition record by ID
    Transition(u64),
    /// Monotonic sequence counter
    RecordSeq,
}
```

## Rationale

- **Singular nouns**: `Bond` not `Bonds`, `Admin` not `Admins`. This matches the conceptual model where each key represents a single storage entry or namespace.
- **snake_case**: Consistent with Rust naming conventions for enum variants in this codebase.
- **Descriptive parameterized variants**: `Bond(Address)` clearly indicates the key is indexed by address, making the storage layout self-documenting.

## Arbitration Contract Storage Layout

The arbitration contract separates global configuration from per-entity records:

- **Instance storage** holds only small, global configuration: `Admin`, quorum settings, pause state, and monotonic counters. These entries are loaded on every invocation, so they must stay small and bounded.
- **Persistent storage** holds per-entity `Dispute(u64)` records. Each read and write extends the record's TTL according to the storage TTL policy (clamped to the network maximum).

### Arbitration keys

| Key | Storage | Description |
| --- | --- | --- |
| `Admin` | instance | Contract administrator |
| `Quorum` | instance | Voting quorum configuration |
| `Paused` | instance | Global pause state |
| `DisputeCounter` | instance | Monotonic counter for dispute IDs |
| `Dispute(u64)` | persistent | Individual dispute record, TTL-extended on access |
| `VoterCasted(u64, Address)` | instance | Whether a voter has cast a vote for a dispute |
| `VoterCounter(u64)` | instance | Number of votes cast for a dispute |
| `DisputeVotes(u64)` | instance | Vote tally for a dispute |

Dispute records are never stored in instance storage: doing so would make the cost of every entry point grow with the total number and size of disputes ever created. Descriptions are bounded (1024 bytes) to keep individual records small.

## Related Documentation

- [Datakey Fingerprint](datakey-fingerprint.md) - Storage key stability and migration considerations
- [Storage TTL](storage-ttl.md) - Time-to-live configuration for storage entries
