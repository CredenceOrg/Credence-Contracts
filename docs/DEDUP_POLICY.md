# Dedup Policy

When we dedupe input, when we reject on duplicate, and why.

**Audience:** contributor.

Every mutating entrypoint in the Credence contracts must decide what constitutes a
duplicate and what to do about it. This document catalogues the patterns so
reviewers can check new code against established intent and contributors can
reason about cross-contract consistency without reading every commit.

---

## Table of Contents

1. [The Three Strategies](#the-three-strategies)
2. [1. Sequential Nonce (consume-once)](#1-sequential-nonce-consume-once)
3. [2. Dedup Key (exists-check)](#2-dedup-key-exists-check)
4. [3. Operation Hash (executed-once)](#3-operation-hash-executed-once)
5. [4. Deterministic Proposal ID (convergent dedup)](#4-deterministic-proposal-id-convergent-dedup)
6. [5. Set-Membership Guard (signer/identity dedup)](#5-set-membership-guard-signeridentity-dedup)
7. [Choosing a Strategy (Decision Table)](#choosing-a-strategy-decision-table)
8. [Error Codes](#error-codes)
9. [Related Documents](#related-documents)

---

## The Three Strategies

| Strategy | Reject on | Storage | Use when |
|---|---|---|---|
| Sequential nonce | Nonce != stored | Instance / Persistent | Every call is distinct; ordering matters |
| Dedup key | Key already exists | Instance / Persistent | Calls may be identical; retry is expected |
| Operation hash | Hash already executed | Instance | One-time execution guarantee |

A fourth strategy, **deterministic proposal ID**, is a hybrid that uses hash
derivation to converge concurrent submissions onto a single record — it is
described in [§4](#4-deterministic-proposal-id-convergent-dedup).

**Rule of thumb:** if a caller might legitimately retry the same operation
(e.g. webhook timeout), use a dedup key or operation hash. If every invocation is
semantically distinct, use a sequential nonce.

---

## 1. Sequential Nonce (consume-once)

**Contracts:** `credence_bond`, `credence_delegation`

Every identity (address) owns a single `u64` nonce that starts at 0. Each
state-changing call must present the current nonce; on success the nonce is
incremented by 1.

### Bond contract

`contracts/credence_bond/src/nonce.rs`:

```rust
// Read current nonce (defaults to 0 if never set).
pub fn get_nonce(e: &Env, identity: &Address) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::Nonce(identity.clone()))
        .unwrap_or(0)
}

// Assert match, then increment.
pub fn consume_nonce(e: &Env, identity: &Address, expected_nonce: u64) {
    let current = get_nonce(e, identity);
    if current != expected_nonce {
        panic_with_error!(e, ContractError::InvalidNonce);
    }
    let next = current.checked_add(1).expect("nonce overflow");
    e.storage()
        .instance()
        .set(&DataKey::Nonce(identity.clone()), &next);
    bump_nonce_ttl(e, &DataKey::Nonce(identity.clone()), 0);
}
```

Entrypoints that consume a nonce:
- `add_attestation(attester, subject, attestation_data, nonce, …)`
- `revoke_attestation(attester, attestation_id, nonce, …)`
- `add_attestation_batch` (each item carries its own nonce)

### Delegation contract

`contracts/credence_delegation/src/nonce.rs` uses the same pattern but stores
nonces in **persistent** storage (not instance) with TTL bumping. It also
exposes `invalidate_nonce_range` for emergency key recovery:

```rust
// Skip an entire range — e.g. after a key compromise.
pub fn invalidate_nonce_range(
    e: &Env,
    identity: &Address,
    new_nonce: u64,
    max_span: u64,    // capped at MAX_NONCE_INVALIDATION_SPAN = 10_000
) -> (u64, u64) { … }
```

Entrypoints that consume a delegation nonce:
- `delegate(…)` / `execute_delegated_delegate(…)`
- `revoke_delegation(…)` / `execute_delegated_revoke(…)`
- `revoke_attestation(…)` / `execute_delegated_revoke_attest(…)`

### Validation Order (critical)

The validation order inside `consume_nonce`-style entrypoints is pinned:

```
1. Deadline check (fail → SignatureExpired, nonce NOT consumed)
2. Domain check   (fail → DomainMismatch,   nonce NOT consumed)
3. Nonce check    (fail → InvalidNonce,     nonce NOT consumed)
4. Consume nonce  (increment stored nonce)
5. Perform action (attest, delegate, revoke, …)
```

**Why it matters:** if a stale or expired payload were to burn the nonce before
the expiry check, the caller would lose a nonce value and be forced to skip it.
Checking the deadline and domain *before* consumption keeps the nonce stream
continuous even when individual payloads are rejected.

See also [delegation-failure-modes.md](delegation-failure-modes.md) —
"Revoke replay security semantics".

---

## 2. Dedup Key (exists-check)

**Contracts:** `credence_bond` (attestations), `credence_bond` (idempotency keys)

When an operation can legitimately arrive more than once — e.g. a webhook retry
after a timeout — a dedup key is stored on first use and checked on every
subsequent attempt.

### Attestation dedup

`contracts/credence_bond/src/types/attestation.rs` defines `AttestationDedupKey`:

```rust
/// Key used to detect duplicate attestations:
/// same verifier, identity, and data.
#[contracttype]
pub struct AttestationDedupKey {
    pub verifier: Address,
    pub identity: Address,
    pub attestation_data: String,
}
```

In `credence_bond::lib::add_attestation`:

```rust
let dedup_key = types::AttestationDedupKey {
    verifier: attester.clone(),
    identity: subject.clone(),
    attestation_data: attestation_data.clone(),
};
if e.storage().instance().has(&dedup_key) {
    panic_with_error!(e, ContractError::DuplicateAttestation);
}
// … later, after all other checks pass:
e.storage().instance().set(&dedup_key, &true);
```

**Distinction from nonce:** the nonce prevents replay of *any* signed payload;
the dedup key prevents re-submission of the *same logical attestation* even with
different nonces. Both layers are applied.

### Idempotency keys (admin operations)

`contracts/credence_bond/src/idempotency.rs`:

```rust
/// SHA-256(actor_address || operation_name || salt_bytes)
pub fn compute_key(e: &Env, actor: &Address, operation: &Symbol, salt: &Bytes) -> Bytes { … }

pub fn check_and_record(e: &Env, actor: &Address, operation: &Symbol, salt: &Bytes) {
    let key = compute_key(e, actor, operation, salt);
    let storage_key = DataKey::IdempotencyKey(key.clone());
    if e.storage().persistent().has(&storage_key) {
        panic_with_error!(e, ContractError::DuplicateIdempotencyKey);
    }
    e.storage().persistent().set(&storage_key, &true);
}
```

Used by externally-triggered admin operations (`slash_bond`, `collect_fees`,
emergency withdrawals). The caller supplies a unique `salt` per request; if the
same `(actor, operation, salt)` tuple is seen again, the call is rejected.

**Why a caller-supplied salt instead of a nonce?** Admin operations are rare and
may be triggered by different off-chain systems that do not share a nonce
counter. The salt lets each system generate its own idempotency token.

### Registry idempotency (stored-result variant)

`contracts/credence_registry/src/idempotency.rs` goes one step further: on
first use it stores the *result* and returns it on replay **if** the caller is
the same. If a different caller tries the same `tx_id`, it returns
`DuplicateDifferentCaller`.

---

## 3. Operation Hash (executed-once)

**Contracts:** `credence_multisig`, `timelock`

When an operation must execute *at most once* regardless of how many proposals
wrap it, the operation's deterministic hash is stored in an `ExecutedOp` set.

### Multisig

`contracts/credence_multisig/src/multisig.rs`:

```rust
// In execute_proposal:
let already_executed = e
    .storage()
    .instance()
    .get(&DataKey::ExecutedOp(op_hash.clone()))
    .unwrap_or(false);

if already_executed {
    panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
}

// Mark executed globally — prevents exact replay across proposals.
e.storage()
    .instance()
    .set(&DataKey::ExecutedOp(op_hash.clone()), &true);
```

The `op_hash` is a `BytesN<32>` provided by the proposer at submission time.
It cryptographically identifies the operation payload. Once any proposal with
that hash is executed, no other proposal (even a different `proposal_id`) can
execute the same operation.

### Timelock

`contracts/timelock/src/lib.rs` uses the same pattern:

```rust
// Replay guard: cannot queue an operation that was already executed.
if e.storage()
    .instance()
    .get(&DataKey::ExecutedOp(op_hash.clone()))
    .unwrap_or(false)
{
    panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
}
```

The check happens at **queue time** (not just execution), preventing an
already-executed operation from even entering the queue.

---

## 4. Deterministic Proposal ID (convergent dedup)

**Contract:** `credence_delegation` (pause system)

When multiple operators submit the *same* pause/unpause action concurrently,
the system should converge them onto a single proposal rather than creating
duplicate proposals that split the vote.

`contracts/credence_delegation/src/pausable.rs`:

```rust
/// Number of ledger sequences per epoch bucket.
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;

fn derive_proposal_id(e: &Env, action: PauseAction) -> u64 {
    let epoch = e.ledger().sequence() / PROPOSAL_EPOCH_SIZE;
    let mut preimage = [0u8; 8];
    // action_u32_big_endian || epoch_u32_big_endian
    preimage[0..4].copy_from_slice(&(action as u32).to_be_bytes());
    preimage[4..8].copy_from_slice(&epoch.to_be_bytes());
    let hash = e.crypto().sha256(&Bytes::from_slice(e, &preimage));
    u64::from_be_bytes(hash.to_array()[0..8].try_into().unwrap())
}
```

When `propose_action` is called:
1. Derive `proposal_id` from `(action, epoch)`.
2. If a proposal with that ID exists → **idempotent**: skip the write, keep the
   existing record.
3. If absent → write the proposal record.
4. Record the caller's approval on the proposal (always).

**Result:** any number of operators submitting "Pause" in the same epoch bucket
converge on one proposal.

**Limits:**
- Different epochs → different IDs (allows re-proposing a stale action).
- Different actions (Pause vs Unpause) → always different IDs.
- `PROPOSAL_EPOCH_SIZE = 100` ledgers ≈ 8 minutes (at 5 s/ledger).

Full rationale: [proposal-id-derivation.md](proposal-id-derivation.md).

---

## 5. Set-Membership Guard (signer/identity dedup)

Simple existence checks that reject a duplicate before it can be stored.

| Contract | Guard | Error |
|---|---|---|
| `credence_multisig` | Signer already in signer list at `initialize` | `AlreadyActive` |
| `credence_multisig` | `add_signer` for an address that is already a signer | `AlreadyActive` |
| `credence_multisig` | Signing a proposal you already signed | `AlreadyActive` |
| `credence_bond` | Creating a bond for an identity that already has one | `BondAlreadyExists` |
| `credence_bond` (batch) | Duplicate attester within a single batch | Raw `panic!("duplicate attester in batch")` (known simplification — single-path uses typed `ContractError`) |
| `credence_bond` (evidence) | Submitting evidence with a hash already stored | `panic!("evidence hash already exists")` |
| `credence_treasury` | Re-registering an existing depositor | No-op (idempotent) |
| `credence_treasury` | Re-registering an existing signer | Guarded: `SignerCount` invariant |
| `admin` | Adding an admin address that is already an admin | `AlreadyActive` |
| `credence_registry` | Registering the same bond → identity pair | Returns existing entry (idempotent) |
| `credence_registry` | Deactivated identity slot | Guards against duplicate entries |

---

## Choosing a Strategy (Decision Table)

| Scenario | Strategy | Example |
|---|---|---|
| Signed user actions; every call is distinct | Sequential nonce | `add_attestation`, `delegate` |
| Admin ops triggered by external webhooks | Idempotency key (salt) | `slash_bond`, `collect_fees` |
| Operations that must execute exactly once | Operation hash | Multisig `execute_proposal`, timelock `execute_operation` |
| Concurrent submissions of the same proposal | Deterministic proposal ID | Pause proposal |
| Preventing the *same data* from being attested twice | Dedup key (composite) | `AttestationDedupKey` |
| Preventing duplicate entries in a set | Existence check | Duplicate signer, duplicate bond |

---

## Error Codes

| Error | Code | Used by |
|---|---|---|
| `InvalidNonce` | 208 | Nonce replay or out-of-order (bond, delegation) |
| `BondAlreadyExists` | 217 | Bond creation for existing identity |
| `DuplicateIdempotencyKey` | 231 | Admin idempotency key replay |
| `DuplicateAttestation` | 300 | Same `(verifier, identity, data)` triple |
| `AlreadyActive` | 405 | Signer already exists, already signed, etc. |
| `ProposalAlreadyExecuted` | — | Operation hash already executed (multisig, timelock) |

All error codes are defined in `contracts/credence_errors/src/lib.rs`.

---

## Related Documents

- [security.md](security.md) — Replay attack prevention overview
- [delegation-failure-modes.md](delegation-failure-modes.md) — Nonce-ordering rationale for revoke
- [proposal-id-derivation.md](proposal-id-derivation.md) — Deterministic proposal ID scheme
- [attestation-batching.md](attestation-batching.md) — Batch dedup & atomicity
- [errors.md](errors.md) — Full error code catalogue
- [multisig.md](multisig.md) — Multisig replay prevention
- [timelock.md](timelock.md) — Timelock replay guard
