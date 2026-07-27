# Fix Issue #746: Add verify_batch_size helper checking bounds

Closes #746

### Summary
This PR adds a `verify_batch_size` helper function in the `validation` module to centralize bounds checking for batch sizes across the contract. It ensures that any batch processed is neither empty nor exceeds the defined maximum size, rejecting non-conforming batches with the explicit, typed errors `ContractError::EmptyBatch` and `ContractError::BatchTooLarge`.

### Threat Model
**What does an attacker get if this check is missing?**
Without an explicit bounds check, an attacker could supply an exceptionally large batch size. While the transaction might eventually fail due to gas limits or out-of-memory conditions in the environment, the delay and resource consumption act as a Denial of Service (DoS) vector, degrading performance for legitimate users. It also makes error handling unclear. Additionally, allowing zero-length batches could bypass expected logic that assumes at least one item was processed, potentially corrupting application state or wasting computational cycles.

### Acceptance Criteria
- [x] Adds the `verify_batch_size` helper function.
- [x] Refactors existing batch bound checks to use the new helper.
- [x] Included negative test (already present but now accurately triggered by typed errors instead of generic panics).
- [x] Avoids generic 500 errors by leveraging `ContractError`.
- [x] Threat modeled.
