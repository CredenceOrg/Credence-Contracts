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

### Functions

#### `get_pending_claims_paginated`

```rust
pub fn get_pending_claims_paginated(
    e: &Env,
    user: &Address,
    cursor: u64,
    limit: u32,
) -> Vec<PendingClaim>
```

Returns paginated pending claims using cursor-based pagination:
- `cursor`: Starting claim_id (0 for first page, or last_seen_id from previous page)
- `limit`: Maximum claims to return (capped at MAX_PAGINATION_LIMIT = 100)

**Deterministic ordering**: Claims are ordered by `claim_id`, ensuring consistent results across pages even when claims are added/removed.

#### `get_pending_claims_count`

```rust
pub fn get_pending_claims_count(e: &Env, user: &Address) -> u32
```

Returns the count of unprocessed pending claims for a user.

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
- `offset`: Number of claims to skip
- `limit`: Maximum claims to process (capped at MAX_BATCH_CLAIMS = 50)
- `claim_types`: Optional filter for specific claim types

#### `get_claim_by_id`

```rust
pub fn get_claim_by_id(e: &Env, claim_id: u64) -> PendingClaim
```

Retrieve a specific claim by ID.

#### `process_claim_by_id`

```rust
pub fn process_claim_by_id(e: &Env, user: &Address, claim_id: u64) -> ClaimResult
```

Process a single specific claim by ID.

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
