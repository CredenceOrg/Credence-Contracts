# CredenceDelegation API Reference

The **CredenceDelegation** contract manages the relationships between account owners and their authorized delegates. It is designed to handle temporary permissions for identity management and the tracking of attestation statuses within the ecosystem.



## 🏗 Data Structures

### `Delegation`
The core object representing a permission grant.
| Field | Type | Description |
| :--- | :--- | :--- |
| `owner` | `Address` | The account granting the permission. |
| `delegate` | `Address` | The account receiving the permission. |
| `delegation_type` | `DelegationType` | The scope of the grant (Attestation or Management). |
| `expires_at` | `u64` | Ledger timestamp when the permission automatically expires. Must be in the bounded lifetime window. |
| `revoked` | `bool` | Manual override flag to cancel permission before expiry. |
| `scheme` | `u32` | The signature scheme tag used to authorize the delegation (`0` = Ed25519, `1` = Secp256r1, `2` = ML-DSA44). Persisted at storage time and reported by `get_delegation_summary`. |

### `DelegationType` (Enum)
* **`Attestation``**: Permission to vouch for identity claims.
* **`$Management``**: Permission to perform administrative actions on behalf of the owner.

### `AttestationStatus` (Enum)
Used for checking the health of an attestation:
* **`Active`**: Found and valid.
* **`Revoked`**: Found but manually cancelled.
** **`NotFound`**: No record exists.

---

## Initialization

### `initialize(e: Env, admin: Address)`
Sets the contract administrator. 
* **Guard**: Panics with `"already initialized" if called more than once.

---

## Delegation Workflows

### `delegate(...)`
Creates a new delegation record.
* **Parameters**: `owner`, `delegate`, `delegation_type`, `expires_at`.
* **Authorization**: `owner.require_auth()`.
* **Validation**: `expires_at` must be greater than the current ledger timestamp and no later than `now + MAX_DELEGATION_DURATION` (`365 days` by default).
* **Logic**: Overwrites any existing delegation of the same type.

### `execute_delegated_delegate(...)`
Creates a delegation through a relayed, domain-separated payload.
* **Parameters**: `owner`, `delegate`, `delegation_type`, `expires_at`, `payload`, `signature`.
* **Authorization**: `owner.require_auth()` and a `DomainTag::Delegate` payload bound to the current contract address.
** **Validation***: Applies the same expiry bounds as `delegate(...)` before nonce consumption, so an invalid over-long expiry cannot burn the owner's nonce. The payload's `signature_domain` must equal `"CredenceDelegation"` and the scheme must be supported.

### `revoke_delegation(...)`
Cancels a generic delegation.
* **Parameters**: `owner`, `delegate`, `delegation_type`.
* **Authorization**: Only the `owner` can revoke.
* **Logic**: Sets `revoked` to `true`.

### `revoke_attestation(...)`
A specific helper function to revoke permissions specifically of the `Attestation` type.
* **Parameters**: `attester` (the owner), `subject` (the delegate).
* **Authorization**: `attester.require_auth()`.

---

## Relayed Payload Verification

Every relayed entry point (`execute_delegated_delegate`, `execute_delegated_revoke`, `execute_delegated_revoke_attest`) accepts a `DelegatedActionPayload` plus a raw `signature: Bytes` argument. Verification proceeds in this order:

1. **Domain separation** — `payload.domain` must match the expected `DomainTag` for the entry point.
2. **Signature domain** — `payload.signature_domain` must equal `SIGNATURE_DOMAIN = "CredenceDelegation"`. Any other value fails with `DomainMismatch` (504).
3. **Owner / target / contract binding** — the payload must be bound to the caller, the delegate and the current contract address.
4. **Scheme support** — `payload.scheme` must be one of the registered scheme tags. Unknown schemes (e.g. `99`) fail with `UnknownScheme`.
5. **Signature verification** — the payload message is verified against the supplied signature bytes using the configured verifier for the scheme. Ed25519 additionally relies on `owner.require_auth()`.

### Message pre-image

The signed message is the 32-byte SHA-256 digest of the XDR-encoded payload:

```
message = sha256(xdr(payload))
```

This is exposed to clients as `domain::payload_message(e, &payload) -> Bytes`. The `signature` argument on each relayed entry point is the raw signature over this digest for the selected scheme.

---

## Validation & View Functions



### `is_valid_delegate(...)`
The primary check for other contracts to use.
* **Logic**: Returns `true` only if the record exists, `revoked` is false, and the current ledger timestamp is strictly less than `expires_at`. At `timestamp == expires_at`, the delegation is expired and invalid.

### `get_attestation_status(...)`A high-level check for the state of a specific attestation.
* **Returns**: `Active`, `Revoked`, or `NotFound`.

### `get_delegation(...)`
**Signature**: `pub fn get_delegation(e: Env, owner: Address, delegate: Address, delegation_type: DelegationType) -> Delegation`
Returns the raw `Delegation` struct. Panics if no record is found.

### `get_delegation_summary(...)`
Returns a summary of the delegation including the persisted `scheme` that was used to authorize it. For a Secp256r1 delegation this reports `scheme == 1`.

---

## Security & Error Reference

| Panic Message | Cause |
| :--- | :--- |
| `already initialized` | Attempted to re-run the `initialize` function. |
| `expiry must be in the future` | The `expires_at` provided is $\le$ current ledger timestamp. |
| `delegation expiry exceeds maximum duration` | The `expires_at` provided is greater than `now + MAX_DELEGATION_DURATION`. |
| `delegation not found` | Attempted to get or revoke a non-existent record. |
| `already revoked` | Attempted to revoke a delegation that is already in a revoked state. |
| `DomainMismatch` (504) | Payload `domain` or `signature_domain` does not match the expected value. |
| `UnknownScheme` | Payload `scheme` tag is not supported by the contract. |
| `VerificationFailed` | The supplied signature did not verify against the payload message. |

## Expiry Security Notes

`MAX_DELEGATION_DURATION` is currently `365 days`. This rejects unbounded
delegations such as `u64::MAX` while preserving normal short-lived delegation
flows. Expired delegations are invalid even if they have not been manually
revoked, and an owner may still revoke an expired record to preserve explicit
audit state.
