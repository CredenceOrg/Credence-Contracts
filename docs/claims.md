# Claims Module

The claims module implements a pull-payment pattern for reward distribution in the Credence protocol.

## Overview

Instead of pushing payments directly to users (which can fail due to contract fallback behavior or be gamed by malicious recipients), the protocol creates pending claims that users must explicitly claim.

## Key Features

- **Pull-payment pattern**: Prevents griefing and failed transfers
- **Claim expiry**: Claims expire after 30 days if not claimed
- **Batch processing**: Process multiple claims in a single transaction
- **Pagination**: Enumerate and process claims in bounded chunks to prevent gas exhaustion
- **Claim types**: Support for multiple reward types (verifier rewards, slashing rewards, etc.)

## Pagination

The claims module provides cursor+limit pagination for safe enumeration of large claim sets without unbounded gas consumption.

### Pagination Functions

#### `get_pending_claims_paginated`

```rust
pub fn get_pending_claims_paginated(
    e: &Env,
    user: &Address,
    cursor: u64,
    limit: u32,
) -> Vec<PendingClaim>
```

Retrieve paginated pending claims for a user:
- `cursor`: Starting claim_id (0 for the first page).
- `limit`: Maximum number of claims to return (capped at `MAX_PAGINATION_LIMIT`).

**Deterministic ordering**: Claims are ordered by `claim_id`.

#### `process_claims_paginated`

```rust
pub fn process_claims_paginated(
    e: &Env,
    user: &Address,
    offset: u32,
    limit: u32,
    claim_types: Vec<ClaimType>,
) -> ClaimResult
```

Process claims with offset-based pagination:
- `offset`: Number of claims to skip.
- `limit`: Maximum claims to process (capped at `MAX_BATCH_CLAIMS`).
- `claim_types`: Optional filter for specific claim types.

### Examples

#### Retrieve Paginated Claims

```rust
let claims = get_pending_claims_paginated(&env, &user, 0, 10);
assert_eq!(claims.len(), 10);
```

#### Process Paginated Claims

```rust
let result = process_claims_paginated(
    &env,
    &user,
    0,
    20,
    Vec::from_slice(&env, &[ClaimType::VerifierReward]),
);
assert_eq!(result.processed_count, 20);
```

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_BATCH_CLAIMS` | 50 | Maximum claims per batch process |
| `MAX_PAGINATION_LIMIT` | 100 | Maximum claims per paginated enumeration |
| `DEFAULT_CLAIM_EXPIRY` | 30 days | Time before unclaimed rewards expire |

## Claim Types

- `VerifierReward`: Rewards for successful attestations
- `SlashingReward`: Rewards from successful challenges
- `PenaltyRefund`: Refunds from early exit penalties
- `FeeRebate`: Protocol fee rebates
- `DisputeReward`: Dispute resolution rewards

## Security Considerations

1. **Gas bounds**: All pagination functions have hard limits to prevent unbounded loops
2. **Auth required**: Claim processing requires user authentication
3. **Idempotency**: Processed claims cannot be reprocessed
4. **Deterministic ordering**: Cursor-based pagination provides consistent results
