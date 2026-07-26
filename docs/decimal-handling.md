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
