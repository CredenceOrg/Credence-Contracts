# Nonce Model: Domain-Bound Replay Prevention for Signed Actions

## Overview

The Credence Bond contract uses a **monotonic, per-identity nonce** combined with
**deadline enforcement** and **domain (contract address) binding** to prevent
replay attacks on signed actions (`add_attestation`, `revoke_attestation`,
`add_attestation_batch`).

Every signed action carries three replay-protection parameters:

| Parameter      | Type      | Purpose |
|----------------|-----------|---------|
| `contract_id`  | `Address` | Binds the signature to a specific contract address (cross-contract replay prevention) |
| `deadline`     | `u64`     | Ledger timestamp after which the signature expires (replay-after-expiry prevention) |
| `nonce`        | `u64`     | Per-identity monotonic counter (duplicate-submission prevention) |

## Atomic Validation Order

All three checks are performed atomically in `nonce::validate_and_consume` in
the following order:

1. **Deadline check** — fail fast if `now > deadline + grace_window`. This
   happens **before** any storage write, so an expired signature cannot
   consume a nonce.

2. **Domain match** — panic with `DomainMismatch` if `expected_contract !=
   current_contract_address()`. Prevents a signature intended for one contract
   from being replayed against another.

3. **Nonce consumption** — atomically verifies and increments the per-identity
   nonce. Prevents the same signature from being submitted more than once.

> **Fail-safe property**: If either the deadline or domain check fails, the
> nonce is **not consumed**. The caller's next attempt with a corrected
> deadline/contract will use the same nonce value.

## Nonce Storage

- **Key**: `DataKey::Nonce(identity: Address)` → `u64`
- **Default**: `0` (every identity starts at nonce 0)
- **Monotonic**: Always increases by 1 on each consume; never decremented
- **TTL**: Extended by `MIN_NONCE_TTL` (518 400 ledgers ≈ 30 days) on each consume

## Grace Window

The optional `GraceWindow` parameter (stored at `DataKey::GraceWindow`) widens
the deadline acceptance by `grace` seconds. Default is `0` (strict enforcement).

```text
Accepted if: now <= deadline + grace_window
```

- A non-zero grace window **widens the replay surface** by exactly `grace`
  seconds.
- Setting a non-zero value is a **security-relevant decision** and should only
  be done when inclusion-delay problems require relaxing deadlines.

## SIGNATURE_DOMAIN (Defense-in-Depth)

Each contract in the Credence system defines a `SIGNATURE_DOMAIN` constant:

```rust
pub(crate) const SIGNATURE_DOMAIN: &str = "CredenceBond";
```

This string constant is embedded in the WASM binary and can never change at
runtime. The function `validate_and_consume_with_domain_string` additionally
verifies this constant at runtime as a **defense-in-depth** layer on top of
the contract address check.

The uniqueness of `SIGNATURE_DOMAIN` across all Credence contracts is enforced
by the workspace-level integration test at `tests/signature_domains_unique.rs`.

## Entrypoints Using Domain-Bound Nonces

| Entrypoint | Parameters (replay-related) | Nonce function used |
|---|---|---|
| `add_attestation` | `contract_id`, `deadline`, `nonce` | `validate_and_consume` |
| `revoke_attestation` | `contract_id`, `deadline`, `nonce` | `validate_and_consume` |
| `add_attestation_batch` | Each `AttestationBatchItem` carries `contract_id`, `deadline`, `nonce` | `validate_and_consume` per item |

## Threat Model Coverage

| Threat | Mitigation |
|---|---|
| **Cross-contract replay**: signature for contract A submitted to contract B | `contract_id` domain binding + `SIGNATURE_DOMAIN` constant |
| **Cross-action replay**: signature for `add_attestation` used on `revoke_attestation` | Per-identity monotonic nonce (one signature per nonce) |
| **Replay-after-expiry**: stale signature submitted long after deadline | Deadline check with configurable grace window |
| **Duplicated submission**: same signature submitted twice | Nonce consumed on first use, second attempt rejected |
| **Nonce exhaustion / overflow** | `checked_add` with explicit panic on overflow |

## Example (Add Attestation)

```rust
// Off-chain: construct the signed payload
let payload = (nonce, contract_id, deadline, attester, subject, data);

// On-chain validation
nonce::validate_and_consume(&e, &attester, &contract_id, deadline, nonce);
// └─ deadline check → domain check → nonce consume
```

## API for External Callers

### Production API (available in release WASM)

- `nonce::get_nonce(e, identity)` → `u64`
- `nonce::consume_nonce(e, identity, expected_nonce)` — raw nonce check (use
  `validate_and_consume` for domain-bound operations)
- `nonce::require_not_expired(e, deadline)` — standalone deadline check
- `nonce::require_domain_match(e, expected_contract)` — standalone domain check
- **`nonce::validate_and_consume(e, identity, expected_contract, deadline, nonce)`** —
  **primary entrypoint**: deadline → domain → nonce
- `nonce::validate_and_consume_with_grace(e, identity, expected_contract, deadline, nonce, grace)` —
  deadline with explicit grace override
- `nonce::validate_and_consume_with_domain_string(e, identity, expected_contract, deadline, nonce)` —
  defense-in-depth: adds `SIGNATURE_DOMAIN` string check
- `nonce::get_grace_window(e)` → `u64`
- `nonce::set_grace_window(e, grace)` → `u64` (previous value)

### Test / Testutils API

The following helpers are available under `#[cfg(any(test, feature = "testutils"))]`:

- `nonce::set_nonce(e, identity, nonce)` — directly set a nonce for testing

