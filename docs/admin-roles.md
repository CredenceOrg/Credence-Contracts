# Admin Role Management

## Overview

The Admin Role Management system provides comprehensive role-based access control for the Credence trust protocol. It implements a hierarchical role structure with secure assignment, revocation, and management capabilities.

## Role Hierarchy

The system implements three distinct roles with clear privilege separation:

### Super Admin (Level 3)
- **Highest privilege level**
- Can manage all other admins
- Can assign/revoke any role
- Can modify contract configuration
- Cannot be removed if they're the last super admin

### Admin (Level 2)
- **Intermediate privilege level**
- Can manage operators
- Can assign/revoke operator roles
- Can perform most administrative tasks
- Cannot manage other admins or super admins

### Operator (Level 1)
- **Basic privilege level**
- Can perform limited operational tasks
- Cannot manage other users
- Cannot assign roles

### Owner

The owner is the address stored at contract initialization. The owner is always treated as having the highest effective authority and can act on **any** admin, including other SuperAdmins, regardless of the strict-rank rule. This override exists specifically so that a SuperAdmin who has been suspended or deactivated by a peer can always be recovered by the owner. The owner is not exempt from the effective-status check: a deactivated or suspended owner cannot act until reinstated.

See [OPERATOR_BALANCES.md](OPERATOR_BALANCES.md) for what each operation costs and how to keep balances healthy.

## Core Features

### Multi-Admin Support
- Support for multiple administrators at each role level
- Configurable minimum and maximum admin limits
- Prevents single points of failure

### Role Assignment & Revocation
- Secure role assignment with authorization checks
- Role-based permissions for management operations
- Audit trail through event emission

### Self-Removal Protection
- Prevents removal of the last admin at any role level
- Configurable minimum admin requirements
- Ensures system continuity

### Activity Management
- Admin deactivation and reactivation
- Maintains audit history during deactivation
- Preserves role assignments during inactive periods
- Timed suspension — block-bounded, self-expiring suspension distinct from indefinite deactivation

## Effective Authority

Every privileged entry point in the admin contract (and the pausable admin auth check) resolves the caller's authority through a single internal helper, `require_effective_role(e, caller, min_role) -> AdminInfo`. This helper enforces the following conditions and returns the caller's `AdminInfo` on success:

1. The caller must be a registered admin.
2. The caller must be active (`inactive = false`).
3. The caller must not be suspended at the current ledger timestamp (`now >= suspended_until`).
4. The caller's role must be at least `min_role`.

If any condition fails the helper reverts with `AdminSuspended` (when the caller is deactivated or suspended) or `NotAdmin` (when the caller is not an admin or lacks the required role). The raw stored role is only exposed through the public read-only view `get_role` and must never be used for authorization.

### Rank Rule for Acting on Another Admin

When an admin acts on another admin (remove, deactivate, reactivate, suspend, or update role), the caller's **effective** role must be **strictly greater** than the target's **current** role. This is the single rank rule applied uniformly across all these entry points:

- Equal rank is not enough to act on a peer. Two SuperAdmins cannot suspend, deactivate, remove, or demote each other.
- A caller cannot act on themselves through these entry points. In particular, `reactivate_admin` rejects `caller == target`, so a deactivated admin cannot restore themselves.
- `update_admin_role` additionally requires that the caller may assign the **new** role (`require_role_at_least(caller, get_required_role_to_assign(new_role))`). Both conditions must hold.
- The owner overrides the strict-rank rule for acting on SuperAdmins (see "Owner" above).

## Authority Matrix

The following table is the normative specification for the five privileged entry points that act on another admin. "Effective role" means the caller is active and not suspended; a deactivated or suspended caller is always denied regardless of role.

| Entry point         | Caller effective role | Target current role | Allowed? | Notes |
|---------------------|----------------------|-------------------|----------|-------|
| `remove_admin`       | Operator                | Operator          | No      | Equal rank |
| `remove_admin`       | Admin                   | Operator          | Yes     | Strictly greater |
| `remove_admin`        | Admin                   | Admin             | No      | Equal rank |
| `remove_admin`       | Admin                   | SuperAdmin        | No      | Lower rank |
| `remove_admin`        | SuperAdmin              | Admin             | Yes     | Strictly greater |
| `remove_admin`       | SuperAdmin             | SuperAdmin        | No      | Equal rank |
| `remove_admin`       | Owner                   | SuperAdmin        | Yes     | Owner override |
| `deactivate_admin`   | Operator                | Operator          | No      | Equal rank |
| `deactivate_admin`   | Admin                   | Operator          | Yes     | Strictly greater |
| `deactivate_admin`   | Admin                   | Admin             | No      | Equal rank |
| `deactivate_admin`   | Admin                   | SuperAdmin        | No      | Lower rank |
| `deactivate_admin`   | SuperAdmin             | Admin             | Yes     | Strictly greater |
| `deactivate_admin`   | SuperAdmin             | SuperAdmin        | No      | Equal rank |
| `deactivate_admin`   | Owner                   | SuperAdmin        | Yes     | Owner override |
| `reactivate_admin`   | Operator                | Operator          | No      | Equal rank |
| `reactivate_admin`   | Admin                   | Operator          | Yes     | Strictly greater |
| `reactivate_admin`   | Admin                   | Admin             | No      | Equal rank |
| `reactivate_admin`   | Admin                   | SuperAdmin        | No      | Lower rank |
| `reactivate_admin`   | SuperAdmin             | Admin             | Yes     | Strictly greater |
| `reactivate_admin`   | SuperAdmin             | SuperAdmin        | No      | Equal rank |
| `reactivate_admin`   | Owner                   | SuperAdmin        | Yes     | Owner override |
| `reactivate_admin`   | Any (caller == target) | Any               | No      | Self-reactivation guard |
| `suspend_admin`      | Operator                | Operator          | No      | Equal rank |
| `suspend_admin`      | Admin                   | Operator          | Yes     | Strictly greater |
| `suspend_admin`      | Admin                   | Admin             | No      | Equal rank |
| `suspend_admin`      | Admin                   | SuperAdmin        | No      | Lower rank |
| `suspend_admin`      | SuperAdmin              | Admin             | Yes     | Strictly greater |
| `suspend_admin`      | SuperAdmin              | SuperAdmin        | No      | Equal rank |
| `suspend_admin`      | Owner                   | SuperAdmin        | Yes     | Owner override |
| `update_admin_role` | Operator                | Operator          | No      | Equal rank |
| `update_admin_role` | Admin                   | Operator          | Yes     | Strictly greater + may assign new role |
| `update_admin_role` | Admin                   | Admin             | No      | Equal rank |
| `update_admin_role` | Admin                   | SuperAdmin        | No      | Lower rank |
| `update_admin_role` | SuperAdmin              | Admin             | Yes     | Strictly greater + may assign new role |
| `update_admin_role` | SuperAdmin             | SuperAdmin        | No      | Equal rank |
| `update_admin_role` | Owner                   | SuperAdmin        | Yes     | Owner override + assign check |

Additional rules:

- A deactivated or suspended caller is denied at every entry point (`AdminSuspended`), including the owner.
- `update_admin_role` requires both the strict-rank rule against the target's current role and the "may assign the new role" check.
- Suspension and deactivation of a SuperAdmin by a peer SuperAdmin is denied; only the owner can act on a SuperAdmin.

## Security Features

### Authorization Checks
- Role-based access control for all operations
- Prevents privilege escalation
- Validates caller permissions before execution
- Status-aware effective-role check on every privileged entry point

### Input Validation
- Address validation for all admin operations
- Bounds checking for configuration parameters
- Prevention of duplicate admin assignments

### Event Emission
- Comprehensive event logging for all admin operations
- Audit trail for compliance and monitoring
- Real-time notification of role changes

### Reentrancy Protection
- Guard against reentrancy attacks
- Atomic operation execution
- State consistency guarantees

## API Reference

### Initialization

```rust
initialize(env, super_admin, min_admins, max_admins)
```
- Initializes the contract with a super admin
- Sets minimum and maximum admin limits
- Emits `admin_initialized` event

### Admin Management

```rust
add_admin(env, caller, new_admin, role)
```
- Adds a new admin with specified role
- Requires appropriate authorization level
- Emits `admin_added` event

```rust
remove_admin(env, caller, admin_to_remove)
```
- Removes an admin from the system
- Requires the caller's effective role to be strictly greater than the target's current role (owner override applies)
- Validates minimum admin requirements
- Emits `admin_removed` event

```rust
update_admin_role(env, caller, admin_address, new_role)
```
- Updates an admin's role
- Requires the caller's effective role to be strictly greater than the target's current role (owner override applies)
- Additionally requires the caller may assign `new_role` in the role hierarchy
- Maintains audit trail
- Emits `admin_role_updated` event

### Activity Management

```rust
deactivate_admin(env, caller, admin_address)
```
- Deactivates an admin **permanently** (until manually reactivated)
- Requires the caller's effective role to be strictly greater than the target's current role (owner override applies)
- Preserves admin history
- Emits `admin_deactivated` event

```rust
reactivate_admin(env, caller, admin_address)
```
- Reactivates a deactivated admin
- Requires the caller's effective role to be strictly greater than the target's current role (owner override applies)
- Rejects `caller == target` (self-reactivation guard)
- Restores full privileges
- Emits `admin_reactivated` event

```rust
suspend_admin(env, caller, admin, until_ts)
```
- Suspends `admin` until `until_ts` (Unix timestamp, seconds)
- `until_ts` must be **strictly in the future**; equal or past timestamps are rejected with `AdminSuspended` (113)
- While suspended, `is_admin` and `has_role_at_least` return `false`
- **Auto-reactivation**: once `e.ledger().timestamp() >= until_ts` the admin is automatically treated as active again — no second transaction is required
- Authorization: caller's effective role must be **strictly greater** than the target's current role (owner override applies)
- MinAdmins guard: rejection if suspending would leave fewer than `min_admins` effective active admins
- Emits `admin_suspended` with `(admin_address, until_ts)`

**Suspension vs. deactivation**

| | `deactivate_admin` | `suspend_admin` |
|---|---|---|
| Expiry | Indefinite — manual `reactivate_admin` required | Automatic at `until_ts` |
| Use case | Long-term revocation | Temp key rotation, incident cool-off |
| Recovery tx | Required | None |

### Query Functions

```rust
get_admin_info(env, admin_address)
```
- Returns detailed information about an admin
- Includes role, assignment history, and status

```rust
get_role(env, admin_address)
```
- Public read-only view returning the raw stored role
- Ignores activation and suspension status; **must not** be used for authorization. Use `is_admin` / `has_role_at_least` for status-aware checks.

```rust
is_admin(env, address)
```
- Checks if an address is an active admin
- Returns boolean result

```rust
has_role_at_least(env, address, required_role)
```
- Checks if address has at least the specified role level
- Status-aware: returns `false` for deactivated or suspended admins
- Useful for authorization checks

```rust
get_all_admins(env)
```
- Returns list of all admin addresses
- Includes both active and inactive admins

```rust
get_admins_by_role(env, role)
```
- Returns list of admins with specific role
- Useful for role-based queries

## Configuration

### Admin Limits
- `min_admins`: Minimum number of admins required (default: 1)
- `max_admins`: Maximum number of admins allowed (default: 100)

### Role Assignment Rules
- Super Admin: Can only be assigned by Super Admin (or owner)
- Admin: Can be assigned by Super Admin (or owner)
- Operator: Can be assigned by Super Admin, Admin, or owner

## Usage Examples

### Basic Setup

```rust
// Initialize contract with super admin
admin_contract.initialize(env, super_admin_address, 1, 100);

// Add additional admins
admin_contract.add_admin(env, super_admin_address, admin_address, AdminRole::Admin);
admin_contract.add_admin(env, admin_address, operator_address, AdminRole::Operator);
```

### Role Management

```rust
// Promote operator to admin
admin_contract.update_admin_role(env, super_admin_address, operator_address, AdminRole::Admin);

// Temporarily deactivate admin
admin_contract.deactivate_admin(env, super_admin_address, admin_address);

// Reactivate admin when needed
admin_contract.reactivate_admin(env, super_admin_address, admin_address);
```

### Authorization Checks

```rust
// Check if user has admin privileges
if admin_contract.is_admin(env, user_address) {
    // Proceed with admin operation
}

// Check specific role requirements
if admin_contract.has_role_at_least(env, user_address, AdminRole::Admin) {
    // Proceed with admin-level operation
}
```

## Events

The contract emits the following events for audit and monitoring:

- `admin_initialized`: Contract initialization
- `admin_added`: New admin added
- `admin_removed`: Admin removed
- `admin_role_updated`: Admin role changed
- `admin_deactivated`: Admin deactivated (indefinite)
- `admin_reactivated`: Admin reactivated (manual)
- `admin_suspended`: Admin suspended until a future timestamp; auto-reactivation is implicit — no `admin_reinstated` event is emitted

## Security Considerations

### Minimum Admin Protection
- System prevents removal of last admin at any role level
- Configurable minimum admin requirements
- Ensures administrative continuity

### Privilege Escalation Prevention
- Strict role hierarchy enforcement
- Authorization checks for all operations
- Self-assignment restrictions for higher roles
- Status-aware effective-role check on every privileged entry point
- Strictly-greater rank rule for acting on another admin (equal rank is never enough)
- Deactivated admins cannot reactivate themselves

### Audit Trail
- Complete event logging for all operations
- Immutable role assignment history
- Compliance-friendly audit capabilities

## Best Practices

### Admin Distribution
- Maintain multiple admins at each role level
- Distribute responsibilities across different addresses
- Regularly review and update admin assignments

### Role Assignment
- Follow principle of least privilege
- Assign minimum necessary roles
- Regularly audit role assignments

### Security Monitoring
- Monitor admin events for suspicious activity
- Implement alerting for critical operations
- Regular security reviews of admin permissions

## Integration

The admin contract can be integrated with other Credence contracts to provide centralized role management:

```rust
// Check admin permissions in other contracts
let admin_contract = AdminContractClient::new(&env, &admin_contract_address);
if admin_contract.is_admin(&env, &caller) {
    // Proceed with protected operation
}
```

## Testing

The contract includes comprehensive tests covering:
- Role hierarchy enforcement
- Authorization checks
- Edge cases and boundary conditions
- Security scenarios
- Large-scale operations
- Status-aware effective-role enforcement (deactivated/suspended callers denied)
- Strictly-greater rank rule across all admin-on-admin entry points
- Owner override on SuperAdmins
- Self-reactivation guard

Run tests with:
```bash
cargo test -p admin
```

## Deployment

### Prerequisites
- Soroban CLI installed
- Appropriate network configuration
- Admin addresses prepared

### Deployment Steps
1. Build the contract: `cargo build --target wasm32-unknown-unknown --release -p admin`
2. Deploy to network: `soroban contract deploy --wasm target/wasm32-unknown-unknown/release/admin.wasm`
3. Initialize with super admin: Call `initialize` function
4. Add additional admins as needed

### Post-Deployment
- Verify admin functionality
- Test role assignments
- Configure monitoring for admin events
- Document admin procedures and responsibilities

## Two-Step Admin Transfer

For the operator runbook and rationale, see [TWO-STP