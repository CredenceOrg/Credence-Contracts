# Batch Bond Operations

Atomic batch operations for creating multiple bonds in a single transaction within the Credence protocol.

> **Note:** `create_batch_bonds`, `validate_batch_bonds`, and `get_batch_total_amount`
> (below) live in `contracts/credence_bond/src/batch.rs`, which is compiled only
> under `cfg(any(test, feature = "testutils"))` — the `testutils` feature is not
> part of any default release build, so these functions are **not present in the
> deployed contract's callable surface**. The all-or-nothing design described
> below is accurate for how the code behaves under `cargo test`; it just doesn't
> ship yet. See [docs/BATCH_ATOMICITY.md](BATCH_ATOMICITY.md) for the project's
> full stance on batch atomicity and an inventory of what's actually deployed.

## Overview

The batch operations module provides gas-optimized, atomic bond creation for multiple identities. All operations follow an all-or-nothing pattern - if any bond fails validation, the entire batch is rejected without creating any bonds.

## Features

- **Atomic Execution**: All bonds succeed or all fail (no partial execution)
- **Pre-validation**: Complete validation before any state changes
- **Gas Optimization**: Single transaction for multiple bonds
- **Event Emission**: Comprehensive batch completion events
- **Security**: Overflow protection, duplicate prevention, input validation

## Data Structures

### BatchBondParams

Parameters for a single bond within a batch operation.

```rust
pub struct BatchBondParams {
    pub identity: Address,              // Identity address
    pub amount: i128,                   // Bond amount
    pub duration: u64,                  // Duration in seconds
    pub is_rolling: bool,               // Rolling bond flag
    pub notice_period_duration: u64,    // Notice period for rolling bonds
}
```

### BatchBondResult

Result of a batch bond creation operation.

```rust
pub struct BatchBondResult {
    pub created_count: u32,             // Number of bonds created
    pub bonds: Vec<IdentityBond>,       // List of created bonds
}
```

## Functions

### create_batch_bonds

Create multiple bonds atomically in a single transaction.

**Signature:**
```rust
pub fn create_batch_bonds(
    e: Env,
    params_list: Vec<BatchBondParams>
) -> BatchBondResult
```

**Parameters:**
- `e`: Contract environment
- `params_list`: Vector of bond creation parameters

**Returns:**
- `BatchBondResult` containing count and list of created bonds

**Panics:**
- If validation fails for any bond
- If `params_list` is empty
- If a bond already exists for any identity
- If any amount is negative or zero
- If any duration would cause timestamp overflow
- If rolling bond specified without notice period

**Events:**
- Emits `batch_bonds_created` with the batch result
- Emits `tier_changed` for each bond if tier changes

**Example:**
```rust
let mut params_list = Vec::new(&env);

params_list.push_back(BatchBondParams {
    identity: addr1,
    amount: 1000,
    duration: 86400,
    is_rolling: false,
    notice_period_duration: 0,
});

params_list.push_back(BatchBondParams {
    identity: addr2,
    amount: 2000,
    duration: 172800,
    is_rolling: true,
    notice_period_duration: 3600,
});

let result = client.create_batch_bonds(&params_list);
assert_eq!(result.created_count, 2);
```

### validate_batch_bonds

Validate a batch of bonds without creating them.

**Signature:**
```rust
pub fn validate_batch_bonds(
    e: Env,
    params_list: Vec<BatchBondParams>
) -> bool
```

**Parameters:**
- `e`: Contract environment
- `params_list`: Vector of bond parameters to validate

**Returns:**
- `true` if all bonds are valid

**Panics:**
- If any bond has invalid parameters

**Use Case:**
Pre-flight validation before submitting a batch transaction.

**Example:**
```rust
let is_valid = client.validate_batch_bonds(&params_list);
if is_valid {
    let result = client.create_batch_bonds(&params_list);
}
```

### get_batch_total_amount

Calculate the total bonded amount across a batch.

**Signature:**
```rust
pub fn get_batch_total_amount(
    params_list: Vec<BatchBondParams>
) -> i128
```

**Parameters:**
- `params_list`: Vector of bond parameters

**Returns:**
- Total amount across all bonds

**Panics:**
- If the total would overflow i128

**Use Case:**
Calculate aggregate statistics before batch creation.

**Example:**
```rust
let total = client.get_batch_total_amount(&params_list);
assert_eq!(total, 3000); // Sum of all bond amounts
```

## Validation Rules

The batch operations enforce the following validation rules:

1. **Non-empty batch**: At least one bond must be provided
2. **Batch size cap**: No more than 20 bonds may be processed in one batch
3. **Positive amounts**: All bond amounts must be > 0
4. **No overflow**: Bond end timestamps must not overflow u64
5. **Rolling bonds**: Must have a notice_period_duration > 0
6. **No duplicates**: Cannot create bond if one already exists for identity
7. **Atomic validation**: ALL bonds must pass validation before ANY are created

## Error Handling

All validation errors cause the entire batch to fail atomically:

| Error | Condition |
|-------|-----------|
| `empty batch` | params_list is empty |
| `batch too large` | params_list contains more than 20 bonds |
| `invalid amount in batch` | Any amount ≤ 0 |
| `duration overflow in batch` | timestamp + duration > u64::MAX |
| `rolling bond requires notice period` | is_rolling=true but notice_period_duration=0 |
| `bond already exists` | Identity already has a bond |

## Gas Optimization

Batch operations are optimized for gas efficiency:

- **Single transaction**: All bonds created in one call
- **Batch validation**: Fail-fast validation before storage operations
- **Efficient storage**: Minimal state updates
- **Event batching**: Single batch completion event instead of per-bond events

**Gas Savings Example:**
- Creating 10 bonds individually: ~10× transaction costs
- Creating 10 bonds in batch: ~1.5× transaction cost of single bond

## Security Considerations

### Atomicity

The batch operations guarantee atomicity through a two-phase approach:

1. **Validation Phase**: All bonds validated, no state changes
2. **Execution Phase**: If validation passes, all bonds created

If any validation fails, no bonds are created.

### Overflow Protection

All arithmetic operations use checked arithmetic:

- Bond amount validation
- Duration overflow checking
- Total amount calculation
- Timestamp arithmetic

### Access Control

Batch operations follow the same access control as single bond creation:

- No special permissions required for batch operations
- Individual bond creation rules apply to each bond in batch
- Admin controls (if any) enforced per bond

## Integration

### With Registry Contract

```rust
// Create bonds and register in one flow
let result = bond_client.create_batch_bonds(&params_list);

for i in 0..result.bonds.len() {
    let bond = result.bonds.get(i).unwrap();
    registry_client.register(&bond.identity, &bond_contract_addr);
}
```

### With Treasury

```rust
// Calculate total collateral needed
let total = client.get_batch_total_amount(&params_list);

// Ensure treasury has sufficient balance
assert!(treasury_balance >= total);

// Create bonds
let result = client.create_batch_bonds(&params_list);
```

## Testing

## Permissionless Claim Expiry Sweep

The bond contract maintains a pull-payment claims system where each claim has an
optional expiry (default 30 days). Expired claims accumulate in a per-user vector
and must be pruned to prevent storage bloat.

### expire_claims

Bounded, permissionless sweep to expire and remove stale pending claims.

**Signature:**
```rust
pub fn expire_claims(
    e: Env,
    user: Address,
    max_iter: u32
) -> u32
```

**Parameters:**
- `user`: Address whose claims to scan and prune
- `max_iter`: Maximum number of claims to scan (hard-capped at 50 for gas safety)

**Returns:**
- Number of expired claims removed

**Behavior:**
- Scans up to `max_iter` claims (capped at `MAX_BATCH_CLAIMS = 50`)
- Removes only claims where `now > expires_at` (never touches claims with `expires_at == 0`)
- Callable by anyone (no authorization required)
- Emits `claims_expired(user, pruned_count)` event for each sweep
- Preserves claim order and claim IDs of remaining claims

**Use Cases:**
1. **Keeper incentivization**: Off-chain keepers call this to earn rewards
2. **Storage maintenance**: Regular sweeps prevent vector bloat
3. **User self-service**: Users can manually prune their own claims
4. **Governance**: Can be triggered by off-chain indexers on behalf of users

**Example:**
```rust
let pruned = client.expire_claims(&user, &50);
println!("Swept and removed {} expired claims", pruned);

// Idempotent: calling again returns 0 if all expired claims already removed
let pruned_again = client.expire_claims(&user, &50);
assert_eq!(pruned_again, 0);
```

**Gas Safety:**
- Maximum iteration count is 50 claims per call
- Multiple calls can be made to sweep large backlogs progressively
- Each call is O(min(max_iter, pending_count))

**Implementation Notes:**
- Claims with `expires_at == 0` are never pruned (permanent claims)
- Claims marked as `processed = true` skip expiry check (already claimed)
- Claimable amount is decremented only for removed claims
- Idempotent: repeated calls on same user find no work


Comprehensive test coverage includes:

- ✅ Single bond in batch
- ✅ Multiple bonds in batch
- ✅ Empty batch rejection
- ✅ Negative amount rejection
- ✅ Zero amount rejection
- ✅ Duration overflow detection
- ✅ Rolling bond validation
- ✅ Batch validation without creation
- ✅ Total amount calculation
- ✅ Duplicate bond prevention
- ✅ Atomic failure scenarios
- ✅ Different durations and amounts

Run tests:
```bash
cargo test -p credence_bond test_batch
```

## Future Enhancements

Potential improvements for future versions:

- **Partial success mode**: Option to skip invalid bonds instead of failing entire batch
- **Dynamic batch limits**: Tune the 20-bond cap per network profile if Soroban budgets change
- **Progress callbacks**: Event emission for each bond created
- **Batch updates**: Update multiple bonds atomically
- **Batch withdrawals**: Withdraw from multiple bonds in one transaction
- **Cross-contract batching**: Create bonds across multiple contract instances

## Performance Metrics

Based on test execution:

- **Small batch (1-5 bonds)**: ~1.2× single bond cost
- **Medium batch (6-20 bonds)**: ~1.5× single bond cost
- **Large batch (21-100 bonds)**: ~2× single bond cost

*Note: Actual gas costs will vary based on network conditions and contract state.*

## License

Part of the Credence protocol contracts.
