# Credence Registry Contract

The registry contract provides a bidirectional mapping system between identity addresses and their corresponding bond contract addresses within the Credence trust protocol.

## Overview

The registry serves as a central lookup system that enables:
- **Forward lookups**: Identity → Bond Contract
- **Reverse lookups**: Bond Contract → Identity
- **Registration tracking**: Active/inactive status monitoring
- **Event emission**: Complete audit trail of registry operations

## Architecture

### Data Structures

#### RegistryEntry
```rust
pub struct RegistryEntry {
    pub identity: Address,        // The identity address
    pub bond_contract: Address,   // The bond contract address
    pub registered_at: u64,       // Registration timestamp
    pub active: bool,             // Active status flag
}
```

### Storage Keys
- `Admin`: Administrator address
- `IdentityToBond(Address)`: Forward mapping storage
- `BondToIdentity(Address)`: Reverse mapping storage
- `RegisteredIdentities`: List of all registered identities

## Functions

### Administrative Functions

#### `initialize(admin: Address)`
Initializes the contract with an admin address.

**Authorization**: Requires admin signature
**Events**: Emits `registry_initialized`
**Panics**: If already initialized

#### `transfer_admin(new_admin: Address)`
Transfers admin rights to a new address.

**Authorization**: Requires current admin signature
**Events**: Emits `admin_transferred`
**Panics**: If not initialized or caller is not admin

#### `get_admin() -> Address`
Returns the current admin address.

**Panics**: If not initialized

### Registration Functions

#### `register(identity: Address, bond_contract: Address) -> RegistryEntry`
Registers a new identity-to-bond mapping.

**Authorization**: Requires admin signature
**Returns**: Created `RegistryEntry`
**Events**: Emits `identity_registered`
**Panics if**:
- Caller is not admin
- Identity is already registered
- Bond contract is already associated with another identity

**Example**:
```rust
let entry = client.register(&identity_addr, &bond_contract_addr);
```

#### `deactivate(identity: Address)`
Deactivates a registration (soft delete).

**Authorization**: Requires admin signature
**Events**: Emits `identity_deactivated`
**Panics if**:
- Caller is not admin
- Identity is not registered
- Identity is already deactivated

**Note**: Deactivation preserves the mapping data but marks it as inactive.

#### `reactivate(identity: Address)`
Reactivates a previously deactivated registration.

**Authorization**: Requires admin signature
**Events**: Emits `identity_reactivated`
**Panics if**:
- Caller is not admin
- Identity is not registered
- Identity is already active

### Lookup Functions

#### `get_bond_contract(identity: Address) -> RegistryEntry`
Forward lookup: retrieves the bond contract for a given identity.

**Returns**: Complete `RegistryEntry` including metadata
**Panics**: If identity is not registered

**Example**:
```rust
let entry = client.get_bond_contract(&identity_addr);
let bond = entry.bond_contract;
```

#### `get_identity(bond_contract: Address) -> Address`
Reverse lookup: retrieves the identity for a given bond contract.

**Returns**: Identity `Address`
**Panics**: If bond contract is not registered

**Example**:
```rust
let identity = client.get_identity(&bond_contract_addr);
```

#### `is_registered(identity: Address) -> bool`
Checks if an identity is registered and active.

**Returns**: `true` if registered and active, `false` otherwise

**Example**:
```rust
if client.is_registered(&identity_addr) {
    // Identity is active
}
```

#### `get_all_identities() -> Vec<Address>`
Returns a list of all registered identity addresses.

**Returns**: Vector of all identity addresses (both active and inactive)

**Example**:
```rust
let all_identities = client.get_all_identities();
for identity in all_identities.iter() {
    // Process each identity
}
```

## Events

The contract emits the following events for audit and monitoring:

| Event | Description | Data |
|-------|-------------|------|
| `registry_initialized` | Contract initialized | Admin address |
| `identity_registered` | New registration created | `RegistryEntry` |
| `identity_deactivated` | Registration deactivated | Updated `RegistryEntry` |
| `identity_reactivated` | Registration reactivated | Updated `RegistryEntry` |
| `admin_transferred` | Admin rights transferred | New admin address |

## Security Considerations

### Access Control
- **Admin-only operations**: `register`, `deactivate`, `reactivate`, `transfer_admin`
- All admin operations require signature verification via `require_auth()`
- Initialization is a one-time operation

### Data Integrity
- **Uniqueness guarantees**: Each identity can only map to one bond contract
- **Bidirectional consistency**: Both forward and reverse mappings are maintained atomically
- **No orphaned mappings**: Both directions are always kept in sync

### Registration Validations
1. **Duplicate prevention**: Cannot register the same identity twice
2. **Bond uniqueness**: Cannot register a bond contract that's already associated
3. **State checks**: Prevents double deactivation/reactivation

## Usage Patterns

### Basic Registration Flow
```rust
// 1. Initialize the registry
client.initialize(&admin);

// 2. Register an identity
let entry = client.register(&identity, &bond_contract);

// 3. Perform lookups
let bond = client.get_bond_contract(&identity);
let id = client.get_identity(&bond_contract);

// 4. Check status
if client.is_registered(&identity) {
    // Identity is active
}
```

### Deactivation/Reactivation Flow
```rust
// Deactivate a registration
client.deactivate(&identity);
assert!(!client.is_registered(&identity));

// Mappings still exist but marked inactive
let entry = client.get_bond_contract(&identity);
assert!(!entry.active);

// Reactivate when needed
client.reactivate(&identity);
assert!(client.is_registered(&identity));
```

### Batch Operations
```rust
// Get all registered identities
let identities = client.get_all_identities();

// Process each one
for identity in identities.iter() {
    if client.is_registered(&identity) {
        let entry = client.get_bond_contract(&identity);
        // Process active entry
    }
}
```

## Testing

The contract includes comprehensive test coverage (21 tests):

### Core Functionality Tests
- Initialization and double-initialization prevention
- Registration with duplicate prevention
- Forward and bidirectional lookups
- Registration status checking

### State Management Tests
- Deactivation and reactivation flows
- State preservation during deactivation
- Multiple registrations handling

### Admin Tests
- Admin-only operation verification
- Admin transfer functionality

### Edge Cases
- Unregistered identity/bond lookups
- Double deactivation/reactivation prevention
- Timestamp verification

Run tests:
```bash
cargo test -p credence_registry
```

## Deployment

### Build for WASM
```bash
cargo build --target wasm32-unknown-unknown --release -p credence_registry
```

The compiled WASM file will be located at:
```
target/wasm32-unknown-unknown/release/credence_registry.wasm
```

### Deploy with Soroban CLI
```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_registry.wasm \
  --source <SECRET_KEY> \
  --network <NETWORK>
```

### Initialize After Deployment
```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET_KEY> \
  --network <NETWORK> \
  -- initialize \
  --admin <ADMIN_ADDRESS>
```

## Integration with Credence Protocol

The registry contract integrates with the broader Credence ecosystem:

1. **Bond Contract**: Each identity has one bond contract registered
2. **Backend Services**: Query registry for identity/bond relationships
3. **Attestation System**: Verify identity ownership before attestations
4. **Delegation System**: Validate identity-bond mappings for delegated operations

## Future Enhancements

Potential improvements for future versions:

- **Pagination**: Add pagination support for `get_all_identities()`
- **Filtering**: Query by active/inactive status
- **Batch operations**: Register/deactivate multiple identities atomically
- **Metadata**: Add custom metadata fields to `RegistryEntry`
- **History tracking**: Record registration history and state transitions
- **Multi-admin**: Support multiple admin addresses with different permission levels

## License

Part of the Credence protocol contracts.
