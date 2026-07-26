# Test Helper Library

This document outlines the test utilities available to **contributors** working on Credence Contracts. Instead of reinventing boilerplate for every test suite, we provide a centralized `testutils` crate alongside feature-gated helpers built directly into our contracts. 

Writing tests using these common primitives ensures our test suites remain readable, maintainable, and uniform across the workspace.

## The `testutils` Crate

The `crates/testutils` library provides common primitives needed across all contract test suites. 

### Generating Addresses

Instead of manually generating random bytes or using hardcoded identifiers, use the deterministic address generators from `testutils`. This ensures your tests communicate intent clearly.

```rust
use soroban_sdk::{Env, testutils::Address as _};
use testutils::{admin, user, attacker};

#[test]
fn test_access_control() {
    let env = Env::default();
    
    // Generate role-specific addresses
    let protocol_admin = admin(&env);
    let alice = user(&env);
    let malicious_actor = attacker(&env);
    
    // Use them directly in your test assertions or client calls
    assert_ne!(protocol_admin, malicious_actor);
}
```

### Vector Deduplication

When tests require validating unique sets (e.g., ensuring a batch of items has no duplicates), use the stable deduplication helpers. These retain the original insertion order of elements.

```rust
use soroban_sdk::{Env, Vec};
use testutils::deduplicate_stable;

#[test]
fn test_batch_processing() {
    let env = Env::default();
    let mut inputs = Vec::new(&env);
    inputs.push_back(1u32);
    inputs.push_back(2u32);
    inputs.push_back(1u32); // Duplicate

    // Remove duplicates while preserving original insertion order
    let unique_inputs = deduplicate_stable(&env, &inputs);
    assert_eq!(unique_inputs.len(), 2);
}
```

## Contract-Specific Helpers (Feature Gated)

Many contracts compile test-only helpers when the `testutils` feature is enabled. For example, `credence_bond` exposes internal getters that bypass pagination for easier assertions in tests.

To use these in your test harness, ensure the feature is enabled in your `Cargo.toml`:

```toml
[dev-dependencies]
credence_bond = { path = "../../contracts/credence_bond", features = ["testutils"] }
```

### Example: Slash History Verification

Instead of paginating through `get_slash_history_page` in tests, use the feature-gated `get_slash_history` to retrieve the entire history in one call.

```rust
#[cfg(test)]
mod tests {
    use credence_bond::slash_history::testutils::get_slash_history;

    #[test]
    fn test_slashing_records() {
        let env = Env::default();
        // ... execute slash ...
        
        // Retrieve full history directly (only available in test builds)
        let history = get_slash_history(&env);
        assert_eq!(history.len(), 1);
    }
}
```

---
*For details on how the `testutils` feature gate is applied in release builds vs test builds, see [testutils-feature.md](../contracts/credence_bond/docs/testutils-feature.md).*
