# Reentrancy Protection in CredenceBond

## Overview

The CredenceBond contract implements a panic-safe reentrancy guard to protect against reentrancy attacks during external contract calls. This document describes the implementation, security guarantees, and usage patterns.

## Background

### The Reentrancy Problem

Reentrancy attacks occur when a contract makes an external call to another contract, and that external contract calls back into the original contract before the first invocation completes. This can lead to:

- **State inconsistency**: The contract's state may be in an intermediate, invalid state during the callback
- **Double-spending**: Funds could be withdrawn multiple times
- **Logic bypass**: Security checks could be circumvented

### Functions with External Calls

The following CredenceBond functions invoke external callbacks and are protected by the reentrancy guard:

1. **`withdraw_bond`**: Withdraws the full bonded amount and invokes an `on_withdraw` callback
2. **`slash_bond`**: Slashes a portion of the bond and invokes an `on_slash` callback  
3. **`collect_fees`**: Collects protocol fees and invokes an `on_collect` callback

In production, these callbacks would be replaced with token transfer operations (e.g., USDC transfers), which are also external calls that could potentially re-enter.

## Implementation

### RAII-Style Guard

The reentrancy protection uses a **Resource Acquisition Is Initialization (RAII)** pattern with automatic cleanup:

```rust
/// RAII guard that releases the reentrancy lock on drop.
/// This ensures the lock is released even if a panic occurs.
struct ReentrancyGuard<'a> {
    env: &'a Env,
}

impl<'a> Drop for ReentrancyGuard<'a> {
    fn drop(&mut self) {
        let key = Symbol::new(self.env, "locked");
        self.env.storage().instance().set(&key, &false);
    }
}

fn acquire_lock(e: &Env) -> ReentrancyGuard {
    let key = Symbol::new(e, "locked");
    let locked: bool = e.storage().instance().get(&key).unwrap_or(false);
    if locked {
        panic_with_error!(e, ContractError::ReentrancyDetected);
    }
    e.storage().instance().set(&key, &true);
    ReentrancyGuard { env: e }
}
```

### Key Properties

1. **Automatic Release**: The lock is automatically released when the `ReentrancyGuard` goes out of scope, even if a panic occurs
2. **Panic Safety**: No code path can leave the lock permanently stuck at `true`
3. **Zero Runtime Overhead**: The guard is a zero-sized type; the compiler optimizes away the wrapper

### Function Structure

All protected functions follow a four-phase pattern:

```rust
pub fn protected_function(e: Env, ...) -> Result {
    // PHASE 1: Read and validate BEFORE acquiring lock
    // - All storage reads
    // - All validation checks
    // - All computations
    // This ensures no panic can occur while holding the lock during reads
    
    let data = e.storage().instance().get(...)?;
    validate(data)?;
    let result = compute(data);
    
    // PHASE 2: Acquire lock (RAII guard ensures automatic release)
    let _guard = Self::acquire_lock(&e);
    
    // PHASE 3: State updates (checks-effects-interactions pattern)
    e.storage().instance().set(...);
    
    // PHASE 4: External call (with lock held, but guard ensures release on panic)
    if let Some(callback) = get_callback() {
        e.invoke_contract(...);
    }
    
    // Lock automatically released when _guard goes out of scope
    result
}
```

## Security Guarantees

### 1. Reentrancy Prevention

**Guarantee**: A reentrant call to any protected function will be rejected with `ContractError::ReentrancyDetected`.

**Test Coverage**:
- `test_reentrant_callback_rejected_withdraw`
- `test_reentrant_callback_rejected_slash`
- `test_reentrant_callback_rejected_collect_fees`

### 2. Panic Safety

**Guarantee**: If a panic occurs anywhere in a protected function (including during external calls), the lock will be automatically released.

**Test Coverage**:
- `test_panic_in_callback_releases_lock`
- `test_lock_released_after_panic_in_callback`

**Note**: In Soroban's execution model, panics typically cause transaction rollback, so the lock state would revert anyway. However, the RAII pattern provides defense-in-depth and would be critical in environments that support unwinding.

### 3. No Stuck Locks

**Guarantee**: No execution path can permanently set the lock to `true`. Every lock acquisition is paired with an automatic release.

**Test Coverage**:
- `test_panic_before_lock_acquisition_no_stuck_lock`
- `test_sequential_calls_work_after_lock_release`
- `test_lock_state_during_successful_operation`

### 4. Validation Before Lock

**Guarantee**: All validation and reads occur BEFORE the lock is acquired. This minimizes the time the lock is held and ensures validation errors cannot leave the lock stuck.

**Test Coverage**:
- `test_slash_validation_error_before_lock`
- `test_collect_fees_not_admin_before_lock`
- `test_rolling_bond_notice_validation_before_lock`

## Checks-Effects-Interactions Pattern

In addition to the reentrancy guard, all protected functions follow the **Checks-Effects-Interactions** pattern:

1. **Checks**: Validate all preconditions (BEFORE lock acquisition)
2. **Effects**: Update contract state (AFTER lock acquisition, BEFORE external calls)
3. **Interactions**: Make external calls (AFTER state updates, WITH lock held)

This ensures that even if reentrancy were somehow possible, the contract state would already be updated to prevent double-spending or other exploits.

### Example: withdraw_bond

```rust
// CHECKS (before lock)
let bond = read_and_validate_bond()?;
validate_owner()?;
validate_active()?;
validate_notice_period()?;
let amount = calculate_withdrawal(bond);

// Acquire lock
let _guard = Self::acquire_lock(&e);

// EFFECTS (after lock, before external call)
bond.bonded_amount = 0;
bond.active = false;
e.storage().instance().set(&bond_key, &bond);

// INTERACTIONS (after state update, with lock held)
if let Some(callback) = get_callback() {
    e.invoke_contract(&callback, "on_withdraw", amount);
}
```

## Testing Strategy

### Test Categories

1. **Basic Reentrancy Detection**: Verify that setting the lock manually triggers detection
2. **Reentrant Callbacks**: Use a malicious callback contract that attempts to re-enter
3. **Panic Safety**: Verify lock is released even when callbacks panic
4. **Validation Ordering**: Verify validation errors occur before lock acquisition
5. **Sequential Operations**: Verify lock is properly released between operations
6. **Lock State Tracking**: Verify lock state is correct throughout operations
7. **Hostile Token Fault Injection**: Use a malicious SEP-41-compatible token
   whose `transfer` / `transfer_from` callback attempts to re-enter bond
   entrypoints while the outer call is moving funds.

### Hostile Token Threat Model

`ChaosToken` also exposes a configurable hostile-token mode for adversarial
fault injection. When armed, its next `transfer` or `transfer_from` attempts a
cross-contract call back into the bond contract and records whether that call
was rejected. The token uses `try_invoke_contract` so a rejected re-entry can be
observed without forcing the outer token transfer to fail.

The hostile-token suite covers re-entry attempts into:

- `withdraw` from a slash-to-treasury transfer after the lock-up period.
- `withdraw_early` from an early-exit treasury/user payout.
- `slash` from a slash-to-treasury transfer.
- `top_up` from an allowance-based `transfer_from`.
- `collect_fees` from an early-exit payout while protocol fees remain pending.

Each attack asserts that the re-entry was attempted, the nested call was
rejected, the lock is released after the outer call, and bond invariants still
hold (`slashed_amount <= bonded_amount`, non-negative balances, and no
double-spend of token balances).

### Current Audit Note

The hostile-token tests intentionally exercise the real token transfer paths,
not only test callback hooks. During review, the highest-risk surfaces are any
fund-moving entrypoint that calls `TokenClient::transfer` or
`TokenClient::transfer_from` before acquiring the same reentrancy lock used by
withdrawal, slashing, and fee collection paths. If a hostile-token test fails,
do not relax the assertion; treat it as evidence that the entrypoint is missing
guard coverage and either document the bypass or add the same guard to that
path after audit approval.

### Malicious Callback Contract

The test suite includes a `MaliciousCallback` contract that attempts various reentrancy attacks:

```rust
#[contract]
pub struct MaliciousCallback;

#[contractimpl]
impl MaliciousCallback {
    pub fn on_withdraw(env: Env, _amount: i128) {
        // Attempt reentrant call to withdraw_bond
        let client = CredenceBondClient::new(&env, &bond_addr);
        let _ = client.try_withdraw_bond(&owner);
    }
    
    pub fn on_withdraw_panic(_env: Env, _amount: i128) {
        panic!("intentional panic during callback");
    }
}
```

## Future Considerations

### Token Transfer Integration

When real token transfers are added (replacing the callback mechanism), the reentrancy surface will expand:

1. **Token contract calls**: Transfers to ERC20/Stellar tokens could re-enter
2. **Multiple external calls**: A single operation might involve multiple token transfers
3. **Cross-contract interactions**: Token contracts might have their own callbacks

**Recommendation**: Keep the reentrancy guard in place even after token integration. The guard provides defense-in-depth against unexpected reentrancy vectors.

### Gas Optimization

The current implementation prioritizes security over gas optimization. Potential optimizations:

1. **Selective guarding**: Only guard functions that make external calls
2. **Read-only reentrancy**: Allow reentrant reads while blocking writes
3. **Per-function locks**: Use separate locks for independent operations

**Recommendation**: Do not optimize until profiling shows the guard is a bottleneck. Security should take precedence.

### Cross-Contract Reentrancy

The current guard only protects against reentrancy within the same contract instance. Cross-contract reentrancy (where Contract A calls Contract B, which calls Contract C, which calls back to Contract A) is not prevented.

**Recommendation**: Follow the Checks-Effects-Interactions pattern strictly. Update all state before making any external calls, regardless of reentrancy guards.

## Audit Checklist

When auditing reentrancy protection:

- [ ] All functions with external calls use `acquire_lock`
- [ ] All reads and validations occur BEFORE `acquire_lock`
- [ ] The guard is stored in a variable (e.g., `let _guard = ...`) to ensure it lives until function end
- [ ] No manual `release_lock` calls exist (rely on RAII Drop)
- [ ] State updates occur AFTER lock acquisition but BEFORE external calls
- [ ] External calls are the last operations in the function
- [ ] Test coverage includes reentrant callbacks and panic scenarios

## References

- [Reentrancy Attack (Ethereum)](https://consensys.github.io/smart-contract-best-practices/attacks/reentrancy/)
- [Checks-Effects-Interactions Pattern](https://docs.soliditylang.org/en/latest/security-considerations.html#use-the-checks-effects-interactions-pattern)
- [RAII Pattern in Rust](https://doc.rust-lang.org/rust-by-example/scope/raii.html)
- [Soroban Security Best Practices](https://soroban.stellar.org/docs/learn/security)

## Changelog

### 2024-01-XX: Initial Implementation
- Implemented RAII-style reentrancy guard with automatic lock release
- Refactored `withdraw_bond`, `slash_bond`, and `collect_fees` to use panic-safe guard
- Moved all reads and validations before lock acquisition
- Added comprehensive test suite with malicious callback contract
- Documented security guarantees and usage patterns
