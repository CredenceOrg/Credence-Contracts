# Decimal Normalization & Precision Guidelines

## Internal Accounting

The Credence protocol uses a **Fixed 18-Decimal Precision** for all internal accounting. This ensures that yield calculations, slashing penalties, and tier thresholds remain consistent regardless of the underlying collateral token.

## Normalization Process

1. **Inbound (normalize):** When a user creates a bond, the native token amount is scaled UP to 18 decimals.
   - _Formula:_ `amount * 10^(18 - token_decimals)`
2. **Outbound (denormalize):** When a user withdraws, the internal 18-decimal amount is scaled DOWN to the token's native precision.
   - _Formula:_ `amount / 10^(18 - token_decimals)`

## Limitations

- **Maximum Decimals:** The protocol strictly supports tokens with up to 18 decimals. Tokens exceeding this (e.g., 24 or 36 decimals) will be rejected by the normalization layer to prevent arithmetic overflow in the 18-decimal accounting space.
- **Truncation:** Small amounts that cannot be represented in the native token's precision (e.g., 0.0000001 of an 18-decimal internal amount being withdrawn to a 6-decimal USDC token) will be truncated.

## Basis-Point Chains

Use `credence_math::mul_div_i128(a, b, denom, mode, msg)` when a percentage, fee, penalty, or pro-rata formula would otherwise multiply and divide in multiple steps. The helper widens the intermediate product to 256 bits before division, so `a * b` can exceed `i128::MAX` as long as the final rounded result still fits in `i128`.

- Use `Rounding::Down` for back-compatible truncation toward zero. This matches the legacy `bps(amount, bps, ..)` result.
- Use `Rounding::Up` when the protocol must collect at least the fractional fee or penalty amount.
- Use `Rounding::Nearest` when symmetric nearest-integer behavior is desired; half-way cases round away from zero.

`bps` and `split_bps` keep their original multiply-then-divide behavior for compatibility. New multi-step formulas should prefer one `mul_div_i128` call per logical ratio, or `bps_round_up` when basis-point math should round away from zero.

## Percentage Chains

Next to the basis-point family, the same overflow-safe 256-bit intermediate is exposed for plain `%` math under a denominator of 100:

- `credence_math::percent(amount, percentage, mul_msg, _div_msg)` — `(amount * percentage) / 100` for `i128`. Panics with `mul_msg` when the result would not fit in `i128`.
- `credence_math::percent_u64(amount, percentage, mul_msg)` — same formula for `u64`.
- `credence_math::percent_round_up(amount, percentage, msg)` — same shape, rounds away from zero.
- `credence_math::split_percent(amount, percentage, mul_msg, div_msg, sub_msg)` — returns `(fee, net)` where `fee = percent(amount, percentage, mul_msg, div_msg)` and `net = amount - fee`.

The denominators are exposed as constants to keep the math reviews honest: `pub const BPS_DENOMINATOR: i128 = 10_000;` and `pub const PERCENT_DENOMINATOR: i128 = 100;` (both in `credence_math`).

## Saturating Helpers (`sat_*`)

For paths that must never revert on an internal overflow (UI/aggregation, off-chain scoring, dashboard roll-ups), use the new **saturating** family. They use a 256-bit intermediate, **clamp** to `i128::MIN` / `i128::MAX` on overflow, and **silently return `0`** when the denominator is zero (where the panicking siblings `mul_div_i128`, `bps`, and `percent` would raise a transaction-reverting panic).

- `credence_math::sat_mul_div_i128(a, b, denom, mode)` — the panic-free sibling of `mul_div_i128`.
- `credence_math::sat_mul_bps(...)` / `credence_math::sat_bps(...)` (alias).
- `credence_math::sat_mul_bps_u64(...)` / `credence_math::sat_bps_u64(...)` (alias).
- `credence_math::sat_bps_round_up(...)`, `credence_math::sat_split_bps(...)`.
- `credence_math::sat_percent(...)` / `credence_math::sat_mul_percent(...)` (alias).
- `credence_math::sat_percent_u64(...)` / `credence_math::sat_mul_percent_u64(...)` (alias).
- `credence_math::sat_percent_round_up(...)`, `credence_math::sat_split_percent(...)`.

These are **append-only additions** — every pre-existing helper (`mul_div_i128`, `bps`, `bps_round_up`, `bps_u64`, `split_bps`, `percent`, `percent_u64`, `percent_round_up`, `split_percent`) keeps its full signature and panic contract for backward compatibility. New code that does not need the panic should reach for the `sat_*` family directly.

### Migration notes

- The trailing `_div_msg` parameter on `percent(...)` and the `div_msg` parameter on `split_percent(...)` are **intentionally retained** as no-op forward-compat placeholders so existing call sites keep compiling. Do not drop them from call sites.
- The new helpers do not introduce a default rounding mode; callers must pass an explicit `Rounding::Down` (truncate toward zero), `Rounding::Up` (round away from zero), or `Rounding::Nearest` (half-ties round away from zero) to `sat_mul_div_i128` and stop relying on the implicit behavior of `amount * bps / 10_000`.
