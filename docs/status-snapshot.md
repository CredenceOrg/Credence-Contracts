# Bond Status Snapshot Helper

## Overview

`get_bond_status_snapshot()` is a read-only contract method that returns a stable, flat struct describing the current state of a bond. It is designed for backend ingestion: one call, no joins, deterministic schema.

## Return Type: `BondStatusSnapshot`

```rust
pub struct BondStatusSnapshot {
    /// Current tier derived from bonded_amount.
    pub tier: BondTier,
    /// Seconds remaining in the cooldown window.
    /// 0 if no withdrawal request is pending or the cooldown has elapsed.
    pub cooldown_remaining_secs: u64,
    /// Whether emergency mode is currently enabled.
    pub emergency_mode: bool,
    /// Net available balance (bonded_amount − slashed_amount).
    pub available_balance: i128,
    /// Ledger timestamp at which the snapshot was taken.
    pub snapshot_timestamp: u64,
}
```

### `tier: BondTier`

Derived from `bonded_amount` at call time using the same thresholds as `get_tier()`.

| Tier     | bonded_amount range (6-decimal units) |
|----------|---------------------------------------|
| Bronze   | < 1,000,000,000 (< 1 000 USDC)        |
| Silver   | 1,000,000,000 – 4,999,999,999         |
| Gold     | 5,000,000,000 – 19,999,999,999        |
| Platinum | ≥ 20,000,000,000 (≥ 20 000 USDC)      |

### `cooldown_remaining_secs: u64`

Seconds until the cooldown window closes. Computed as:

```
end = withdrawal_requested_at + cooldown_period
remaining = max(0, end − now)
```

Returns `0` when:
- No withdrawal request has been made (`withdrawal_requested_at == 0`)
- The cooldown period is not configured (`cooldown_period == 0`)
- The cooldown window has already elapsed (`now >= end`)

### `emergency_mode: bool`

Reflects `EmergencyConfig.enabled`. Returns `false` if emergency config has never been set.

### `available_balance: i128`

```
available_balance = bonded_amount − slashed_amount
```

This is the maximum amount the bond holder can withdraw. Always ≥ 0.

### `snapshot_timestamp: u64`

The ledger timestamp at the moment the snapshot was taken. Backends should store this alongside the snapshot to detect staleness.

## Contract Method

```rust
pub fn get_bond_status_snapshot(e: Env) -> BondStatusSnapshot
```

**Read-only.** Does not modify any contract state.

**Panics:**
- `"no bond"` — no bond has been created in this contract instance yet.

## Usage Example

```rust
let snap = contract.get_bond_status_snapshot();

println!("Tier:              {:?}", snap.tier);
println!("Cooldown left:     {} s", snap.cooldown_remaining_secs);
println!("Emergency mode:    {}", snap.emergency_mode);
println!("Available balance: {}", snap.available_balance);
println!("Snapshot at:       {}", snap.snapshot_timestamp);
```

## Backend Integration Notes

- Call this endpoint on every reputation-engine tick to get a consistent view of bond health.
- `snapshot_timestamp` lets you detect if a cached snapshot is stale.
- `available_balance` is the authoritative figure for withdrawal eligibility checks; do not recompute it from raw bond fields.
- `emergency_mode: true` should suppress normal reputation scoring and trigger an alert.

## Related

- [Slashing](slashing.md)
- [Tier System](tier-system.md)
- [Emergency](emergency.md)
- Source: `contracts/credence_bond/src/status_snapshot.rs`
- Tests: `contracts/credence_bond/src/test_status_snapshot.rs`
