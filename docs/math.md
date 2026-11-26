# Credence Math — Overflow-Safe Fixed-Point Helpers

Shared arithmetic for Credence contracts lives in the `credence_math` crate.
This document covers the **fixed-point (WAD)** helpers, percentage / basis-point
families, and the regression vectors that lock their behaviour for audits.

For the broader helper catalogue (checked primitives, slippage, rates, time),
see [`ARITHMETIC_HELPERS.md`](./ARITHMETIC_HELPERS.md) and
[`decimal-handling.md`](./decimal-handling.md).

## Why fixed-point?

Credence normalizes all internal token accounting to **18 decimal places**.
Multiplying two normalized amounts without a scale correction overflows the
true economic product by `10^18`. The WAD helpers apply that scale in one
overflow-safe `mul_div` step:

| Helper | Formula | Rounding / Error Handling |
|--------|---------|---------------------------|
| `checked_mul_wad(a, b)` | `(a * b) / WAD` | Toward zero; returns `Result<i128, ContractError>` (`Overflow`) |
| `checked_mul_wad_up(a, b)` | `(a * b) / WAD` | Away from zero; returns `Result<i128, ContractError>` (`Overflow`) |
| `checked_div_wad(a, b)` | `(a * WAD) / b` | Toward zero; returns `Result<i128, ContractError>` (`DivisionByZero` / `Overflow`) |
| `checked_div_wad_up(a, b)` | `(a * WAD) / b` | Away from zero; returns `Result<i128, ContractError>` (`DivisionByZero` / `Overflow`) |
| `mul_wad(a, b, _msg)` *(deprecated)* | `(a * b) / WAD` | Toward zero; fixed static panic on overflow |
| `mul_wad_up(a, b, _msg)` *(deprecated)* | `(a * b) / WAD` | Away from zero; fixed static panic on overflow |
| `div_wad(a, b, _msg)` *(deprecated)* | `(a * WAD) / b` | Toward zero; fixed static panic on zero denom or overflow |
| `div_wad_up(a, b, _msg)` *(deprecated)* | `(a * WAD) / b` | Away from zero; fixed static panic on zero denom or overflow |
| `sat_mul_wad(a, b)` | same as `mul_wad` | Saturates / never panics |
| `sat_div_wad(a, b)` | same as `div_wad` | Saturates; `b == 0 → 0` |

`WAD = 10^18 = 1_000_000_000_000_000_000`.

All checked and legacy WAD helpers widen the intermediate product to 256 bits via
`checked_mul_div_i128`, so `a * b` may exceed `i128::MAX` as long as the **final**
rounded quotient fits. Saturating siblings clamp to `i128::MIN` / `i128::MAX`.

For contract entrypoints with an `&Env` in scope, combine any `checked_*` helper
(`checked_add_i128`, `checked_sub_i128`, `checked_mul_i128`, `checked_div_i128`,
`checked_ceil_div_i128`, `checked_mul_div_i128`, `checked_mul_u64`,
`checked_mul_wad*`, `checked_div_wad*`) with `unwrap_or_panic_error(env, ...)` to
abort with a typed `ContractError` via `panic_with_error!`.

```rust
use credence_math::{checked_div_wad, checked_mul_wad, WAD};

assert_eq!(checked_mul_wad(WAD, WAD), Ok(WAD));       // 1.0 * 1.0 = 1.0
assert_eq!(checked_div_wad(2 * WAD, 2 * WAD), Ok(WAD)); // 2.0 / 2.0 = 1.0
assert_eq!(checked_mul_wad(3, WAD / 2), Ok(1));       // truncates 1.5 → 1
```

## Percentage and basis-point companions

| Helper | Scale | Notes |
|--------|-------|-------|
| `bps` / `bps_round_up` / `split_bps` | `/ 10_000` | Panicking family |
| `sat_mul_bps` / `sat_bps` / `sat_split_bps` | `/ 10_000` | Panic-free |
| `percent` / `percent_round_up` / `split_percent` | `/ 100` | Panicking; `_div_msg` is a no-op placeholder |
| `sat_percent` / `sat_mul_percent` / `sat_split_percent` | `/ 100` | Panic-free |

Constants: `BPS_DENOMINATOR = 10_000`, `PERCENT_DENOMINATOR = 100`.

## Regression vectors

`cargo test -p credence_math` exercises deterministic vectors covering:

1. **WAD identity / zero / half / remainder** — `fixed_point::tests::wad_regression_vectors`
2. **WAD saturation and zero-denominator** — `fixed_point::tests::sat_wad_clamps_and_zero_denom`
3. **Percent + sat helpers** — `tests::percent_and_sat_regression_vectors`
4. **Wide-intermediate mul_div / bps / slippage / floor_to_day** — existing unit suite
5. **Property: `mul_wad(x, WAD) == x`** and **`sat_mul_bps(x, 10_000) == x`**

| Scenario | Expected |
|----------|----------|
| Multiplication near overflow bounds | No panic when final result fits; saturate or panic only on final overflow |
| Division by zero | Panicking helpers panic with `msg`; `sat_*` returns `0` |
| Rounding boundaries | `Down` truncates toward zero; `Up` rounds away from zero |
| Regression vectors | All listed tests pass with the documented outputs |

## Module layout

```
contracts/credence_math/src/
├── lib.rs           # checked ops, bps/percent, sat_*, slippage, chunking
├── fixed_point.rs   # WAD helpers + regression vectors
├── rate.rs          # Rate::compound
├── time.rs          # SECONDS_PER_* constants
└── timestamp.rs     # Timestamp::floor_to_day / add_business_days
```

## Verification

```bash
cargo test -p credence_math
```
