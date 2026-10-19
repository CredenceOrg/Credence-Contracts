# Claims Pagination

The `credence_bond` contract implements a pull-payment pattern for reward claims to prevent griefing attacks and failed transfers.

## Cursor-Pagination

To avoid unbounded loops and budget exhaustion during claims enumeration and processing, the contract implements cursor-based pagination. Claims are processed sequentially in insertion order (ordered by monotonically increasing `claim_id`).

### Key Functions

- `process_claim_by_id`: Allows processing a single claim by its unique ID.
- `process_claims_paginated`: Enables bulk processing of claims with a bounded limit. 

### Usage

```rust
// Process a single claim
let result = process_claim_by_id(&env, &user, claim_id);

// Process a batch of claims starting after a cursor
let cursor = 0; // Starts from the beginning
let limit = 50; // Maximum claims to process in this batch
let result = process_claims_paginated(&env, &user, cursor, limit, claim_types);
```

By providing a cursor and a maximum limit, callers can efficiently fetch and process large claim sets without exceeding transaction constraints. Processed claims are removed from the vector to maintain efficiency.
