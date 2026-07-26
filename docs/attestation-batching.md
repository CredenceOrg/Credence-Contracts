# Batch Attestation System

The batch attestation entrypoint allows multiple credibility attestations to be submitted atomically.

## Entrypoint

```rust
pub fn add_attestation_batch(
    e: Env,
    subject: Address,
    items: Vec<AttestationBatchItem>,
) -> Vec<Attestation>
```

### AttestationBatchItem

```rust
#[contracttype]
pub struct AttestationBatchItem {
    pub attester: Address,
    pub attestation_data: String,
    pub nonce: u64,
}
```

## Constraints and Security

- **Bounded Batch Size:** The size of the batch `items` is strictly capped ($N \le 64$).
- **Atomicity:** All validations must pass for every item in the batch. If any item is rejected (e.g. invalid signature, duplicate, or unregistered attester), the entire batch reverts.
- **Duplicate Prevention:** 
  - Unique attesters in the batch: The same attester cannot appear multiple times in a single batch.
  - Dedup keys: Prevents duplicate attestations (same attester, subject, and data) in storage.
- **Weight Caps:**
  - Unit Weight Cap: The weight for each individual attestation is capped by the configured maximum weight.
  - Aggregate Weight Cap: The sum of weights of all attestations in the batch must not exceed the configured maximum weight.
- **Storage Optimization:** Reads and writes `SubjectAttestations` only once per batch.
