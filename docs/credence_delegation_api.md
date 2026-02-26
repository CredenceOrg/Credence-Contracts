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
| `expires_at` | `u64` | Ledger timestamp when the permission automatically expires. |
| `revoked` | `bool` | Manual override flag to cancel permission before expiry. |

### `DelegationType` (Enum)
* **`Attestation`**: Permission to vouch for identity claims.
* **`Management`**: Permission to perform administrative actions on behalf of the owner.

### `AttestationStatus` (Enum)
Used for checking the health of an attestation:
* **`Active`**: Found and valid.
* **`Revoked`**: Found but manually cancelled.
* **`NotFound`**: No record exists.

---

## Initialization

### `initialize(e: Env, admin: Address)`
Sets the contract administrator. 
* **Guard**: Panics with `"already initialized"` if called more than once.

---

## Delegation Workflows

### `delegate(...)`
Creates a new delegation record.
* **Parameters**: `owner`, `delegate`, `delegation_type`, `expires_at`.
* **Authorization**: `owner.require_auth()`.
* **Validation**: `expires_at` must be a future timestamp.
* **Logic**: Overwrites any existing delegation of the same type.

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

## Validation & View Functions



### `is_valid_delegate(...)`
The primary check for other contracts to use.
* **Logic**: Returns `true` only if the record exists, `revoked` is false, and the current ledger timestamp is less than `expires_at`.

### `get_attestation_status(...)`
A high-level check for the state of a specific attestation.
* **Returns**: `Active`, `Revoked`, or `NotFound`.

### `get_delegation(...)`
**Signature**: `pub fn get_delegation(e: Env, owner: Address, delegate: Address, delegation_type: DelegationType) -> Delegation`
Returns the raw `Delegation` struct. Panics if no record is found.

---

## Security & Error Reference

| Panic Message | Cause |
| :--- | :--- |
| `already initialized` | Attempted to re-run the `initialize` function. |
| `expiry must be in the future` | The `expires_at` provided is $\le$ current ledger timestamp. |
| `delegation not found` | Attempted to get or revoke a non-existent record. |
| `already revoked` | Attempted to revoke a delegation that is already in a revoked state. |