# Multi-Signature Contract

## Overview

The Credence Multi-Signature (MultiSig) contract provides a secure, flexible framework for governance and administrative actions requiring multi-party approval. It implements a threshold signature scheme where proposals must receive a configurable number of signatures before execution.

## Key Features

- **Configurable Threshold**: Set the required number of signatures (1 to N signers)
- **Proposal System**: Submit, sign, and execute proposals with full lifecycle management
- **Flexible Action Types**: Support for contract calls, transfers, configuration changes, and custom actions
- **Signer Management**: Add/remove signers with automatic threshold adjustment
- **Expiration Support**: Optional proposal expiration timestamps
- **Event Emission**: Comprehensive event logging for all operations
- **Security**: Authorization checks, replay prevention, and status validation

## Architecture

### Data Structures

#### ActionType
Defines the type of action a proposal represents:
- `ContractCall`: Generic contract call to another contract
- `Transfer`: Token/asset transfer
- `ConfigChange`: Configuration modification
- `SignerManagement`: Add/remove signer operations
- `Custom`: Custom action type for extensions

#### ProposalStatus
Tracks the lifecycle state of a proposal:
- `Pending`: Awaiting signatures
- `Executed`: Successfully executed
- `Rejected`: Rejected by admin
- `Expired`: Passed expiration timestamp

#### Proposal
Complete proposal data structure containing:
- Unique identifier
- Action type and target details
- Function name and arguments (for contract calls)
- Description and metadata
- Timestamps (proposed, expiration)
- Current status
- Proposer address

### Storage Keys

The contract uses the following storage keys:
- `Admin`: Contract administrator
- `Signer(Address)`: Authorized signer mapping
- `SignerCount`: Total number of signers
- `SignerList`: Vector of all signer addresses
- `Threshold`: Required signature count
- `ProposalCounter`: Auto-incrementing proposal ID
- `Proposal(u64)`: Proposal data by ID
- `Signature(u64, Address)`: Signature tracking
- `SignatureCount(u64)`: Cached signature count per proposal

## API Reference

### Initialization

#### `initialize(admin, signers, threshold)`
Initialize the multi-sig contract with initial configuration.

**Parameters:**
- `admin: Address` - Admin address with management privileges
- `signers: Vec<Address>` - Initial list of authorized signers
- `threshold: u32` - Required number of signatures (1 ≤ threshold ≤ signer count)

**Panics:**
- If signers list is empty
- If threshold is 0 or exceeds signer count

**Events:**
- `multisig_initialized`

**Example:**
```rust
let admin = Address::generate(&e);
let signers = vec![&e, signer1, signer2, signer3];
client.initialize(&admin, &signers, &2); // 2-of-3 multisig
```

### Signer Management

#### `add_signer(admin, signer)`
Add a new authorized signer. Only admin can call.

**Parameters:**
- `admin: Address` - Admin address (must authenticate)
- `signer: Address` - Address to add as signer

**Panics:**
- If caller is not admin
- If signer already exists

**Events:**
- `signer_added`

#### `remove_signer(admin, signer)`
Remove a signer. Threshold auto-adjusts if it exceeds new signer count.

**Parameters:**
- `admin: Address` - Admin address (must authenticate)
- `signer: Address` - Address to remove

**Panics:**
- If caller is not admin
- If signer doesn't exist
- If removing would leave zero signers

**Events:**
- `signer_removed`
- `threshold_auto_adjusted` (if threshold was adjusted)

#### `set_threshold(admin, threshold)`
Update the signature threshold requirement.

**Parameters:**
- `admin: Address` - Admin address (must authenticate)
- `threshold: u32` - New threshold value

**Panics:**
- If caller is not admin
- If threshold is 0 or exceeds signer count

**Events:**
- `threshold_updated`

### Proposal Lifecycle

#### `submit_proposal(...)`
Submit a new proposal for multi-sig approval.

**Parameters:**
- `proposer: Address` - Signer submitting (must authenticate)
- `action_type: ActionType` - Type of action
- `target: Option<Address>` - Target contract (optional)
- `function_name: Option<String>` - Function to call (optional)
- `arguments: Option<Bytes>` - Encoded arguments (optional)
- `description: String` - Human-readable description
- `expires_at: u64` - Expiration timestamp (0 = no expiration)
- `metadata: Option<String>` - Custom metadata (optional)

**Returns:**
- `u64` - Unique proposal ID

**Panics:**
- If caller is not a signer
- If description is empty

**Events:**
- `proposal_submitted`

**Example:**
```rust
let proposal_id = client.submit_proposal(
    &signer1,
    &ActionType::ConfigChange,
    &None,
    &None,
    &None,
    &String::from_str(&e, "Update protocol fee to 1%"),
    &0_u64, // no expiration
    &None,
);
```

#### `sign_proposal(signer, proposal_id)`
Sign an existing proposal. Only signers can sign.

**Parameters:**
- `signer: Address` - Signer address (must authenticate)
- `proposal_id: u64` - ID of proposal to sign

**Panics:**
- If caller is not a signer
- If proposal doesn't exist
- If proposal is not pending
- If proposal has expired
- If signer already signed

**Events:**
- `proposal_signed`

**Example:**
```rust
client.sign_proposal(&signer1, &proposal_id);
client.sign_proposal(&signer2, &proposal_id);
```

#### `execute_proposal(proposal_id)`
Execute a proposal once threshold is met. Anyone can execute.

**Parameters:**
- `proposal_id: u64` - ID of proposal to execute

**Panics:**
- If proposal doesn't exist
- If proposal is not pending
- If proposal has expired
- If signature count < threshold

**Events:**
- `proposal_executed`
- `proposal_expired` (if expired during execution attempt)

**Note:** This marks the proposal as executed but doesn't perform the actual action. The caller must invoke the target contract or perform the action separately.

**Example:**
```rust
// After getting enough signatures
client.execute_proposal(&proposal_id);
```

#### `reject_proposal(admin, proposal_id)`
Reject a proposal. Only admin can reject.

**Parameters:**
- `admin: Address` - Admin address (must authenticate)
- `proposal_id: u64` - ID of proposal to reject

**Panics:**
- If caller is not admin
- If proposal doesn't exist
- If proposal is not pending

**Events:**
- `proposal_rejected`

### Query Functions

#### `get_proposal(proposal_id) -> Proposal`
Retrieve full proposal details by ID.

#### `get_signature_count(proposal_id) -> u32`
Get current signature count for a proposal.

#### `has_signed(proposal_id, signer) -> bool`
Check if a signer has signed a specific proposal.

#### `is_signer(address) -> bool`
Check if an address is an authorized signer.

#### `get_threshold() -> u32`
Get the current signature threshold.

#### `get_signer_count() -> u32`
Get the total number of authorized signers.

#### `get_signers() -> Vec<Address>`
Get list of all authorized signer addresses.

#### `get_admin() -> Address`
Get the admin address.

## Usage Patterns

### 1. Standard 2-of-3 MultiSig

```rust
// Initialize
let admin = Address::generate(&e);
let signers = vec![&e, signer1, signer2, signer3];
client.initialize(&admin, &signers, &2);

// Submit proposal
let proposal_id = client.submit_proposal(
    &signer1,
    &ActionType::Transfer,
    &Some(recipient),
    &None,
    &None,
    &String::from_str(&e, "Transfer 1000 tokens to treasury"),
    &0_u64,
    &None,
);

// Sign with 2 signers
client.sign_proposal(&signer1, &proposal_id);
client.sign_proposal(&signer2, &proposal_id);

// Execute
client.execute_proposal(&proposal_id);
```

### 2. Contract Call with Arguments

```rust
// Encode arguments for a contract call
let args = encode_contract_args(&e, arg1, arg2);

let proposal_id = client.submit_proposal(
    &proposer,
    &ActionType::ContractCall,
    &Some(target_contract),
    &Some(String::from_str(&e, "update_config")),
    &Some(args),
    &String::from_str(&e, "Update configuration parameters"),
    &0_u64,
    &None,
);
```

### 3. Proposal with Expiration

```rust
let current_time = e.ledger().timestamp();
let expires_at = current_time + 86400; // 24 hours

let proposal_id = client.submit_proposal(
    &proposer,
    &ActionType::ConfigChange,
    &None,
    &None,
    &None,
    &String::from_str(&e, "Time-sensitive configuration update"),
    &expires_at,
    &None,
);
```

### 4. Dynamic Signer Management

```rust
// Add a new signer
client.add_signer(&admin, &new_signer);

// Increase threshold
client.set_threshold(&admin, &3);

// Remove compromised signer
client.remove_signer(&admin, &compromised_signer);
// Threshold auto-adjusts if needed
```

## Security Considerations

### Authorization
- All sensitive operations require proper authentication
- Admin-only functions: `add_signer`, `remove_signer`, `set_threshold`, `reject_proposal`
- Signer-only functions: `submit_proposal`, `sign_proposal`
- Public functions: `execute_proposal` (after threshold met), all query functions

### Replay Prevention
- Each signer can sign a proposal only once
- Duplicate signatures are rejected

### State Validation
- Proposals can only be signed/executed when in `Pending` status
- Expired proposals cannot be signed or executed
- Executed/rejected proposals cannot be modified

### Threshold Safety
- Threshold must always be: 1 ≤ threshold ≤ signer count
- Automatic threshold adjustment when removing signers
- Cannot remove last signer

### Overflow Protection
- All arithmetic operations use checked arithmetic
- Signature count, signer count, and proposal ID tracking protected against overflow

### Event Transparency
- All state-changing operations emit events
- Events include relevant data for off-chain monitoring

## Events

| Event | Data | Description |
|-------|------|-------------|
| `multisig_initialized` | (admin, signer_count, threshold) | Contract initialized |
| `signer_added` | signer | New signer added |
| `signer_removed` | signer | Signer removed |
| `threshold_updated` | threshold | Threshold changed |
| `threshold_auto_adjusted` | threshold | Threshold auto-adjusted after signer removal |
| `proposal_submitted` | (proposal_id, proposer, action_type, description) | New proposal created |
| `proposal_signed` | (proposal_id, signer, signature_count) | Proposal signed |
| `proposal_executed` | (proposal_id, action_type, signatures) | Proposal executed |
| `proposal_rejected` | (proposal_id, admin) | Proposal rejected |
| `proposal_expired` | proposal_id | Proposal expired |

## Testing

The contract includes comprehensive test coverage:

### Initialization Tests
- Valid initialization
- Empty signers list rejection
- Invalid threshold (zero, exceeds signers)

### Signer Management Tests
- Add/remove signers
- Duplicate signer prevention
- Last signer protection
- Automatic threshold adjustment

### Threshold Tests
- Setting valid thresholds
- Zero threshold rejection
- Exceeding signer count rejection

### Proposal Tests
- Submission by signers
- Non-signer submission rejection
- Empty description rejection
- Multiple proposals

### Signing Tests
- Valid signing
- Non-signer signing rejection
- Double signing prevention
- Multiple signers

### Execution Tests
- Execution with sufficient signatures
- Insufficient signatures rejection
- Already executed rejection
- Exact threshold scenarios

### Expiration Tests
- Signing expired proposal rejection
- Executing expired proposal rejection

### Threshold Scenarios
- 1-of-1, 2-of-3, 3-of-3 configurations
- 2-of-5 configuration
- Complex multi-proposal scenarios

### Integration Tests
- Multiple proposals with different outcomes
- Signer management workflow
- End-to-end governance scenarios

## Gas Optimization

- Cached counts (signer_count, signature_count) to avoid iteration
- Signer list stored as vector for efficient enumeration
- Minimal storage operations
- Early validation to fail fast

## Limitations and Future Enhancements

### Current Limitations
- `execute_proposal` marks as executed but doesn't perform the actual action
- No built-in proposal cancellation by proposer
- No weighted voting (all signers equal)
- No delegation mechanism

### Potential Enhancements
- **Weighted Voting**: Assign voting power to signers
- **Proposal Cancellation**: Allow proposers to cancel pending proposals
- **Timelock**: Mandatory delay between approval and execution
- **Batch Proposals**: Submit multiple proposals atomically
- **Vote Delegation**: Allow signers to delegate voting power
- **Proposal Queuing**: Priority queue for proposals
- **Automatic Execution**: Execute when threshold is reached (with callback)
- **Role-Based Access**: Different roles with different permissions

## Integration Examples

### Treasury Integration
```rust
// MultiSig controlling treasury withdrawals
let proposal_id = multisig.submit_proposal(
    &signer,
    &ActionType::ContractCall,
    &Some(treasury_contract),
    &Some(String::from_str(&e, "propose_withdrawal")),
    &Some(encode_args(&e, &recipient, &amount)),
    &String::from_str(&e, "Withdraw 1000 from treasury"),
    &0_u64,
    &None,
);

// After signatures and execution
if multisig.get_proposal(&proposal_id).status == ProposalStatus::Executed {
    // Invoke treasury withdrawal
}
```

### Bond Contract Governance
```rust
// MultiSig for slashing governance
let proposal_id = multisig.submit_proposal(
    &governor,
    &ActionType::ContractCall,
    &Some(bond_contract),
    &Some(String::from_str(&e, "slash")),
    &Some(encode_args(&e, &bond_id, &slash_amount)),
    &String::from_str(&e, "Slash bond due to violation"),
    &expiry,
    &Some(String::from_str(&e, "{\"violation_id\": \"123\"}")),
);
```

## Best Practices

1. **Threshold Selection**: Choose threshold based on security needs vs. operational efficiency
2. **Proposal Descriptions**: Use clear, detailed descriptions for transparency
3. **Expiration Times**: Set reasonable expiration for time-sensitive proposals
4. **Metadata Usage**: Include relevant context in metadata field
5. **Event Monitoring**: Monitor events for off-chain tracking and alerting
6. **Regular Audits**: Periodically review signer list and threshold
7. **Secure Key Management**: Protect signer private keys appropriately
8. **Testing**: Test all proposal types in development environment first

## Conclusion

The Credence Multi-Signature contract provides a robust, flexible foundation for decentralized governance and administrative actions. Its configurable threshold system, comprehensive proposal lifecycle management, and security features make it suitable for a wide range of use cases from simple treasury management to complex governance workflows.
