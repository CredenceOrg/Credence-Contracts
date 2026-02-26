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

## Security Features

### Authorization Checks
- Role-based access control for all operations
- Prevents privilege escalation
- Validates caller permissions before execution

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
- Validates minimum admin requirements
- Emits `admin_removed` event

```rust
update_admin_role(env, caller, admin_address, new_role)
```
- Updates an admin's role
- Maintains audit trail
- Emits `admin_role_updated` event

### Activity Management

```rust
deactivate_admin(env, caller, admin_address)
```
- Deactivates an admin temporarily
- Preserves admin history
- Emits `admin_deactivated` event

```rust
reactivate_admin(env, caller, admin_address)
```
- Reactivates a deactivated admin
- Restores full privileges
- Emits `admin_reactivated` event

### Query Functions

```rust
get_admin_info(env, admin_address)
```
- Returns detailed information about an admin
- Includes role, assignment history, and status

```rust
is_admin(env, address)
```
- Checks if an address is an active admin
- Returns boolean result

```rust
has_role_at_least(env, address, required_role)
```
- Checks if address has at least the specified role level
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
- Super Admin: Can only be assigned by Super Admin
- Admin: Can be assigned by Super Admin
- Operator: Can be assigned by Super Admin or Admin

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
- `admin_deactivated`: Admin deactivated
- `admin_reactivated`: Admin reactivated

## Security Considerations

### Minimum Admin Protection
- System prevents removal of last admin at any role level
- Configurable minimum admin requirements
- Ensures administrative continuity

### Privilege Escalation Prevention
- Strict role hierarchy enforcement
- Authorization checks for all operations
- Self-assignment restrictions for higher roles

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
