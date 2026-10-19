# Arithmetic Helpers — `credence_math`

## Philosophy

Every arithmetic operation in Credence contracts goes through a checked helper
from the `credence_math` crate. There are three families of helpers, each
serving a different call-site need:

| Family | Overflow behaviour | Denominator = 0 | Use case |
|--------|-------------------|-----------------|----------|
| **Panic-message** (`add_i128`, `mul_i128`, `div_i128`, ...) | Panics with a `&'static str` message | Panics with the same message | Hot paths where failure is a programming error |
| **Typed-error** (`div_checked_i128`, `ceil_div_checked_i128`, `checked_add_or_error`, ...) | Returns `Err(ContractError::Overflow)` | Returns `Err(ContractError::DivisionByZero)` | Paths where a zero denom or overflow is reachable at runtime (e.g. fully-slashed bond) |
| **Saturating** (`sat_mul_div_i128`, `sat_mul_bps`, ...) | Clamps to `i128::MIN` / `i128::MAX` | Silently returns `0` | Non-critical UX/aggregation paths that must never revert |

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `BPS_DENOMINATOR` | `10_000` | Basis-point full scale (100% = 10_000 bps) |
| `PERCENT_DENOMINATOR` | `100` | Percentage full scale (100% = 100) |

## Rounding Modes

```rust
enum Rounding { Down, Up, Nearest }
```

| Variant | Behaviour |
|---------|-----------|
| `Down` | Truncate the fractional remainder toward zero (matches Rust integer division) |
| `Up` | Round away from zero when the division leaves any remainder |
| `Nearest` | Round to the nearest integer; exact half-way cases round away from zero |

`Nearest` is **not** banker's rounding (round-half-to-even). It uses
round-half-away-from-zero so the behaviour is deterministic and
cross-platform.

## Checked Primitives (Panic-Message Family)

These helpers accept a `&'static str` message that is used as the panic
payload when the operation fails. The message is **not** sent to the
user — it is a developer-facing diagnostic. Choose a message that will
make sense in a stack trace or Sentry event.

| Helper | Signature | Fails when |
|--------|-----------|------------|
| `mul_u64` | `(a: u64, b: u64, msg) -> u64` | `a * b` exceeds `u64::MAX` |
| `add_i128` | `(a: i128, b: i128, msg) -> i128` | `a + b` exceeds `i128::MIN` / `i128::MAX` |
| `sub_i128` | `(a: i128, b: i128, msg) -> i128` | `a - b` exceeds `i128::MIN` / `i128::MAX` |
| `mul_i128` | `(a: i128, b: i128, msg) -> i128` | `a * b` exceeds `i128::MIN` / `i128::MAX` |
| `div_i128` | `(a: i128, b: i128, msg) -> i128` | `b == 0` or `i128::MIN / -1` |
| `ceil_div_i128` | `(a: i128, b: i128, msg) -> i128` | `b == 0` or `a + (b - 1)` overflows |

**When to use**: Use these on hot paths where the inputs have already been
validated or are guaranteed by construction not to fail. When they do fail
it represents a programming error that should be caught in testing.

**When NOT to use**: Do NOT use these when a failing condition is a
reachable runtime state (e.g. `b` comes from user-supplied data). Use a
typed-error variant instead.

## Checked Primitives (Typed-Error Family)

These helpers return `Result<i128, ContractError>` so callers can handle
failure with standard Rust error propagation (`?`). They are the preferred
choice for contract entrypoints where the failure should map to a
wire-stable error code.

| Helper | Signature | `Err` on overflow | `Err` on zero denom |
|--------|-----------|-------------------|---------------------|
| `div_checked_i128` | `(a: i128, b: i128) -> Result<i128, ContractError>` | `Overflow` | `DivisionByZero` |
| `ceil_div_checked_i128` | `(a: i128, b: i128) -> Result<i128, ContractError>` | `Overflow` | `DivisionByZero` |
| `checked_add_or_error` | `(a: i128, b: i128) -> Result<i128, ContractError>` | `Overflow` | N/A |

**When to use**: Use these on paths where failure is triggered by untrusted
input (user-supplied amounts, slash percentages from bond state, etc.) so
the caller receives a structured `ContractError` rather than a panic string.

## Wide-Intermediate Multiply-Divide

### `mul_div_i128`

```rust
pub fn mul_div_i128(a: i128, b: i128, denom: i128, mode: Rounding, msg: &'static str) -> i128
```

Computes `a * b / denom` using a 256-bit intermediate (`U256`) so the
intermediate product `a * b` can safely exceed `i128::MAX`. The result is
rounded according to `mode`.

**Panics when**:
- `denom == 0` (with `msg`)
- The final rounded result does not fit in `i128` (with `msg`)

**Use for**: Any multi-step percentage, fee, penalty, or pro-rata formula
that would otherwise require an intermediate product that could overflow
`i128`.

### `sat_mul_div_i128`

```rust
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, mode: Rounding) -> i128
```

Saturating sibling of `mul_div_i128`. Instead of panicking:
- Clamps to `i128::MAX` on positive overflow
- Clamps to `i128::MIN` on negative overflow
- Returns `0` when `denom == 0`

**Use for**: Non-critical UX/aggregation paths that must never revert the
transaction.

## Basis-Point Helpers

| Helper | Rounding | Panics on overflow | Uses wide intermediate |
|--------|----------|-------------------|----------------------|
| `bps` | Down (truncate) | Yes (via inner `mul_i128`/`div_i128`) | No — `amount * bps` must fit in `i128` |
| `bps_round_up` | Up | Yes (via `mul_div_i128`) | Yes |
| `bps_u64` | Down (truncate) | Yes (via `mul_u64`) | No — `amount * bps` must fit in `u64` |
| `split_bps` | Down (truncate) | Yes (via `bps` + `sub_i128`) | No |
| `sat_mul_bps` | Down (truncate) | No — saturates | Yes |

### `bps`

```rust
pub fn bps(amount: i128, bps: u32, mul_msg: &'static str, div_msg: &'static str) -> i128
```

Computes `amount * bps / 10_000`. The multiply and divide are separate
checked steps — the intermediate `amount * bps` must not exceed `i128`.
For amounts that could overflow, use `mul_div_i128` with `Rounding::Down`
or `sat_mul_bps` instead.

### `bps_round_up`

```rust
pub fn bps_round_up(amount: i128, bps_value: u32, msg: &'static str) -> i128
```

Rounds away from zero when the division leaves a remainder. Uses
`mul_div_i128` internally so the intermediate product cannot overflow.

### `split_bps`

```rust
pub fn split_bps(
    amount: i128,
    bps_value: u32,
    mul_msg: &'static str,
    div_msg: &'static str,
    sub_msg: &'static str,
) -> (i128, i128)
```

Returns `(fee, net)` where `fee = bps(amount, bps_value, ...)` and
`net = amount - fee`. Panics if `fee > amount`.

## Rate Helpers

### `Rate::compound`

```rust
pub fn compound(rate: u32, periods: u32) -> i128
```

Computes the compounded multiplier over `periods` periods at the given
`rate` (in basis points). Uses iterative multiplication via
`mul_div_i128` with `Rounding::Down`.

The result is scaled by `BPS_DENOMINATOR` (10_000), so a return value of
`10_000` represents 1.0x (no growth).

**Assumptions**:
- The rate is in basis points (e.g. 500 = 5% per period)
- All periods compound discretely (not continuously)
- The multiplier is rounded down at each compounding step

## Timestamp Helpers

### `floor_to_day`

```rust
pub fn floor_to_day(ts: u64) -> u64
```

Floors a Unix timestamp (seconds since epoch) to the start of its UTC day
(midnight 00:00:00). Equivalent to `(ts / 86_400) * 86_400`.

**Properties**:
- Idempotent: `floor_to_day(floor_to_day(ts)) == floor_to_day(ts)`
- Monotone: `a <= b` implies `floor_to_day(a) <= floor_to_day(b)`
- Result is always a multiple of 86_400

### `Timestamp::add_business_days`

```rust
pub fn add_business_days(t: u64, n: u64) -> u64
```

Adds `n` business days to timestamp `t`, skipping weekends (Saturday and
Sunday). The time-of-day component is preserved exactly.

**Assumptions**:
- Business week: Monday through Friday
- Weekends: Saturday and Sunday are skipped
- No public holidays are accounted for
- Epoch (1970-01-01) is a Thursday

## Slippage Check

```rust
pub fn slippage_bps_check(
    requested: i128,
    actual: i128,
    max_slippage_bps: u32,
) -> Result<(), ContractError>
```

Returns `Ok(())` if the absolute difference between `requested` and
`actual` is within `max_slippage_bps` basis points of `requested`.

Returns `Err(ContractError::SlippageExceeded)` when:
- `max_slippage_bps == 0` and `requested != actual`
- `requested == 0` and `actual != 0`
- The proportional difference exceeds `max_slippage_bps`

The comparison uses a 256-bit intermediate to avoid overflow.

## Validation Helpers

### `require_valid_percent_split`

```rust
pub fn require_valid_percent_split(splits: &Vec<u32>) -> Result<(), ContractError>
```

Validates that an array of percentage splits sums to exactly
`BPS_DENOMINATOR` (10_000). Returns:
- `Ok(())` when the sum is exactly 10_000
- `Err(ContractError::Overflow)` if the sum exceeds `u32::MAX`
- `Err(ContractError::InvalidPercentSplit)` if the sum is not 10_000

### `chunked_iter`

```rust
pub fn chunked_iter<T, F>(
    e: &Env,
    items: &Vec<T>,
    chunk_size: u32,
    f: F,
) -> u32
```

Splits a `Vec<T>` into chunks of `chunk_size` and invokes `f` for each
chunk. Panics with `ContractError::DivisionByZero` when `chunk_size == 0`.
Returns the number of chunks produced.

## Summary: Choosing the Right Helper

| You want to... | Use this helper |
|---|---|
| Add two `i128` values, panic on overflow | `add_i128` |
| Add two `i128` values, return a typed error on overflow | `checked_add_or_error` |
| Subtract two `i128` values, panic on underflow | `sub_i128` |
| Multiply two `i128` values, panic on overflow | `mul_i128` |
| Divide two `i128` values, panic on zero denom | `div_i128` |
| Divide two `i128` values, return a typed error on zero denom | `div_checked_i128` |
| Compute `ceil(a / b)` with panic | `ceil_div_i128` |
| Compute `ceil(a / b)` with typed error | `ceil_div_checked_i128` |
| Compute `a * bps / 10_000` | `bps` (or `mul_div_i128` for wide intermediate) |
| Compute `a * bps / 10_000`, round up | `bps_round_up` |
| Split amount into `(fee, net)` by bps | `split_bps` |
| Compute `a * b / denom` safely | `mul_div_i128` |
| Compute `a * b / denom`, never panic | `sat_mul_div_i128` |
| Check if `actual` is within bps tolerance of `requested` | `slippage_bps_check` |
| Compound interest over `n` periods | `Rate::compound` |
| Floor a timestamp to its UTC midnight | `floor_to_day` |
| Add business days to a timestamp | `Timestamp::add_business_days` |
| Validate percent splits sum to 10_000 bps | `require_valid_percent_split` |
| Split a vec into batches | `chunked_iter` |
