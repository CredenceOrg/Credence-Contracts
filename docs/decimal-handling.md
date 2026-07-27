# Decimal Normalization & Precision Guidelines

## Internal Accounting

The Credence protocol uses a **Fixed 18-Decimal Precision** for all internal
accounting. This ensures that yield calculations, slashing penalties, and tier
thresholds remain consistent regardless of the underlying collateral token.

## Supported Decimal Range

| Property              | Value  |
|-----------------------|--------|
| Minimum decimals      | 0      |
| Maximum decimals      | 18     |
| Normalized precision  | 18     |

Tokens with decimals outside the range [0, 18] are **rejected** by
`validate_supported_decimals` with `ContractError::UnsupportedDecimals`.
The 18-decimal ceiling guarantees that the scaling factor `10^(18 - token_decimals)`
fits comfortably in `i128` and that normalized amounts cannot overflow during
subsequent multiply/divide operations.

## Normalization Process

### Ingress — `normalize(env, token, native_amount)`

Scales a native token amount **up** to 18-decimal precision:

| Token decimals | Formula                         | Example (1 token)              |
|---------------|---------------------------------|--------------------------------|
| 0             | `amount * 10^18`                | `1 → 1_000_000_000_000_000_000` |
| 6             | `amount * 10^12`                | `1_000_000 → 10^18`             |
| 8             | `amount * 10^10`                | `100_000_000 → 10^18`           |
| 18            | `amount * 1` (no-op)            | `10^18 → 10^18`                 |

Normalize is **always exact** — multiplying up cannot lose precision. If the
scaled result would exceed `i128::MAX` the operation panics.

### Egress — `denormalize(env, token, normalized_amount)`

Scales a 18-decimal amount **down** to native token precision:

| Token decimals | Formula                          | Example (10^18 normalized)     |
|---------------|----------------------------------|--------------------------------|
| 0             | `amount / 10^18`                 | `10^18 → 1`                    |
| 6             | `amount / 10^12`                 | `10^18 → 1_000_000`            |
| 8             | `amount / 10^10`                 | `10^18 → 100_000_000`          |
| 18            | `amount / 1` (no-op)             | `10^18 → 10^18`                |

### Rounding Mode — `denormalize_with_rounding(env, token, amount, rounding)`

- **`Rounding::Down`** (default) — truncates the fractional remainder toward
  zero. Matches the behavior of `denormalize()` and standard integer division.
- **`Rounding::Up`** — rounds away from zero when the division leaves any
  remainder. Useful for protocols that must collect at least the specified amount.

### Truncation Hazard

When denormalizing to a token with fewer decimals than the normalized
representation, the fractional part is **truncated** (discarded under
`Rounding::Down`).

**Example**: A normalized amount of `999_999_999_999` (less than one
6-decimal unit) denormalizing to a 6-decimal USDC token yields **0** because
`999_999_999_999 / 10^12 = 0`.

Use the helper functions to detect truncation before it happens:

| Helper                         | Returns `true` when ...                            |
|-------------------------------|---------------------------------------------------|
| `would_denormalize_to_zero`   | the amount is smaller than the smallest native unit |
| `can_denormalize_exactly`     | the amount can be represented without truncation    |
| `can_normalize_safely`        | the amount will not overflow on normalize           |

### Roundtrip Invariant

For any supported decimal and any amount that does not overflow,
`denormalize(env, token, normalize(env, token, amount)) == amount`.

## Overflow Boundaries

| Token decimals | Max safe native amount (before normalize overflow) |
|---------------|-----------------------------------------------------|
| 0             | `i128::MAX / 10^18 ≈ 1.7 × 10^20`                  |
| 6             | `i128::MAX / 10^12 ≈ 1.7 × 10^26`                  |
| 8             | `i128::MAX / 10^10 ≈ 1.7 × 10^28`                  |
| 18            | `i128::MAX` (no scaling)                            |

Use `can_normalize_safely` for a runtime pre-check before calling `normalize`.

## Design Decisions

1. **Scaling always via multiplier** — Tokens with >18 decimals are unsupported
   (rejected at ingress). The normalization layer only multiplies up, never
   divides on ingress. This eliminates complexity around dual-mode (mul vs div)
   scaling.

2. **Symbol validation is separate** — `require_non_zero_currency` checks token
   symbols independently from decimal validation. `validate_supported_decimals`
   returns only the decimal count; use `validate_supported_decimals_and_symbol`
   when both checks are needed.

3. **Cached decimal reads** — `validate_supported_decimals` caches the token
   `decimals()` call so calling `normalize` or `denormalize` reads the decimals
   exactly once, avoiding redundant host calls.

4. **Zero-amount short-circuit** — Both `normalize` and `denormalize` return
   `0` immediately for zero input, avoiding unnecessary division (and the
   attendant truncation semantics) for the zero case.

5. **Scale=1 short-circuit** — For 18-decimal tokens the scale factor is 1 and
   both `normalize` and `denormalize` return the input unchanged, avoiding a
   redundant multiply or divide.

## Basis-Point Chains

Use `credence_math::mul_div_i128(a, b, denom, mode, msg)` when a percentage,
fee, penalty, or pro-rata formula would otherwise multiply and divide in
multiple steps. The helper widens the intermediate product to 256 bits before
division, so `a * b` can exceed `i128::MAX` as long as the final rounded result
still fits in `i128`.

- Use `Rounding::Down` for back-compatible truncation toward zero. This matches
  the legacy `bps(amount, bps, ..)` result.
- Use `Rounding::Up` when the protocol must collect at least the fractional fee
  or penalty amount.
- Use `Rounding::Nearest` when symmetric nearest-integer behavior is desired;
  half-way cases round away from zero.

`bps` and `split_bps` keep their original multiply-then-divide behavior for
compatibility. New multi-step formulas should prefer one `mul_div_i128` call per
logical ratio, or `bps_round_up` when basis-point math should round away from
zero.

## Percentage Chains

Next to the basis-point family, the same overflow-safe 256-bit intermediate is
exposed for plain `%` math under a denominator of 100:

- `credence_math::percent(amount, percentage, mul_msg, _div_msg)` —
  `(amount * percentage) / 100` for `i128`. Panics with `mul_msg` when the
  result would not fit in `i128`.
- `credence_math::percent_u64(amount, percentage, mul_msg)` — same formula for `u64`.
- `credence_math::percent_round_up(amount, percentage, msg)` — same shape,
  rounds away from zero.
- `credence_math::split_percent(amount, percentage, mul_msg, div_msg, sub_msg)` —
  returns `(fee, net)` where `fee = percent(amount, percentage, mul_msg, div_msg)`
  and `net = amount - fee`.

The denominators are exposed as constants to keep the math reviews honest:
`pub const BPS_DENOMINATOR: i128 = 10_000;` and
`pub const PERCENT_DENOMINATOR: i128 = 100;` (both in `credence_math`).

## Saturating Helpers (`sat_*`)

For paths that must never revert on an internal overflow (UI/aggregation,
off-chain scoring, dashboard roll-ups), use the **saturating** family. They use
a 256-bit intermediate, **clamp** to `i128::MIN` / `i128::MAX` on overflow, and
**silently return `0`** when the denominator is zero (where the panicking
siblings `mul_div_i128`, `bps`, and `percent` would raise a
transaction-reverting panic).

- `credence_math::sat_mul_div_i128(a, b, denom, mode)` — the panic-free sibling
  of `mul_div_i128`.
- `credence_math::sat_mul_bps(...)` / `credence_math::sat_bps(...)` (alias).
- `credence_math::sat_mul_bps_u64(...)` / `credence_math::sat_bps_u64(...)` (alias).
- `credence_math::sat_bps_round_up(...)`, `credence_math::sat_split_bps(...)`.
- `credence_math::sat_percent(...)` / `credence_math::sat_mul_percent(...)` (alias).
- `credence_math::sat_percent_u64(...)` / `credence_math::sat_mul_percent_u64(...)` (alias).
- `credence_math::sat_percent_round_up(...)`, `credence_math::sat_split_percent(...)`.

These are **append-only additions** — every pre-existing helper
(`mul_div_i128`, `bps`, `bps_round_up`, `bps_u64`, `split_bps`, `percent`,
`percent_u64`, `percent_round_up`, `split_percent`) keeps its full signature
and panic contract for backward compatibility. New code that does not need the
panic should reach for the `sat_*` family directly.

### Migration notes

- The trailing `_div_msg` parameter on `percent(...)` and the `div_msg`
  parameter on `split_percent(...)` are **intentionally retained** as no-op
  forward-compat placeholders so existing call sites keep compiling. Do not drop
  them from call sites.
- The new helpers do not introduce a default rounding mode; callers must pass an
  explicit `Rounding::Down` (truncate toward zero), `Rounding::Up` (round away
  from zero), or `Rounding::Nearest` (half-ties round away from zero) to
  `sat_mul_div_i128` and stop relying on the implicit behavior of
  `amount * bps / 10_000`.