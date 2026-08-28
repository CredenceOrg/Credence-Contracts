# Lease Signatures for Relayed Delegation Payloads

This guide is written for downstream integrators building front-ends, relayers,
and indexers around the Credence delegation flow. It documents the signature
format and verification model that the contract uses for relayed, time-bounded
operations.

---

## What is being signed?

The contract does not verify a naked signature blob. Instead, the relayer
submits a `DelegatedActionPayload` and the owner signs the payload hash.
That payload is the canonical authorization object for a relayed lease-style
operation.

A payload carries the full context needed to make the signature non-replayable:

- `domain` — which entrypoint the signature is intended for
- `owner` — the identity whose authority is being invoked
- `target` — the delegate or subject address the action targets
- `contract_id` — the deployment / chain context for the delegation contract
- `nonce` — per-owner monotonic replay protection
- `scheme` — the signature scheme tag
- `ledger_number` — the Stellar ledger sequence at signing time
- `signature_domain` — contract-specific binding string

A minimal Rust-shaped example looks like this:

```rust
use credence_delegation::domain::{DelegatedActionPayload, DomainTag, SIGNATURE_DOMAIN};
use credence_delegation::verifier::SchemeTag;
use soroban_sdk::{Address, Env, String};

fn build_payload(
    e: &Env,
    owner: &Address,
    delegate: &Address,
    contract_id: &Address,
    nonce: u64,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: SchemeTag::Ed25519.to_u32(),
        ledger_number: e.ledger().sequence(),
        signature_domain: String::from_str(e, SIGNATURE_DOMAIN),
    }
}
```

The payload should be hashed and signed by the owner before the relayer submits
it to the contract.

---

## Verification model

The contract validates a relayed payload in a strict order:

1. `verify_payload(...)` — bind the payload to the expected domain, owner,
   target, and contract address.
2. `check_payload_age(...)` — reject stale payloads whose `ledger_number` is too
   old or in the future.
3. `verify_delegated_signature(...)` — dispatch according to the `scheme` tag.
4. `consume_nonce(...)` — burn the nonce only after the payload is accepted.

This order matters. A stale or mismatched payload should fail before a nonce is
consumed, and a replayed payload should not silently look like a fresh request.

---

## Scheme tags and what they mean

The relay payload uses a wire-stable `scheme` tag:

| Scheme tag | Meaning | Verification path |
| :--- | :--- | :--- |
| `0` | `Ed25519` | Verified implicitly by Soroban auth at the call site via `owner.require_auth()` |
| `1` | `Secp256r1` | Dispatched to a registered verifier contract |
| `2` | `MLDSA44` | Dispatched to a registered verifier contract |

The numeric values are part of the wire format and must not be renumbered after
deployment. New schemes must be appended at the end only.

### Ed25519

Legacy delegated requests are implicitly treated as Ed25519. The dispatcher does
not perform a second cryptographic check for this path; it trusts the Soroban
auth engine that already validated the owner signature at the call site.

### Secp256r1 and MLDSA44

For these schemes, the contract looks up a registered verifier contract for the
requested scheme and invokes it with:

```rust
fn verify(owner: Address, message: Bytes, signature: Bytes) -> bool
```

The verifier must return `true` for a valid signature and must reject invalid
or malformed input. If the verifier returns `false`, panics internally, or the
scheme has no registered verifier, the contract fails with a stable error code.

---

## Failure modes that integrators should expect

| Error | Trigger |
| :--- | :--- |
| `DomainMismatch` | payload domain does not match the entrypoint |
| `OwnerMismatch` | payload owner does not equal the caller owner |
| `TargetMismatch` | payload target does not match the delegate or subject supplied at the call site |
| `ContractIdMismatch` | payload contract context does not match the current contract |
| `PayloadTooOld` | payload is older than the allowed ledger window |
| `UnknownScheme` | scheme tag is not one of `0`, `1`, or `2` |
| `VerifierNotRegistered` | scheme `1` or `2` has no registered verifier |
| `VerificationFailed` | verifier returned `false` or rejected the signature |

These error codes are wire-stable. Integrators should treat them as part of the
public contract interface and not depend on human-readable strings.

---

## Operational guidance for relayers

- Populate `ledger_number` with the current Stellar ledger sequence at signing
  time, not with wall-clock time.
- Use a fresh nonce from `get_nonce(owner)` for every new relayed request.
- Re-sign promptly if the payload is stale; a payload that is too old will fail
  before the nonce is consumed.
- Keep the `signature_domain` and `contract_id` fields consistent with the
  deployment you are targeting.

A relayer that follows this model can safely submit a signed payload without
relying on hidden assumptions about the underlying verifier path.

---

## Related docs

- [DELEGATION_HANDBOOK.md](DELEGATION_HANDBOOK.md) — end-to-end relayed-action flow and nonce model
- [LEASE_MODEL.md](LEASE_MODEL.md) — lease scope, expiry, and renewal semantics
- [LEASE_SCOPES.md](LEASE_SCOPES.md) — scope bits, permissions, and guard functions
- [credence_delegation_api.md](credence_delegation_api.md) — contract entrypoints and types
