# Protocol Parameters Documentation

## Overview

The protocol parameters system provides governance-controlled configuration for the Credence Bond contract. All parameters have defined types, units, and enforced min/max bounds to ensure safe operation.

## Governance Control

**Who can update parameters:** Only the contract admin (governance address) can update protocol parameters.

**Authorization:** All setter functions require the caller to be the contract admin. Non-admin callers are rejected with "not admin" error.

**Access pattern:**
```rust
// Admin must be authenticated
admin.require_auth();

// Admin address must match stored admin
let stored_admin: Address = e.storage().instance().get(&DataKey::Admin).unwrap();
if admin != stored_admin {
    panic!("not admin");
}
```

## Parameter Categories

### 1. Fee Rates

Fee rates are expressed in basis points (bps), where 1 bps = 0.01%.

#### Protocol Fee Rate

- **Parameter:** `protocol_fee_bps`
- **Type:** `u32`
- **Unit:** Basis points (bps)
- **Default:** 50 bps (0.5%)
- **Minimum:** 0 bps (0%)
- **Maximum:** 1000 bps (10%)
- **Description:** Protocol-wide fee charged on operations
- **Getter:** `get_protocol_fee_bps(e: &Env) -> u32`
- **Setter:** `set_protocol_fee_bps(e: &Env, admin: &Address, value: u32)`

#### Attestation Fee Rate

- **Parameter:** `attestation_fee_bps`
- **Type:** `u32`
- **Unit:** Basis points (bps)
- **Default:** 10 bps (0.1%)
- **Minimum:** 0 bps (0%)
- **Maximum:** 500 bps (5%)
- **Description:** Fee charged for attestation operations
- **Getter:** `get_attestation_fee_bps(e: &Env) -> u32`
- **Setter:** `set_attestation_fee_bps(e: &Env, admin: &Address, value: u32)`

### 2. Cooldown Periods

Cooldown periods are time delays between certain operations, expressed in seconds.

#### Withdrawal Cooldown

- **Parameter:** `withdrawal_cooldown_secs`
- **Type:** `u64`
- **Unit:** Seconds
- **Default:** 604,800 seconds (7 days)
- **Minimum:** 0 seconds (no cooldown)
- **Maximum:** 2,592,000 seconds (30 days)
- **Description:** Time delay required between withdrawal request and execution
- **Getter:** `get_withdrawal_cooldown_secs(e: &Env) -> u64`
- **Setter:** `set_withdrawal_cooldown_secs(e: &Env, admin: &Address, value: u64)`

#### Slash Cooldown

- **Parameter:** `slash_cooldown_secs`
- **Type:** `u64`
- **Unit:** Seconds
- **Default:** 86,400 seconds (24 hours)
- **Minimum:** 0 seconds (no cooldown)
- **Maximum:** 604,800 seconds (7 days)
- **Description:** Time delay required between consecutive slash operations
- **Getter:** `get_slash_cooldown_secs(e: &Env) -> u64`
- **Setter:** `set_slash_cooldown_secs(e: &Env, admin: &Address, value: u64)`

### 3. Tier Thresholds

Tier thresholds define value boundaries that determine user/operation tiers. Values are expressed in token units (smallest denomination).

#### Bronze Tier Threshold

- **Parameter:** `bronze_threshold`
- **Type:** `i128`
- **Unit:** Token units (e.g., stroops for XLM, smallest unit for USDC)
- **Default:** 100,000,000 (100 tokens with 6 decimals)
- **Minimum:** 0
- **Maximum:** 1,000,000,000,000 (1 million tokens with 6 decimals)
- **Description:** Minimum bonded amount to achieve Bronze tier
- **Getter:** `get_bronze_threshold(e: &Env) -> i128`
- **Setter:** `set_bronze_threshold(e: &Env, admin: &Address, value: i128)`

#### Silver Tier Threshold

- **Parameter:** `silver_threshold`
- **Type:** `i128`
- **Unit:** Token units
- **Default:** 1,000,000,000 (1,000 tokens with 6 decimals)
- **Minimum:** 100,000,000 (must be >= bronze threshold)
- **Maximum:** 10,000,000,000,000 (10 million tokens with 6 decimals)
- **Description:** Minimum bonded amount to achieve Silver tier
- **Getter:** `get_silver_threshold(e: &Env) -> i128`
- **Setter:** `set_silver_threshold(e: &Env, admin: &Address, value: i128)`

#### Gold Tier Threshold

- **Parameter:** `gold_threshold`
- **Type:** `i128`
- **Unit:** Token units
- **Default:** 10,000,000,000 (10,000 tokens with 6 decimals)
- **Minimum:** 1,000,000,000 (must be >= silver threshold)
- **Maximum:** 100,000,000,000,000 (100 million tokens with 6 decimals)
- **Description:** Minimum bonded amount to achieve Gold tier
- **Getter:** `get_gold_threshold(e: &Env) -> i128`
- **Setter:** `set_gold_threshold(e: &Env, admin: &Address, value: i128)`

#### Platinum Tier Threshold

- **Parameter:** `platinum_threshold`
- **Type:** `i128`
- **Unit:** Token units
- **Default:** 100,000,000,000 (100,000 tokens with 6 decimals)
- **Minimum:** 10,000,000,000 (must be >= gold threshold)
- **Maximum:** 1,000,000,000,000,000 (1 billion tokens with 6 decimals)
- **Description:** Minimum bonded amount to achieve Platinum tier
- **Getter:** `get_platinum_threshold(e: &Env) -> i128`
- **Setter:** `set_platinum_threshold(e: &Env, admin: &Address, value: i128)`

## Parameter Change Events

All successful parameter updates emit a `parameter_changed` event for off-chain tracking and auditing.

### Event Structure

**Event Topic:** `parameter_changed`

**Event Data Fields:**
1. `parameter` (String) - Name of the parameter that changed
2. `old_value` (i128) - Previous value (normalized to i128 for consistency)
3. `new_value` (i128) - New value (normalized to i128)
4. `updated_by` (Address) - Address that performed the update (governance address)
5. `timestamp` (u64) - Ledger timestamp when the update occurred

### Event Example

```rust
e.events().publish(
    (Symbol::new(e, "parameter_changed"),),
    (
        String::from_str(e, "protocol_fee_bps"),
        50_i128,  // old_value
        100_i128, // new_value
        admin.clone(),
        1234567890_u64, // timestamp
    ),
);
```

## Governance Update Flow

### Example: Updating Protocol Fee Rate

1. **Governance Proposal:** Governance proposes to increase protocol fee from 0.5% to 1%
   - Current: 50 bps
   - Proposed: 100 bps

2. **Validation:** Proposal is validated against bounds
   - Check: 100 bps is within [0, 1000] bps ✓

3. **Governance Vote:** Governance votes and approves the proposal

4. **Execution:** Admin executes the parameter update
   ```rust
   use credence_bond::parameters;
   
   // Admin authenticates
   admin.require_auth();
   
   // Update parameter
   parameters::set_protocol_fee_bps(&e, &admin, 100);
   ```

5. **Event Emission:** Contract emits `parameter_changed` event
   ```
   Event: parameter_changed
   - parameter: "protocol_fee_bps"
   - old_value: 50
   - new_value: 100
   - updated_by: <admin_address>
   - timestamp: <current_ledger_time>
   ```

6. **Verification:** Off-chain systems detect event and update their state

### Example: Updating Multiple Parameters

```rust
use credence_bond::parameters;

// Admin authenticates once
admin.require_auth();

// Update fee rates
parameters::set_protocol_fee_bps(&e, &admin, 75);
parameters::set_attestation_fee_bps(&e, &admin, 15);

// Update cooldown periods
parameters::set_withdrawal_cooldown_secs(&e, &admin, 86400); // 1 day
parameters::set_slash_cooldown_secs(&e, &admin, 43200);      // 12 hours

// Update tier thresholds
parameters::set_bronze_threshold(&e, &admin, 200_000_000);   // 200 tokens
parameters::set_silver_threshold(&e, &admin, 2_000_000_000); // 2,000 tokens
```

Each update emits a separate `parameter_changed` event.

## Error Handling

### Authorization Errors

**Error:** `"not admin"`
- **Cause:** Caller is not the contract admin
- **Resolution:** Only the governance address can update parameters

**Error:** `"not initialized"`
- **Cause:** Contract has not been initialized
- **Resolution:** Initialize contract with `initialize(admin)` first

### Bounds Validation Errors

**Error:** `"protocol_fee_bps out of bounds"`
- **Cause:** Value < 0 or value > 1000
- **Resolution:** Use value within [0, 1000] bps range

**Error:** `"attestation_fee_bps out of bounds"`
- **Cause:** Value < 0 or value > 500
- **Resolution:** Use value within [0, 500] bps range

**Error:** `"withdrawal_cooldown_secs out of bounds"`
- **Cause:** Value < 0 or value > 2,592,000
- **Resolution:** Use value within [0, 2,592,000] seconds range

**Error:** `"slash_cooldown_secs out of bounds"`
- **Cause:** Value < 0 or value > 604,800
- **Resolution:** Use value within [0, 604,800] seconds range

**Error:** `"bronze_threshold out of bounds"`
- **Cause:** Value < 0 or value > 1,000,000,000,000
- **Resolution:** Use value within [0, 1,000,000,000,000] range

**Error:** `"silver_threshold out of bounds"`
- **Cause:** Value < 100,000,000 or value > 10,000,000,000,000
- **Resolution:** Use value within [100,000,000, 10,000,000,000,000] range

**Error:** `"gold_threshold out of bounds"`
- **Cause:** Value < 1,000,000,000 or value > 100,000,000,000,000
- **Resolution:** Use value within [1,000,000,000, 100,000,000,000,000] range

**Error:** `"platinum_threshold out of bounds"`
- **Cause:** Value < 10,000,000,000 or value > 1,000,000,000,000,000
- **Resolution:** Use value within [10,000,000,000, 1,000,000,000,000,000] range

## Best Practices

1. **Gradual Changes:** Make incremental parameter adjustments rather than large jumps
2. **Testing:** Test parameter changes on testnet before mainnet deployment
3. **Monitoring:** Monitor `parameter_changed` events to track governance actions
4. **Documentation:** Document rationale for parameter changes in governance proposals
5. **Bounds Awareness:** Always check min/max bounds before proposing changes
6. **Impact Analysis:** Analyze impact of parameter changes on existing bonds and operations

## Integration Examples

### Reading Parameters in Contract Logic

```rust
use crate::parameters;

pub fn calculate_fee(e: &Env, amount: i128) -> i128 {
    let fee_bps = parameters::get_protocol_fee_bps(e);
    (amount * fee_bps as i128) / 10_000
}

pub fn check_tier(e: &Env, bonded_amount: i128) -> BondTier {
    let platinum = parameters::get_platinum_threshold(e);
    let gold = parameters::get_gold_threshold(e);
    let silver = parameters::get_silver_threshold(e);
    let bronze = parameters::get_bronze_threshold(e);
    
    if bonded_amount >= platinum {
        BondTier::Platinum
    } else if bonded_amount >= gold {
        BondTier::Gold
    } else if bonded_amount >= silver {
        BondTier::Silver
    } else if bonded_amount >= bronze {
        BondTier::Bronze
    } else {
        BondTier::None
    }
}
```

### Off-Chain Event Monitoring

```javascript
// Example: Monitor parameter changes
contract.on('parameter_changed', (event) => {
  const { parameter, old_value, new_value, updated_by, timestamp } = event;
  
  console.log(`Parameter ${parameter} changed from ${old_value} to ${new_value}`);
  console.log(`Updated by: ${updated_by} at ${new Date(timestamp * 1000)}`);
  
  // Update local cache
  updateParameterCache(parameter, new_value);
  
  // Notify stakeholders
  notifyGovernanceAction(parameter, old_value, new_value);
});
```

## Security Considerations

1. **Admin Key Security:** The admin address has full control over parameters. Secure key management is critical.
2. **Bounds Enforcement:** All bounds are enforced at the contract level. Out-of-bounds values are rejected.
3. **No Silent Failures:** All validation errors panic with descriptive messages.
4. **Event Transparency:** All changes are publicly auditable via events.
5. **Immutable Bounds:** Min/max bounds are hardcoded and cannot be changed without contract upgrade.

## Future Enhancements

Potential future improvements to the parameters system:

1. **Time-Locked Updates:** Require a delay between proposal and execution
2. **Multi-Sig Governance:** Require multiple signatures for parameter changes
3. **Parameter Ranges:** Allow governance to adjust min/max bounds within safe limits
4. **Emergency Pause:** Add ability to pause parameter updates in emergencies
5. **Parameter History:** Store historical parameter values on-chain
