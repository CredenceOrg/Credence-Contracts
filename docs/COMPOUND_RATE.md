# Compounding Rate Math

**Audience**: Downstream Integrators / Protocol Contributors

This document explains how interest and penalty rates compound over time within the Credence smart contracts. All percentage calculations use fixed-point math in basis points (bps), where `10_000` represents `1.0x` (or 100%).

## The `Rate::compound` function

Our compounding logic relies on iterative multiplication using `mul_div_i128`. We do not use floating-point math, exponentiation (`f64::powf`), or approximations like Taylor series. 

Instead, the `compound(rate: u32, periods: u32) -> i128` function in the `credence_math` crate computes the exact compounded multiplier over `n` periods iteratively.

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

### Why Iterative Compounding?

In Soroban WASM environments, we enforce `#![no_std]` and strictly reject floating-point math (`#![deny(clippy::float_arithmetic)]`) to ensure deterministic behavior and cross-platform reproducibility. 

While iterative loops consume CPU instructions linearly `O(n)`, typical `periods` in our models represent a bounded number of days or epochs (e.g. `<= 365`). The operation is constrained enough to fit comfortably within the CPU instruction budget without hitting the `env.cost_estimate()` limits. We trade execution cycles for absolute precision and safety.

## Using Compounding in a Contract

Here is an example of applying a compounded rate during a state transition, such as processing a late withdrawal:

```rust
use credence_math::{Rate, mul_div_i128, Rounding};

pub fn apply_late_penalty(amount: i128, days_late: u32, penalty_bps: u32) -> i128 {
    // 1. Get the compounded multiplier (scaled by 10_000)
    let compounded_multiplier = Rate::compound(penalty_bps, days_late);
    
    // 2. Apply it to the principal amount
    mul_div_i128(amount, compounded_multiplier, 10_000, Rounding::Down, "apply penalty")
}
```

When integrating downstream (e.g., a TS/JS indexer calculating expected balances), you must replicate this exact iterative truncation. Floating-point math `1.005^3` may differ in the lower bits from our iterative fixed-point integer math. Use `BigInt` iteratively off-chain to predict the exact on-chain outcome.
