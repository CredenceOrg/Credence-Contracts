# Tier System

Identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

## Thresholds (configurable in code)

| Tier     | Bonded amount (in 6 decimals) |
|----------|-------------------------------|
| Bronze   | 0 ≤ amount < 1,000           |
| Silver   | 1,000 ≤ amount < 5,000       |
| Gold     | 5,000 ≤ amount < 20,000     |
| Platinum | amount ≥ 20,000             |

Constants: `TIER_BRONZE_MAX`, `TIER_SILVER_MAX`, `TIER_GOLD_MAX` in `tiered_bond.rs`.

## Behaviour

- **get_tier()**: Returns current tier for the bond’s `bonded_amount`.
- Tier is derived from amount; no separate storage.
- On **create_bond**, **top_up**, **withdraw** (and **withdraw_early**), a **tier_changed** event is emitted only when the tier actually changes.

## Events

- **tier_changed**: (identity, new_tier)

## Upgrade / downgrade

- **Upgrade**: Increasing bonded amount (create_bond or top_up) can move to a higher tier.
- **Downgrade**: Decreasing amount (withdraw / withdraw_early) can move to a lower tier.
- Partial withdrawals that keep amount in the same band do not change tier.
