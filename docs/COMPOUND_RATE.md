# Compound Rate Math

This document explains how we handle compounding in fee and interest math across the Credence contracts, and is intended primarily for **contributors** implementing or auditing financial math within the protocol.

## Background

In traditional finance, interest and fees are often compounded on a monthly or daily basis using complex exponentials (e.g. `P(1 + r/n)^(nt)`). In Soroban smart contracts, floating-point operations and complex exponentials are too expensive computationally (WASM instructions) and can introduce rounding inconsistencies across nodes. 

Instead of true compound interest, we rely on a **per-second simple interest** model applied linearly over the elapsed time, which functions as a continuous-ish approximation of compounding without the overhead.

## How it works

When a user interacts with a contract that accrues interest (e.g., when withdrawing or updating a debt position), the contract calculates the interest by multiplying the principal by a fixed per-second rate and the elapsed time since the last accrual.

### Concrete Example

Instead of abstract formulas, here is a concrete example of how you should calculate interest in a contract (such as a treasury or lending pool), using our checked math helpers.

```rust
use soroban_sdk::{Env, panic_with_error};
use credence_math::checked_mul_i128;

// Assume these values are retrieved from contract storage
let principal: i128 = 1_000_000_0000000; // 1M tokens (with 7 decimals)
let rate_per_second: i128 = 1585489; // Corresponds to ~5% APY scaled
let elapsed_seconds: i128 = 86400; // 1 day

// 1. Calculate the interest for one second
let interest_per_sec = checked_mul_i128(
    principal, 
    rate_per_second, 
    "interest mul overflow"
);

// 2. Multiply by the elapsed time
let total_interest = checked_mul_i128(
    interest_per_sec, 
    elapsed_seconds, 
    "time mul overflow"
);

// 3. (Optional) apply scaling divisors if the rate is stored with extra precision
let scaled_interest = total_interest / 1_000_000_000;
```

## Why this approach?

1. **Gas efficiency:** `checked_mul_i128` compiles down to cheap integer multiplications.
2. **Determinism:** Integer math is fully deterministic on all validators. 
3. **Simplicity:** Contributors don't need to reason about Taylor series approximations for `e^x`.

## When do we actually compound?

Because we use a linear approximation, interest does not technically "compound" (earn interest on interest) *until* a state transition explicitly adds the accrued interest to the principal in storage. 

If a user wants their earned yield to start generating its own yield, they must call an entrypoint that forces an accrual and rolls the interest into the principal (e.g., a `compound()` or `roll_yield()` function). This places the gas cost of compounding directly on the user who benefits from it.
