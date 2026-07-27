# Business Calendar & Timezones

**Audience**: Integrators and Operators

This document defines what Credence Contracts treat as "business days" and outlines timezone assumptions when interacting with scheduled operations, time-locks, and guards.

## Standard Business Hours

All time-based logic in the Credence protocol evaluates against a single, global standard:

*   **Timezone**: UTC (Coordinated Universal Time)
*   **Business Days**: Monday through Friday
*   **Business Hours**: 09:00:00 to 16:59:59 (inclusive)

The protocol **does not** account for local public holidays, daylight saving time (DST) shifts, or region-specific trading hours.

## Concrete Example: Scheduled Operations

Certain guards, such as `require_within_business_hours`, enforce that an operation executes within the business hours window.

### Rejection (Outside Business Hours)

If an operator attempts to execute a scheduled multisig withdrawal on a weekend or off-hours, the transaction will revert:

```rust
// Attempting an operation at `t = 216_000` (Saturday, Jan 3, 1970 12:00:00 UTC)
let e = Env::default();
require_within_business_hours(&e, 216_000); // Panics: OutsideBusinessHours
```

### Success (Inside Business Hours)

To succeed, the transaction must be included in a ledger whose timestamp falls within the accepted window:

```rust
// Attempting an operation at `t = 147_599` (Friday, Jan 2, 1970 16:59:59 UTC)
let e = Env::default();
require_within_business_hours(&e, 147_599); // Succeeds
```

## Guidance for Integrators

When submitting scheduled operations or estimating early-exit cooldowns:
1. Always normalize local time to UTC.
2. If your scheduled task falls on a weekend, defer execution until at least `09:00:00 UTC` the following Monday.
