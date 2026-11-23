# Bond Authorization Matrix

## Overview

This document catalogs every state-changing entrypoint in the `CredenceBond`
contract and the authorization (`require_auth`) check that gates it.  The
matrix serves as the canonical reference for auditors, integrators, and
developers who need to understand **who** is authorized to call **what**.

## Entrypoint → Authority Matrix

| Entrypoint | Scope | Required Auth | Mechanism |
|---|---|---|---|
| `create_bond(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `withdraw(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `withdraw_early(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `withdraw_bond(identity)` | Owner-scoped | `identity` | `identity.require_auth()` + `bond.identity == identity` check |
| `top_up(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `extend_duration(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `request_withdrawal(identity)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `renew_if_rolling(identity)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `request_cooldown_withdrawal(identity, …)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `execute_cooldown_withdrawal(identity)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `cancel_cooldown(identity)` | Owner-scoped | `identity` | `identity.require_auth()` |
| `slash(admin, identity, …)` | Admin-only | `admin` | delegated to `slashing::slash_bond` → `guards::require_admin` → `credence_errors::require_admin!` |
| `slash_bond(admin, identity, …)` | Admin-only | `admin` | `guards::require_admin` → `credence_errors::require_admin!` |
| `initialize(admin, …)` | Admin-only | `admin` | `admin.require_auth()` |
| `set_early_exit_config(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `set_accepted_tokens(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + stored admin check |
| `set_borrow_frozen(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + stored admin check |
| `set_token(admin, …)` | Admin-only | `admin` | delegated to `token_integration::set_token` |
| `register_attester(attester)` | Admin-only | stored admin | loads stored admin → `admin.require_auth()` |
| `unregister_attester(attester)` | Admin-only | stored admin | loads stored admin → `admin.require_auth()` |
| `set_attester_stake(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `set_weight_config(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `transfer_admin(current_admin, new_admin)` | Admin-only | both parties | `current_admin.require_auth()` + `new_admin.require_auth()` |
| `collect_fees(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `set_fee_config(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `set_liquidation_treasury(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `set_slash_treasury(admin, …)` | Admin-only | `admin` | `admin.require_auth()` + `guards::require_admin` |
| `add_attestation(attester, …)` | Attester-scoped | `attester` | `attester.require_auth()` + registration check |
| `revoke_attestation(attester, …)` | Attester-scoped | `attester` | `attester.require_auth()` + original-attester check |
| Governance parameter setters | Admin-only | `admin` | each delegates to `parameters::set_*` which enforces admin check |

## Owner-Scoped Authorization

For all owner-scoped entrypoints, the authentication check is:

```rust
identity.require_auth();
```

This is the first check after the pause guard (`Self::require_not_paused(&e)`)
and before any storage reads or mutations.  The Soroban host enforces that
the transaction's signature envelope includes a valid signature covering the
`identity` address (or a delegated auth entry covering it).

**Rationale**: Without this check, any address could mutate or drain another
identity's bond by calling the entrypoint directly.  The `identity` field
stored in the `IdentityBond` struct is an assertion about who created the
bond, not an enforcement mechanism — enforcement comes from `require_auth()`.

## Admin-Only Authorization

Admin-only entrypoints use one of two patterns:

### Pattern 1: Inline admin check (standalone `require_auth` + stored admin comparison)

```rust
admin.require_auth();
let stored_admin: Address = e.storage().instance()
    .get(&DataKey::Admin)
    .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
if admin != stored_admin {
    panic_with_error!(e, ContractError::NotAdmin);
}
```

### Pattern 2: `guards::require_admin` macro-based check

```rust
admin.require_auth();
guards::require_admin(&e, &admin);
```

The `guards::require_admin` function delegates to `credence_errors::require_admin!`,
which performs **both** the stored-admin comparison **and** calls
`admin.require_auth()`.  The macro is defined in
[`credence_errors/src/macros.rs`](../contracts/credence_errors/src/macros.rs).

### Pattern 3: Delegated admin check

Some entrypoints (e.g., parameter setters) delegate to a module function
that internally enforces the admin check.  These always include
`admin.require_auth()` somewhere in the call chain.

## Attester-Scoped Authorization

Attestation entrypoints (`add_attestation`, `revoke_attestation`) require the
`attester` address to authorize, and additionally verify that the attester is
registered in contract storage.

## Security Invariants

1. **Every state-changing entrypoint MUST include a `require_auth()` call.**
   There are no anonymous / permissionless state mutations in this contract.
   (Read-only entrypoints like `describe_config`, `describe_bond`,
   `get_identity_state`, `is_attester`, etc. are exempt from auth.)

2. **Owner-scoped functions authenticate against `identity`, not `caller`.**
   The `identity` passed as the first argument must match the authorized
   signer; there is no separate `caller` parameter.

3. **Admin functions authenticate against `admin`, then verify against
   `DataKey::Admin` storage.**  Two-step verification prevents a scenario
   where auth passes but the stored admin does not match.

4. **No entrypoint uses a bare `require_auth()` on the first argument without
   also checking it against stored state.**  The auth tree validates that a
   signature exists; the storage check validates that the signer has the
   correct role.

## Test Coverage

Authentication gating is tested in [`contracts/credence_bond/src/test_auth.rs`](../contracts/credence_bond/src/test_auth.rs)
with both happy-path (correct signer succeeds) and sad-path (wrong signer
panics) tests for every owner-scoped and admin-scoped entrypoint.

Tests verify:
- Owner functions succeed when `identity` authorizes
- Owner functions panic when a stranger's address is passed as `identity`
  (auth tree mismatch triggers a host panic — `mock_all_auths` in the test
  harness simulates all auth passing, so the test author must construct the
  scenario where the wrong identity is provided and `require_auth` fires
  against an unexpected identity)
- Admin functions succeed when the stored admin authorizes
- Admin functions panic when a non-admin calls

## References

- [`docs/access-control.md`](./access-control.md) — General access control system design
- [`contracts/credence_errors/src/macros.rs`](../contracts/credence_errors/src/macros.rs) — `require_admin!` macro definition
- [`contracts/credence_bond/src/guards.rs`](../contracts/credence_bond/src/guards.rs) — Guard helpers
- [`contracts/credence_bond/src/test_auth.rs`](../contracts/credence_bond/src/test_auth.rs) — Auth boundary tests
