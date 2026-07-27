# Off-Chain Promise System

**Audience:** Contributors  
**Last updated:** 2026-07-25

A user signs a `DelegatedActionPayload` off-chain, a relayer submits it on-chain,
and the contract verifies the payload before executing the action. This pattern
enables gasless operations (the relayer pays the fee) and offline signing flows
(mobile wallets, air-gapped signers).

---

## 1. The Payload

Every off-chain promise is a `DelegatedActionPayload` (`contracts/credence_delegation/src/domain.rs:89-108`):

```rust
pub struct DelegatedActionPayload {
    pub domain: DomainTag,
    pub owner: Address,
    pub target: Address,
    pub contract_id: Address,
    pub nonce: u64,
    pub scheme: u32,
    pub signature_domain: String,
}
```

| Field | Meaning |
|---|---|
| `domain` | Which action this payload authorises — `Delegate`, `RevokeDelegation`, or `RevokeAttestation`. |
| `owner` | The identity whose authority is being invoked. |
| `target` | The address the action targets (delegate or attestation subject). |
| `contract_id` | The delegation contract address — prevents cross-contract replay. |
| `nonce` | Monotonically increasing per-owner counter — prevents replay. |
| `scheme` | Signature scheme tag (0 = Ed25519, 1 = Secp256r1, 2 = MLDSA44). |
| `signature_domain` | String `"CredenceDelegation"` — additional domain binding. |

---

## 2. Construction (Off-Chain)

The owner builds this payload outside the contract, then signs its serialised
form with their private key.

### Step-by-step

1. **Read the current nonce** by querying the contract:
   ```
   GET /contract/{id}/get_nonce?identity={owner}
   ```  
   Returns `0` for first-time callers.

2. **Assemble the struct** with the correct `domain` tag for the intended action.
   The `contract_id` must match the deployment address. The `signature_domain`
   must be `"CredenceDelegation"`.

3. **Hash and sign** the payload with the owner's private key using the scheme
   indicated by `scheme` (default Ed25519).

4. **Send the signed payload** to a relayer (off-chain HTTP, direct submission,
   etc.). The relayer will call the appropriate `execute_delegated_*` entrypoint.

### Example (Rust test helper)

From `contracts/credence_delegation/src/test_domain_separation.rs:33-50`:

```rust
fn make_payload(
    e: &Env,
    domain: DomainTag,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
        signature_domain: String::from_str(e, "CredenceDelegation"),
    }
}
```

---

## 3. Submission (By Relayer)

Three entrypoints accept relayed payloads:

| Entrypoint | DomainTag | Action |
|---|---|---|
| `execute_delegated_delegate` | `Delegate` | Create or replace a delegation |
| `execute_delegated_revoke` | `RevokeDelegation` | Revoke a management delegation |
| `execute_delegated_revoke_attest` | `RevokeAttestation` | Revoke an attestation delegation |

Each takes the same core parameters: `(owner, target, delegation_type, payload)`,
with `execute_delegated_delegate` also requiring `expires_at`.

### Example call

```rust
client.execute_delegated_delegate(
    &owner,
    &delegate,
    &DelegationType::Attestation,
    &expiry,
    &payload,              // ← the DelegatedActionPayload built off-chain
);
```

---

## 4. Verification (On-Chain)

When a relayer submits a payload, the contract performs these checks **in
order**. Failure at any step panics without modifying state.

### 4.1 Pause guard

```
require_not_paused()
```
Panics with `ContractError::ContractPaused` if the contract is paused.

### 4.2 Owner authentication

```
owner.require_auth()
```
Soroban's built-in auth engine verifies the transaction signature (Ed25519).
For non-Ed25519 schemes this alone does not prove ownership — see step 4.4.

### 4.3 Domain-separated payload verification

`domain::verify_payload()` (`contracts/credence_delegation/src/domain.rs:122-146`)
asserts all fields match:

| Check | Error code |
|---|---|
| `payload.domain == expected_domain` | `DomainMismatch` (504) |
| `payload.owner == caller_owner` | `OwnerMismatch` (505) |
| `payload.target == caller_target` | `TargetMismatch` (506) |
| `payload.contract_id == current_contract_address` | `ContractIdMismatch` (507) |
| `payload.signature_domain == "CredenceDelegation"` | `DomainMismatch` (504) |

A signature created for `execute_delegated_delegate` cannot be replayed against
`execute_delegated_revoke` because the domain tags differ.

### 4.4 Signature scheme dispatch

```rust
let scheme = domain::decode_scheme_safe(&payload);
verifier::verify_delegated_signature(&e, &owner, &message, &signature, scheme.to_u32());
```

- **Ed25519 (0):** Already verified by `owner.require_auth()` — no extra work.
- **Secp256r1 / MLDSA44:** Dispatches to a registered verifier contract via
  cross-contract call. Panics with `VerifierNotRegistered` if none is registered.

### 4.5 Expiry validation (delegate only)

```
now < expires_at <= now + MAX_DELEGATION_DURATION
```

Panics with `ContractError::ExceedsMaxDelegationDuration` or
`ContractError::DelegationExpired`. This check runs **before** nonce
consumption so an invalid expiry does not burn a nonce.

### 4.6 Nonce consumption

```rust
nonce::consume_nonce(&e, &owner, payload.nonce);
```

`consume_nonce` (`contracts/credence_delegation/src/nonce.rs:110-123`) reads
the stored nonce for `owner`, panics with `InvalidNonce` unless
`payload.nonce == stored_nonce`, then increments the stored value by 1.

Because both the direct path (`delegate`, `revoke_delegation`) and the relayed
path (`execute_delegated_*`) share the same nonce namespace, a nonce consumed
by one path cannot be reused by the other.

### 4.7 State transition

Only after all checks pass does the contract write state:

```rust
Self::store_delegation(…)   // or mark_delegation_revoked(…)
```

---

## 5. Security Properties

### Replay prevention

- **Nonces:** Each payload carries a per-owner monotonic nonce. Once consumed it
  cannot be reused.
- **Domain tags:** A `Delegate`-tagged payload cannot authorise a revoke action.
- **Contract binding:** `contract_id` prevents replay across deployments.
- **Signature domain:** `signature_domain = "CredenceDelegation"` prevents
  cross-contract replay within the Credence workspace.

### Key-compromise recovery

If the owner's key is compromised or a batch of pre-signed payloads is leaked,
the owner can call `invalidate_nonce_range(identity, new_nonce)` to skip the
nonce forward. Every payload with `nonce < new_nonce` becomes permanently
unspendable. The maximum jump per call is `MAX_NONCE_INVALIDATION_SPAN = 10_000`
to bound gas cost.

See [nonce-replay-proof.md](../contracts/credence_delegation/docs/nonce-replay-proof.md)
for the formal proof.

### Validation ordering (revoke paths)

The delegated revoke entrypoints enforce:

1. Domain-separated payload verification
2. Nonce consumption
3. State transition (`mark_delegation_revoked`)

This ordering ensures a replayed revoke payload fails with `InvalidNonce`
rather than `AlreadyRevoked`, preserving observable error semantics. See
[delegation-failure-modes.md](delegation-failure-modes.md).

---

## 6. Error Codes

| Code | Name | When |
|---|---|---|
| 501 | `DelegationNotFound` | No delegation record exists for the key |
| 502 | `AlreadyRevoked` | Delegation was already revoked |
| 504 | `DomainMismatch` | Payload domain or signature_domain mismatch |
| 505 | `OwnerMismatch` | Payload owner does not match caller |
| 506 | `TargetMismatch` | Payload target does not match caller |
| 507 | `ContractIdMismatch` | Payload contract_id does not match deployment |
| 508 | `UnknownScheme` | Scheme tag not recognised |
| 509 | `VerifierNotRegistered` | No verifier for the scheme |
| 510 | `DelegationInactive` | Delegation revoked or expired |
| 511 | `VerificationFailed` | Signature verification failed |
| 512 | `InvalidNonce` | Nonce mismatch or replay |
| 513 | `DelegationNotExpired` | Cleanup attempted before expiry |

---

## 7. Related Documents

- [credence-delegation.md](credence-delegation.md) — contract overview and direct-auth API
- [delegation.md](delegation.md) — delegation types, expiry, pausing
- [delegation-failure-modes.md](delegation-failure-modes.md) — revoke validation ordering
- [signature-scheme-upgrade.md](signature-scheme-upgrade.md) — multi-scheme verifier registry
- [nonce-replay-proof.md](../contracts/credence_delegation/docs/nonce-replay-proof.md) — formal nonce invalidation proof
- [nonce.rs](../contracts/credence_delegation/src/nonce.rs) — nonce implementation
- [domain.rs](../contracts/credence_delegation/src/domain.rs) — payload structure and verification
