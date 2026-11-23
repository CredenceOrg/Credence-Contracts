# Tier System

Identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

## Thresholds (Admin-configurable with Fallback Constants)

All thresholds are internally represented and checked in **normalized 18-decimal format**. This ensures consistent boundary checks across different tokens regardless of their native decimal precision.

### Exact Boundary Table

| Tier | Lower Bound (Inclusive) | Upper Bound (Exclusive) | Default Threshold Value (Code Constant) |
|---|---|---|---|
| **Bronze** | `0` | `< 1,000 * 10^18` | `TIER_BRONZE_MAX = 1,000,000,000,000,000,000,000` |
| **Silver** | `1,000 * 10^18` | `< 5,000 * 10^18` | `TIER_SILVER_MAX = 5,000,000,000,000,000,000,000` |
| **Gold** | `5,000 * 10^18` | `< 20,000 * 10^18` | `TIER_GOLD_MAX = 20,000,000,000,000,000,000,000` |
| **Platinum** | `20,000 * 10^18` | `i128::MAX` | N/A (Catch-all) |

### Boundary Semantics

- **Bronze**: Any amount $A$ where $0 \le A < 1,000 \times 10^{18}$. Negative amounts map to Bronze but are rejected by validation on ingress.
- **Silver**: Any amount $A$ where $1,000 \times 10^{18} \le A < 5,000 \times 10^{18}$.
- **Gold**: Any amount $A$ where $5,000 \times 10^{18} \le A < 20,000 \times 10^{18}$.
- **Platinum**: Any amount $A$ where $A \ge 20,000 \times 10^{18}$ (up to `i128::MAX`).

---

## Admin Configuration

The contract admin can update the tier thresholds at runtime using the `set_tier_thresholds` function.

### Method Signature

```rust
pub fn set_tier_thresholds(
    e: Env,
    admin: Address,
    bronze_max: i128,
    silver_max: i128,
    gold_max: i128,
)
```

### Constraints and Validation

To ensure mathematical consistency, the contract validates the proposed thresholds and panics if any of the following bounds are violated:
1. `bronze_max > 0` (The Bronze boundary must be positive).
2. `silver_max > bronze_max` (Silver boundary must exceed Bronze).
3. `gold_max > silver_max` (Gold boundary must exceed Silver).

### Events

Updating the thresholds emits a `tier_thresholds_changed` event:
- **Topics**: `("tier_thresholds_changed",)`
- **Data**: `(old_thresholds, new_thresholds)` where each is a `TierThresholds` struct.

---

## Behaviour

- **get_tier()**: Returns current tier for the bond’s `bonded_amount` using the configured thresholds.
- Tier is derived dynamically from amount; there is no separate tier storage.
- On **create_bond**, **top_up**, **withdraw** (and **withdraw_early**), a **tier_changed** event is emitted only when the tier actually changes.
- **Slashing**: Slashing increases `slashed_amount` but does not modify `bonded_amount`. Therefore, a slashed bond does not lose its tier rank (reputation is preserved).

### Tier change events

When the on-chain tier transitions, two events fire (both are emitted on every
transition; clients should consume the v2 flavour when possible):

| Event | Topics | Data |
|---|---|---|
| `tier_changed` (v1, deprecated) | `(tier_changed,)` | `(identity, new_tier)` |
| `tier_changed_v2` (current) | `(tier_changed_v2, identity)` | `(old_tier, new_tier, timestamp)` |

Both events are emitted only on actual transitions (no-op operations with no
tier change are silent). `tier_changed_v2` matches the v2 indexer contract
introduced in **issue #241** and is the only one that carries the previous tier
plus the timestamp directly in the event itself.

## Upgrade / Downgrade

- **Upgrade**: Increasing bonded amount (create_bond or top_up) can move to a higher tier.
- **Downgrade**: Decreasing amount (withdraw / withdraw_early) can move to a lower tier.
- Partial withdrawals that keep the amount in the same band do not trigger a tier change or emit events.

## Verified Boundary Behaviour

The `contracts/credence_bond/src/test_tier_boundary_fuzz.rs` suite proves the
following invariants hold for any valid threshold configuration (default or
admin-tuned):

| Invariant | Verification |
|---|---|
| `get_tier_for_amount(a)` is a deterministic step function in `a` | Properties 1–2 of `proptest` sweep + deterministic fuzz |
| Every threshold cross from below to exactly-at produces a tier change | Table-driven `test_boundary_table_*_thresholds` |
| Every threshold cross from exactly-at to above produces a tier change | Table-driven `test_boundary_table_*_thresholds` |
| Boundary values are read from the configured constants (not hardcoded) | `setup_with_initial_bond(BRONZE_MAX - 1)` style tests |
| Crossing two thresholds in a single `top_up` produces exactly **one** event | `test_top_up_crosses_two_thresholds_in_one_call` |
| Tier rank is monotone non-decreasing with `bonded_amount` regardless of operation order | `fuzz_tier_tracks_bonded_amount_under_random_sequences` |
| Tier is preserved across `slash` (the input is `bonded_amount`, not available balance) | `test_fully_slashed_bond_preserves_tier` + regression |
| A full exit (`withdraw_bond`) collapses tier to `Bronze` (because `bonded_amount → 0`) | `test_full_exit_collapses_tier_to_bronze` |
| `create_bond` / `top_up` / `withdraw` emit `tier_changed_v2` carrying `(old_tier, new_tier, timestamp)` | `test_*_emits_tier_changed_event_*` |
| `slash` does not emit a tier event (no `bonded_amount` change) | `test_slash_does_not_emit_tier_event` |

Run locally:

```bash
cargo test -p credence_bond test_tier
cargo test -p credence_bond tier
BOND_TIER_FUZZ_ITERS=10000 BOND_TIER_FUZZ_ACTIONS=20 \
    cargo test -p credence_bond --release fuzz_tier_tracks_bonded_amount_under_random_sequences -- --nocapture
```

