# Access Control System

## Overview

The Credence access control module provides reusable, composable role-based access control modifiers for smart contracts. It implements three primary roles (Admin, Verifier, Identity Owner) with support for role composition and comprehensive security event logging.

## Architecture

### Core Roles

#### 1. Admin Role
- **Purpose**: Full administrative privileges over the contract
- **Capabilities**:
  - Initialize contract configuration
  - Add/remove verifiers
  - Execute slashing operations
  - Modify system parameters
- **Storage**: Single admin address stored at key `"admin"`
- **Modifier**: `require_admin(e: &Env, caller: &Address)`

#### 2. Verifier Role
- **Purpose**: Validate and verify identity claims
- **Capabilities**:
  - Verify identity attestations
  - Review and approve claims
  - Participate in validation workflows
- **Storage**: Per-verifier boolean flag with key prefix `"verifier"`
- **Modifier**: `require_verifier(e: &Env, caller: &Address)`
- **Management**: Admin can add/remove verifiers

#### 3. Identity Owner Role
- **Purpose**: Self-sovereign control over personal identity and bonds
- **Capabilities**:
  - Manage own bonds
  - Withdraw funds
  - Update personal identity data
- **Modifier**: `require_identity_owner(e: &Env, caller: &Address, expected: &Address)`
- **Validation**: Direct address comparison

### Role Composition

The system supports combining roles for flexible access patterns:

```rust
require_admin_or_verifier(e: &Env, caller: &Address)
```

This modifier allows either admin OR verifier to proceed, enabling shared responsibilities while maintaining clear audit trails.

## API Reference

## Bond Entrypoint Authority Matrix

The bond contract uses explicit authorization at each state-changing entrypoint.

| Entrypoint | Scope | Required auth |
|---|---|---|
| `create_bond(identity, ...)` | Owner-scoped | `identity.require_auth()` |
| `withdraw(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `withdraw_early(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `top_up(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `extend_duration(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `request_withdrawal(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `renew_if_rolling(...)` | Owner-scoped | Stored bond owner: `bond.identity.require_auth()` |
| `withdraw_bond(identity)` | Owner-scoped | `identity.require_auth()` and `bond.identity == identity` |
| `slash(...)` / `slash_bond(...)` | Admin-only | Stored admin check plus `admin.require_auth()` |
| `set_early_exit_config(...)` | Admin-only | Stored admin check plus `admin.require_auth()` |
| `register_attester(...)` / `unregister_attester(...)` | Admin-only | Stored admin `require_auth()` |
| `set_attester_stake(...)` / `set_weight_config(...)` | Admin-only | Stored admin check plus `admin.require_auth()` |
| `collect_fees(...)` | Admin-only | Stored admin check plus `admin.require_auth()` |

### Access Control Modifiers

#### `require_admin(e: &Env, caller: &Address)`
Enforces admin-only access.

**Panics**: 
- `"not admin"` - Caller is not the admin
- `"not initialized"` - Contract not initialized

**Events**: Emits `access_denied` on failure

**Example**:
```rust
pub fn set_config(e: Env, caller: Address, value: u32) {
    require_admin(&e, &caller);
    // Admin-only logic
}
```

#### `require_verifier(e: &Env, caller: &Address)`
Enforces verifier-only access.

**Panics**: `"not verifier"` - Caller is not a registered verifier

**Events**: Emits `access_denied` on failure

**Example**:
```rust
pub fn verify_claim(e: Env, verifier: Address, claim_id: u64) {
    require_verifier(&e, &verifier);
    // Verifier-only logic
}
```

#### `require_identity_owner(e: &Env, caller: &Address, expected: &Address)`
Enforces identity owner access.

**Panics**: `"not identity owner"` - Caller doesn't match expected identity

**Events**: Emits `access_denied` on failure

**Example**:
```rust
pub fn withdraw(e: Env, caller: Address, bond: &IdentityBond) {
    require_identity_owner(&e, &caller, &bond.identity);
    // Owner-only logic
}
```

#### `require_admin_or_verifier(e: &Env, caller: &Address)`
Enforces admin OR verifier access (role composition).

**Panics**: `"not authorized"` - Caller is neither admin nor verifier

**Events**: Emits `access_denied` on failure

**Example**:
```rust
pub fn review_submission(e: Env, reviewer: Address, submission_id: u64) {
    require_admin_or_verifier(&e, &reviewer);
    // Admin or verifier logic
}
```

### Role Management Functions

#### `add_verifier_role(e: &Env, admin: &Address, verifier: &Address)`
Add a new verifier (admin only).

**Events**: Emits `verifier_added` with verifier address

**Example**:
```rust
pub fn add_verifier(e: Env, admin: Address, verifier: Address) {
    add_verifier_role(&e, &admin, &verifier);
}
```

#### `remove_verifier_role(e: &Env, admin: &Address, verifier: &Address)`
Remove an existing verifier (admin only).

**Events**: Emits `verifier_removed` with verifier address

**Example**:
```rust
pub fn remove_verifier(e: Env, admin: Address, verifier: Address) {
    remove_verifier_role(&e, &admin, &verifier);
}
```

### Query Functions

#### `is_admin(e: &Env, address: &Address) -> bool`
Check if an address is the admin (non-panicking).

**Returns**: `true` if address is admin, `false` otherwise

#### `is_verifier(e: &Env, address: &Address) -> bool`
Check if an address is a registered verifier (non-panicking).

**Returns**: `true` if address is a verifier, `false` otherwise

#### `get_admin(e: &Env) -> Address`
Get the current admin address.

**Panics**: `"not initialized"` if contract not initialized

## Events

### `access_denied`
Emitted when access control check fails.

**Topics**: `("access_denied",)`

**Data**: `(caller: Address, role: Symbol, error_code: u32)`

**Error Codes**:
- `1` - NotAdmin
- `2` - NotVerifier
- `3` - NotIdentityOwner
- `4` - NotInitialized

### `verifier_added`
Emitted when a verifier is added.

**Topics**: `("verifier_added",)`

**Data**: `(verifier: Address,)`

### `verifier_removed`
Emitted when a verifier is removed.

**Topics**: `("verifier_removed",)`

**Data**: `(verifier: Address,)`

## Usage Patterns

### Pattern 1: Admin-Only Configuration

```rust
use crate::access_control::require_admin;

pub fn set_penalty_rate(e: Env, admin: Address, rate_bps: u32) {
    require_admin(&e, &admin);
    
    // Validate rate
    if rate_bps > 10000 {
        panic!("rate exceeds 100%");
    }
    
    // Store configuration
    e.storage().instance().set(&Symbol::new(&e, "penalty_rate"), &rate_bps);
}
```

### Pattern 2: Verifier Workflow

```rust
use crate::access_control::require_verifier;

pub fn approve_attestation(e: Env, verifier: Address, attestation_id: u64) {
    require_verifier(&e, &verifier);
    
    // Load attestation
    let key = Symbol::new(&e, "attestation");
    let mut attestation: Attestation = e.storage().instance().get(&key)
        .unwrap_or_else(|| panic!("attestation not found"));
    
    // Mark as verified
    attestation.verified = true;
    attestation.verifier = verifier.clone();
    
    e.storage().instance().set(&key, &attestation);
    e.events().publish((Symbol::new(&e, "attestation_approved"),), attestation_id);
}
```

### Pattern 3: Identity Owner Operations

```rust
use crate::access_control::require_identity_owner;

pub fn update_profile(e: Env, caller: Address, new_data: ProfileData) {
    // Load existing profile
    let key = Symbol::new(&e, "profile");
    let profile: Profile = e.storage().instance().get(&key)
        .unwrap_or_else(|| panic!("profile not found"));
    
    // Verify ownership
    require_identity_owner(&e, &caller, &profile.owner);
    
    // Update profile
    let updated = Profile {
        owner: profile.owner,
        data: new_data,
        updated_at: e.ledger().timestamp(),
    };
    
    e.storage().instance().set(&key, &updated);
}
```

### Pattern 4: Role Composition

```rust
use crate::access_control::require_admin_or_verifier;

pub fn flag_suspicious_activity(e: Env, reporter: Address, identity: Address, reason: Symbol) {
    // Either admin or verifier can flag
    require_admin_or_verifier(&e, &reporter);
    
    // Record the flag
    let flag = SuspiciousActivityFlag {
        identity,
        reporter: reporter.clone(),
        reason,
        timestamp: e.ledger().timestamp(),
    };
    
    e.storage().instance().set(&Symbol::new(&e, "flag"), &flag);
    e.events().publish((Symbol::new(&e, "activity_flagged"),), flag);
}
```

## Security Considerations

### 1. Event Logging
All access control failures emit `access_denied` events for security monitoring and audit trails. Monitor these events to detect:
- Unauthorized access attempts
- Potential security breaches
- Misconfigured permissions

### 2. Role Separation
- Admin role has ultimate authority - protect admin keys carefully
- Verifiers should be trusted entities with limited scope
- Identity owners control only their own data

### 3. Initialization
Always initialize the contract with `initialize(e: Env, admin: Address)` before using access control functions. Uninitialized contracts will panic on admin checks.

### 4. Verifier Management
- Only admin can add/remove verifiers
- Removed verifiers immediately lose access
- Consider implementing verifier rotation policies

### 5. No Role Escalation
The system prevents privilege escalation:
- Verifiers cannot become admin
- Identity owners cannot grant themselves verifier status
- Only admin can modify the verifier set

## Testing

The module includes comprehensive tests covering:

✅ Admin-only access control  
✅ Verifier-only access control  
✅ Identity owner access control  
✅ Role composition (admin OR verifier)  
✅ Unauthorized access scenarios  
✅ Access denial event emissions  
✅ Multiple verifiers management  
✅ Verifier addition/removal  
✅ Edge cases (re-adding verifiers, uninitialized state)  
✅ Event verification for all operations  

**Coverage Note**: Access-control paths are covered by dedicated unit tests, including unauthorized and event-emission scenarios.  
Project-wide numeric coverage requires an external coverage tool (for example `cargo-llvm-cov`), which is not bundled in this repository.

Run tests with:
```bash
cargo test --package credence_bond test_access_control
```

## Integration Example

```rust
use crate::access_control::{require_admin, require_verifier, require_identity_owner};

#[contract]
pub struct CredenceIdentity;

#[contractimpl]
impl CredenceIdentity {
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&Symbol::new(&e, "admin"), &admin);
    }
    
    pub fn add_verifier(e: Env, admin: Address, verifier: Address) {
        add_verifier_role(&e, &admin, &verifier);
    }
    
    pub fn verify_identity(e: Env, verifier: Address, identity: Address) {
        require_verifier(&e, &verifier);
        // Verification logic
    }
    
    pub fn update_identity(e: Env, caller: Address, identity: Address, data: IdentityData) {
        require_identity_owner(&e, &caller, &identity);
        // Update logic
    }
    
    pub fn emergency_freeze(e: Env, admin: Address, identity: Address) {
        require_admin(&e, &admin);
        // Emergency freeze logic
    }
}
```

## Invalid-Address Policy

Every privileged admin entrypoint that accepts a target `Address` MUST
reject the zero/invalid address sentinel.  The sentinel is the strkey
encoding of the all-zero Ed25519 public key:

```
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
```

Assigning a role to — or transferring ownership to — this address can
permanently strand governance, so the guard is enforced before any
authorisation or storage mutation.  The rejection uses
`ContractError::InvalidAdminAddress` (code 110).

### Entrypoints protected

| Entrypoint           | Parameter guarded     |
|----------------------|-----------------------|
| `add_admin`          | `new_admin`           |
| `update_admin_role`  | `admin_address`       |
| `transfer_ownership` | `new_owner`           |
| `reactivate_admin`   | `admin_address`       |
| `deactivate_admin`   | `admin_address`       |
| `remove_admin`       | `admin_to_remove`     |
| `set_pause_signer`   | `signer`              |

### Semantics

An address is considered **invalid** when its string representation
exactly equals the sentinel above.  This is a purely syntactic check:
it catches uninitialised / garbage strkeys.  A valid (non-sentinel)
contract or account address is never rejected by this guard.

## Future Enhancements

Potential improvements for future versions:

1. **Time-based Roles**: Roles with expiration timestamps
2. **Multi-sig Admin**: Require multiple admin signatures for critical operations
3. **Role Hierarchies**: More granular permission levels
4. **Delegation**: Temporary permission delegation
5. **Rate Limiting**: Prevent abuse by limiting operations per time period
6. **Role Metadata**: Store additional context about role assignments

## References

- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Access Control Best Practices](https://soroban.stellar.org/docs/learn/security)
- [Event Logging Guidelines](https://soroban.stellar.org/docs/learn/events)

## Bond Contract Privileged Access Matrix

The following table summarizes every privileged/admin entrypoint in the Bond Contract that requires authorization to prevent regressions. They are exhaustively checked through a data-driven negative matrix in the test suite to ensure:
- non-admin rejection coverage (unauthorized callers are rejected)
- uninitialized coverage (uninitialized contract execution fails)
- real auth enforcement coverage (genuine `require_auth` enforcement without test mocks)

| Entrypoint | Type | Scope |
|---|---|---|
| `set_early_exit_config` | Admin-only | Updates early exit parameters and sets treasury. |
| `register_attester` | Admin-only | Adds a new attester to the contract. |
| `unregister_attester` | Admin-only | Removes an existing attester from the contract. |
| `set_attester_stake` | Admin-only | Sets specific attester stake amount. |
| `set_weight_config` | Admin-only | Configures weighting multipliers and limits. |
| `slash` | Admin-only | Fully or partially slashes a bond and distributes fees. |
| `slash_bond` | Admin-only | Internal/variant wrapper to perform bond slashing. |
| `collect_fees` | Admin-only | Collects all accumulated protocol fees. |

*Note: Any new admin entrypoint must be appended to this matrix and included in the `PrivilegedCase` test array.*
