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
