# Bond Introspection API

Read-only entrypoints that give operators and indexers a snapshot of contract configuration, versioning, and per-identity bond state.

## Entrypoints

### `version(env) -> String`

Returns the constant version string for the contract (e.g., `"0.1.0"`). This is used for off-chain monitoring to identify which build is currently deployed.

**Auth:** none — no `require_auth` is called on this path.

## Motivation

Previously, reconstructing the full contract state required multiple separate calls and event replay. `describe_config` and `describe_bond` collapse that into one call each, with a stable return shape that is part of the public ABI.

## Entrypoints

### `describe_config(env) -> Option<BondConfigView>`

Returns all contract-level configuration in one call.

**Auth:** none — no `require_auth` is called on this path.  
**Returns:** `None` if the contract has not been initialized yet; otherwise `Some(BondConfigView)`.

```rust
pub struct BondConfigView {
    /// DataKey::Admin
    pub admin: Address,
    /// DataKey::EarlyExitConfig — None until set_early_exit_config is called
    pub early_exit_treasury: Option<Address>,
    /// DataKey::EarlyExitConfig — None until set_early_exit_config is called
    pub early_exit_penalty_bps: Option<u32>,
    /// DataKey::WeightConfig
    pub weight_multiplier_bps: u32,
    /// DataKey::WeightConfig
    pub weight_max: u32,
}
```

**Storage keys read:**

| Field | Storage key |
|---|---|
| `admin` | `DataKey::Admin` |
| `early_exit_treasury`, `early_exit_penalty_bps` | `DataKey::EarlyExitConfig` |
| `weight_multiplier_bps`, `weight_max` | `DataKey::WeightConfig` |

### `describe_bond(env, identity: Address) -> Option<BondStateView>`

Returns the bond state for `identity`, or `None` if no bond exists for that address.

**Auth:** none — no `require_auth` is called on this path.  
**Panics:** never (missing bond returns `None`).

```rust
pub struct BondStateView {
    /// DataKey::Bond
    pub identity: Address,
    /// DataKey::Bond
    pub bonded_amount: i128,
    /// DataKey::Bond
    pub slashed_amount: i128,
    /// Derived: bonded_amount.saturating_sub(slashed_amount)
    pub available_amount: i128,
    /// DataKey::Bond
    pub bond_start: u64,
    /// DataKey::Bond
    pub bond_duration: u64,
    /// DataKey::Bond
    pub active: bool,
    /// DataKey::Bond
    pub is_rolling: bool,
    /// DataKey::Bond — 0 means not requested
    pub withdrawal_requested_at: u64,
    /// DataKey::Bond
    pub notice_period_duration: u64,
    /// Derived from bonded_amount via tier thresholds
    pub tier: BondTier,
}
```

**Storage keys read:**

| Field | Storage key |
|---|---|
| All bond fields | `DataKey::Bond` |
| `tier` | derived — no extra storage read |

## Security notes

- Neither entrypoint calls `require_auth` or mutates any storage key.
- Both are safe to call from any context (indexers, dashboards, other contracts).
- `describe_config` returns `None` on uninitialized contracts so callers can safely inspect the contract without tripping a panic.
- `describe_bond` returns `Option::None` rather than panicking so callers can distinguish "bond absent" from "contract not initialized".

## Example usage (Soroban CLI)

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- describe_config

soroban contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- describe_bond \
  --identity <IDENTITY_ADDRESS>
```

## Tier thresholds (for reference)

| Tier | Minimum bonded amount |
|---|---|
| Bronze | < 1 000 |
| Silver | 1 000 – 4 999 |
| Gold | 5 000 – 19 999 |
| Platinum | ≥ 20 000 |
