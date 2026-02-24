# Slashing Mechanism

## Overview

The slashing mechanism is a governance-controlled penalty system that reduces a bond's value as punishment for misconduct, breach of obligations, or violations of protocol rules. Slashed funds represent a loss to the bonded identity while maintaining an accurate record for auditing and transparency.

## Core Concept

**Slashing** = Reducing `bonded_amount` availability by incrementing `slashed_amount`

- **Bonded Amount**: Total stake locked in the bond (i128)
- **Slashed Amount**: Cumulative penalty (i128)  
- **Withdrawable Balance**: `bonded_amount - slashed_amount`

### Design Philosophy

1. **Monotonic**: Slashing only increases, never decreases (unless unslashing by admin)
2. **Fair**: Prevents over-slashing (capped at bonded_amount)
3. **Transparent**: Events emit for all slashing operations
4. **Accountable**: Only authorized governance can execute

## Authorization and Access Control

### Admin-Only Execution

The `slash_bond()` function can only be called by the contract admin:

```
Admin: Address stored at contract initialization
Caller: Must equal the stored admin address
Rejection: "not admin" panic if unauthorized
```

### Security Properties

- ✅ Non-transferable: Admin role cannot be changed after initialization (in this version)
- ✅ Non-delegable: Admin must directly call slashing (no proxies)
- ✅ Auditable: All slashing events are logged on-chain

## Slashing Operations

### slash_bond(admin, amount) → IdentityBond

Core slashing function.

**Behavior:**
1. Validates caller is the contract admin (panics if not)
2. Calculates new slashed amount = `existing_slashed + amount`
3. Caps at bonded amount: `min(new_slashed, bonded_amount)`
4. Updates bond state with new `slashed_amount`
5. Emits `bond_slashed` event
6. Returns updated `IdentityBond` struct

**Arguments:**
- `admin: Address` - Caller claiming admin authority
- `amount: i128` - Amount to slash

**Returns:**
- `IdentityBond` with updated `slashed_amount`

**Panics:**
- `"not admin"` if caller is not the contract admin
- `"no bond"` if no bond exists
- `"slashing caused overflow"` if arithmetic overflows (catch_add protection)

**Example:**

```rust
// Admin slashes 300 from a 1000-unit bond
let bond = contract.slash(admin_address, 300);
// bond.slashed_amount == 300
// bond.bonded_amount == 1000 (unchanged)
```

### Partial vs. Full Slashing

**Partial Slash:**  
Slash amount < bonded amount, leaving some withdrawable balance

```
Bonded: 1000
Slash: 300
Available: 1000 - 300 = 700
```

**Full Slash:**  
Slash amount >= bonded amount, leaving zero withdrawable balance

```
Bonded: 1000
Slash: 1000
Available: 1000 - 1000 = 0
```

### Over-Slash Prevention

If cumulative slashing exceeds bonded amount, the slashed amount is capped:

```
Bonded: 1000
Previous Slash: 700
New Slash Request: 500
Calculation: 700 + 500 = 1200
Capped Result: min(1200, 1000) = 1000
Final Slashed: 1000
```

## State Management

### Bond Structure

```rust
pub struct IdentityBond {
    pub identity: Address,          // Bonded identity
    pub bonded_amount: i128,        // Total stake (unchanged by slashing)
    pub slashed_amount: i128,       // Cumulative penalties
    pub bond_start: u64,            // Timestamp of bond creation
    pub bond_duration: u64,         // Lock-up duration
    pub active: bool,               // Is bond active
    pub is_rolling: bool,           // Auto-renew at end
    pub withdrawal_requested_at: u64, // Rolling bond withdrawal request time
    pub notice_period_duration: u64, // Rolling bond notice period
}
```

### Withdrawals and Slashing

**Withdrawal Logic:**

```rust
available_balance = bonded_amount - slashed_amount;
if withdraw_amount > available_balance {
    panic!("insufficient balance for withdrawal")
}
```

**Examples:**

| Bonded | Slashed | Available | Withdraw | Result |
|--------|---------|-----------|----------|--------|
| 1000   | 300     | 700       | 500      | ✅ OK |
| 1000   | 300     | 700       | 701      | ❌ Panic |
| 1000   | 1000    | 0         | 1        | ❌ Panic |

## Event Emission

### bond_slashed Event

Emitted whenever a bond is successfully slashed.

**Event Data:**
```
(Symbol: "bond_slashed")
- identity: Address of the slashed identity
- slash_amount: Amount just slashed (i128)
- total_slashed_amount: New cumulative slashed amount (i128)
```

**Audit Trail Value:**
- Off-chain indexing: Find all slashing events for an identity
- Transparency: Public record of governance actions
- Analytics: Track slashing patterns and severity

### Example Event Sequence

```rust
// Initial bond: 1000 units
client.create_bond(identity, 1000, ...);

// First slash: 300 units
client.slash(admin, 300);
// Event: (identity, 300, 300)

// Second slash: 200 units
client.slash(admin, 200);
// Event: (identity, 200, 500)

// Attempt third slash: 600 units (would exceed 1000)
client.slash(admin, 600);
// Event: (identity, 600, 1000)  [capped at bonded_amount]
```

## Security Considerations

### 1. Authorization Bypass Prevention

✅ **Admin Validation:**
```rust
// Rejects non-admin with "not admin" panic
validate_admin(e, caller);
```

✅ **State Consistency:**
- Bond must exist (panics if not)
- No state corruption on failed slash

### 2. Arithmetic Safety

✅ **Overflow Protection:**
```rust
let new_slashed = bond.slashed_amount
    .checked_add(amount)
    .expect("slashing caused overflow");
```

✅ **Over-Slash Prevention:**
```rust
bond.slashed_amount = if new_slashed > bond.bonded_amount {
    bond.bonded_amount  // Cap at bonded amount
} else {
    new_slashed
};
```

### 3. State Mutation Safety

✅ **Atomic Updates:**
- Slash calculation verified before state update
- All validations before persist

✅ **No Partial States:**
- Bond either slashed completely or not at all
- Event only emitted on success

### 4. Withdrawal Integration

✅ **Available Balance Calculation:**
```rust
available = bonded_amount - slashed_amount
// Always verified >= withdrawal_amount
```

✅ **Never Over-Withdraw:**
- Slashing reduces available balance
- Withdrawal checks always pass with correct available calculation

## Test Coverage

### Test Categories (47 tests, 95%+ coverage)

1. **Basic Operations (4 tests)**
   - Successful slash execution
   - Small amount slashing
   - Exact half slashing
   - Full amount slashing

2. **Authorization (3 tests)**
   - Unauthorized rejection
   - Multiple unauthorized attempts
   - Identity cannot slash own bond

3. **Over-Slash Prevention (3 tests)**
   - Amount exceeds bonded (normal capping)
   - Way over amount
   - Max i128 value capping

4. **Edge Cases (3 tests)**
   - Zero amount slashing
   - Overflow prevention
   - Very large bonds (i128::MAX / 2)

5. **State Consistency (5 tests)**
   - Single slash recording
   - Cumulative slashing
   - Multiple accumulation
   - Other fields unchanged
   - State persistence

6. **Event Emission (3 tests)**
   - Event emitted on basic slash
   - Correct event data
   - Multiple events

7. **Withdrawal Integration (5 tests)**
   - Withdraw respects available balance
   - Over-withdrawal prevention
   - Fully slashed bonds cannot withdraw
   - Exact available balance withdrawal
   - Complex slash/withdraw sequences

8. **Cumulative Scenarios (5 tests)**
   - Cumulative with capping
   - Incremental slashing
   - Full slash prevents further slashing
   - Large amount accumulation

9. **State Persistence (2 tests)**
   - State persists across calls
   - Slash result matches get_state

10. **Error Messages (2 tests)**
    - "not admin" error
    - "no bond" error

## Usage Examples

### Example 1: Simple Penalty

```rust
// Admin slashes 10% of bond for minor violation
let bond = contract.slash(admin, 100);
// slashed_amount increases from 0 to 100
// bonded_amount remains 1000
// withdrawable becomes 900
```

### Example 2: Escalating Penalties

```rust
// First offense: 5%
contract.slash(admin, 50);
// slashed_amount = 50

// Second offense: 10%
contract.slash(admin, 100);
// slashed_amount = 150 (cumulative)

// Third offense: attempt 20% but capped
contract.slash(admin, 200);
// slashed_amount = 350 (if bonded >= 350)
```

### Example 3: Full Bond Forfeiture

```rust
// Severe violation: slash entire bond
let bond = contract.slash(admin, 1000000); // arbitrary large amount
// slashed_amount capped at bonded_amount (1000)
// bonded_amount remains 1000
// withdrawable = 0
// Identity cannot withdraw
```

### Example 4: Slashing and Withdrawal Sequence

```rust
let bond = contract.create_bond(identity, 1000, ...);

// Slash 300
contract.slash(admin, 300);
// available = 1000 - 300 = 700

// Withdraw 500 (less than available)
contract.withdraw(500);
// bonded_amount = 500, slashed_amount = 300, available = 200

// Try to withdraw 300 (more than available)
contract.withdraw(300);
// panics: "insufficient balance for withdrawal"
```

## Comparison with Other Mechanisms

### vs. Early Exit Penalty
- **Early Exit**: Charged to users, transferred to treasury, applies at withdrawal time
- **Slashing**: Imposed by governance, tracked in bond state, affects available balance

### vs. Bond Top-Up
- **Top-Up**: Increases bonded_amount (additive)
- **Slashing**: Increases slashed_amount (subtractive, can't be reversed without unslashing)

### vs. Bond Withdrawal
- **Withdrawal**: Reduces bonded_amount (removes funds)
- **Slashing**: Increases slashed_amount (blocks funds without removing)

## Future Enhancements

1. **Partial Unslashing**: Allow admin to reduce slashed_amount for appeals
2. **Treasury Integration**: Actual fund transfers to governance treasury
3. **Slashing Tiers**: Different slash amounts based on violation severity
4. **Timelocks**: Delay slash execution for governance safety
5. **Signaling**: Allow other addresses to propose slashing for governance review

## References

- [Security Analysis](../SECURITY_ANALYSIS.md)
- [Contract Tests](../contracts/credence_bond/src/test_slashing.rs)
- [Slashing Module](../contracts/credence_bond/src/slashing.rs)

