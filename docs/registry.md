# Credence Registry

The registry contract maps identity addresses to their bond contract addresses, enabling efficient forward and reverse lookups across the Credence trust protocol.

## Uniqueness Invariants

The registry enforces strict uniqueness at two levels:

| Invariant | Error code | Description |
|---|---|---|
| Identity uniqueness | `#400 IdentityAlreadyRegistered` | Each identity address may appear at most once in the forward mapping |
| Bond contract uniqueness | `#401 BondContractAlreadyRegistered` | Each bond contract address may appear at most once in the reverse mapping |

Both checks are enforced atomically inside `register`. A deactivated entry still occupies its storage slot and therefore still blocks re-registration — use `remove` to free the slot.

## Operations

### `register(identity, bond_contract, allow_non_interface) → RegistryEntry`

Creates a new identity → bond mapping. Requires admin auth.

- Panics `#400` if `identity` already has an entry (active **or** deactivated).
- Panics `#401` if `bond_contract` is already mapped to any identity.
- Panics if `allow_non_interface = false` and the bond contract does not implement `supports_interface(IFACE_CREDENCE_BOND_V1)`.

### `deactivate(identity)`

Soft-deletes the entry by setting `active = false`. The storage keys remain; the identity and bond contract are **not** freed for re-use. Requires admin auth.

- Panics `#402` if identity is not registered.
- Panics `#404` if already deactivated.

### `reactivate(identity)`

Restores a soft-deleted entry to `active = true`. Requires admin auth.

- Panics `#402` if identity is not registered.
- Panics `#405` if already active.

### `remove(identity)`

Hard-deletes the entry. Removes the forward mapping (`identity → bond`), the reverse mapping (`bond → identity`), and the `AllowNonInterface(bond)` flag from storage, and removes the identity from the `RegisteredIdentities` list. After removal both the identity and the bond contract are free to be re-registered. Requires admin auth.

- Panics `#402` if identity is not registered.
- Works on both active and deactivated entries.

### `get_bond_contract(identity) → RegistryEntry`

Forward lookup. Panics `#402` if not registered.

### `get_identity(bond_contract) → Address`

Reverse lookup. Panics `#403` if not registered.

### `is_registered(identity) → bool`

Returns `true` only if the entry exists **and** `active = true`.

## Remove / Reinsert Semantics

```
register(id, bond_A)   → ok
deactivate(id)         → soft-delete; id still blocks re-registration
register(id, bond_B)   → ERROR #400  (entry still in storage)

remove(id)             → hard-delete; both id and bond_A are freed
register(id, bond_B)   → ok  (fresh entry, new timestamp)
register(id2, bond_A)  → ok  (bond_A is also free again)
```

## Error Code Reference

| Code | Name | Trigger |
|---|---|---|
| `#400` | `IdentityAlreadyRegistered` | `register` called with an identity that already has a storage entry |
| `#401` | `BondContractAlreadyRegistered` | `register` called with a bond contract already mapped to another identity |
| `#402` | `IdentityNotRegistered` | `get_bond_contract`, `deactivate`, `reactivate`, or `remove` called for an unknown identity |
| `#403` | `BondContractNotRegistered` | `get_identity` called for an unknown bond contract |
| `#404` | `AlreadyDeactivated` | `deactivate` called on an already-inactive entry |
| `#405` | `AlreadyActive` | `reactivate` called on an already-active entry |

Part of the Credence protocol contracts.

## Known Simplifications

- `get_all_identities()` is unbounded and has no pagination.
- There is no cross-contract binding between bond and registry at initialization.

See [known-simplifications.md](known-simplifications.md#7-get_all_identities-has-no-pagination) for details and production paths.
