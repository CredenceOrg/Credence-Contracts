# Compounding Rate Math

**Audience**: Downstream Integrators / Protocol Contributors

This document explains how interest and penalty rates compound over time within the Credence smart contracts. All percentage calculations use fixed-point math in basis points (bps), where `10_000` represents `1.0x` (or 100%).

## The `Rate::try_compound` function

Our compounding logic is fixed-point and deterministic. We do not use floating-point math, `f64::powf`, or approximations like Taylor series.

The `credence_math` crate exposes two entry points:

```rust
pub const MAX_COMPOUND_PERIODS: u32 = 10_000;

pub fn try_compound(rate: u32, periods: u32) -> Result<i128, ContractError>;
pub fn compound(rate: u32, periods: u32) -> i128; // panicking compatibility wrapper
```

`try_compound` is the preferred API:

- `periods > MAX_COMPOUND_PERIODS` returns `ContractError::InvalidPeriodCount`
  before any multiplication runs. This is a denial-of-service guard: a caller
  passing `u32::MAX` cannot burn the instruction budget.
- `periods <= 500` uses the historical per-period truncation loop, so results
  for realistic accrual horizons are bit-for-bit identical to the old
  implementation.
- `periods > 500` uses exponentiation by squaring, reducing the work from
  `O(periods)` to `O(log periods)`.
- A multiplier that does not fit in `i128` returns `ContractError::Overflow`.

`compound` maps both errors to panics with the messages
`"compound period cap exceeded"` and `"compound overflow"`.

### Why the 500-period split?

The legacy loop truncates after every period. That sequence is not
associative, so a single-rounding power cannot reproduce it exactly for every
input. To avoid silently changing results for existing horizons, the legacy
path is retained through 500 periods (over a year of daily compounding) and a
differential test compares it against an independent loop oracle for every
period count `0..=500`. Longer horizons use squaring and can differ from the legacy loop,
particularly for small rates over long horizons.

### Concrete Example

Suppose a bond is subject to a daily penalty fee of 50 bps (0.5%). If the bond is in default for 3 days, what is the compounded multiplier?

- **Base Factor:** `10_000 + 50 = 10_050`
- **Period 1:** `10_000 * 10_050 / 10_000 = 10_050` (1.005x)
- **Period 2:** `10_050 * 10_050 / 10_000 = 10_100` (1.01x)
- **Period 3:** `10_100 * 10_050 / 10_000 = 10_150` (1.015x)

The final multiplier returned is `10_150`. If the original principal is 1,000 USDC, the new balance with the accumulated penalty is:
```rust
use credence_math::{mul_div_i128, Rounding};

let final_amount = mul_div_i128(1_000, 10_150, 10_000, Rounding::Down, "apply penalty");
// final_amount = 1,015 USDC
```

### Why Fixed-Point?

In Soroban WASM environments, we enforce `#![no_std]` and strictly reject floating-point math (`#![deny(clippy::float_arithmetic)]`) to ensure deterministic behavior and cross-platform reproducibility. Integer multiplication and division are exact and auditable.

## Using Compounding in a Contract

Here is an example of applying a compounded rate during a state transition, such as processing a late withdrawal:

```rust
use credence_errors::ContractError;
use credence_math::{mul_div_i128, Rate, Rounding};

pub fn apply_late_penalty(
    amount: i128,
    days_late: u32,
    penalty_bps: u32,
) -> Result<i128, ContractError> {
    // 1. Get the compounded multiplier (scaled by 10_000)
    let compounded_multiplier = Rate::try_compound(penalty_bps, days_late)?;

    // 2. Apply it to the principal amount
    Ok(mul_div_i128(
        amount,
        compounded_multiplier,
        10_000,
        Rounding::Down,
        "apply penalty",
    ))
}
```

When integrating downstream (e.g., a TS/JS indexer calculating expected balances), replicate the on-chain rounding exactly. For `periods <= 500`, truncate once per period. For `periods > 500`, replicate the squaring combine: start with `multiplier = 10_000` and `power = 10_000 + rate`, then for each bit of `periods` from least significant to most significant, multiply `multiplier` by `power` (dividing by 10_000 with `BigInt` floor division) when the bit is set, and square `power` the same way before moving to the next bit. Floating-point math `1.005^3` may differ in the lower bits from fixed-point integer math.
