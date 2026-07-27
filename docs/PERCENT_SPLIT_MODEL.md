# Percent Split Model — Multi-Recipient Representation

## Audience: Downstream Integrator

This document describes how Credence represents **multi-recipient percent splits** on-chain: units, layout, validation, and how amounts are applied. It is written for integrators building fee routers, payout schedulers, or off-chain UIs that assemble split vectors for contract calls.

---

## Representation

A percent split is a list of basis-point shares. Each share is a `u32` in **basis points (bps)**:

| Concept | Value |
| --- | --- |
| Full allocation | `10_000` bps (= 100%) |
| 1% | `100` bps |
| 0.01% | `1` bps |

Multi-recipient layouts are ordered vectors of recipient/share pairs:

```text
splits = [(recipient_0, bps_0), (recipient_1, bps_1), ..., (recipient_n, bps_n)]
```

Concrete Soroban shape (illustrative):

```rust
use soroban_sdk::{Address, Vec};

/// One recipient's share of a distribution, in basis points.
pub struct PercentSplitShare {
    pub recipient: Address,
    pub bps: u32,
}

/// Multi-recipient split: ordered list of shares.
/// Invariant: sum(share.bps) == 10_000
pub type PercentSplit = Vec<PercentSplitShare>;
```

Integrators should treat the vector as **ordered and authoritative**: payout order follows vector order; residual handling (if any) is applied to the last entry only when a contract documents that policy.

---

## Validation Rule

The guard `require_valid_percent_split(splits)` enforces:

```text
sum(splits[i].bps for all i) == 10_000
```

using overflow-safe accumulation (`checked_add`). Any other total is rejected with a typed contract error.

| Sum of bps | Result |
| --- | --- |
| `< 10_000` | Reject — under-allocated (value would leak / stay unassigned) |
| `== 10_000` | Accept |
| `> 10_000` | Reject — over-allocated (would mint or overdraw) |

Empty vectors and individual `bps == 0` entries are rejected unless a specific entrypoint documents an exception (none do today for multi-recipient splits).

```rust
fn require_valid_percent_split(e: &Env, splits: &Vec<PercentSplitShare>) {
    let mut total: u32 = 0;
    for share in splits.iter() {
        total = total
            .checked_add(share.bps)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
    }
    if total != 10_000 {
        panic_with_error!(e, /* typed invalid-split error */);
    }
}
```

---

## Applying a Split to an Amount

Given a gross `amount: i128` and a validated split, each recipient receives:

```text
payout_i = floor(amount * bps_i / 10_000)   // Rounding::Down by default
```

using `credence_math::mul_div_i128` / `bps` so the intermediate product cannot overflow `i128` before division (see [decimal-handling.md](decimal-handling.md)).

### Two-way fee/net special case

Many bond paths use a **two-lane** split rather than an N-recipient vector:

```rust
use credence_math::split_bps;

// fee_bps out of 10_000 → (fee, net) with fee + net == amount
let (fee, net) = split_bps(amount, fee_bps, "mul", "div", "sub");
```

This is the same bps model with an implicit second recipient (the bond principal / net lane). Multi-recipient vectors generalize that idea to N named addresses.

### Worked example

Gross amount `1_000_000`, three recipients:

| Recipient | bps | Share |
| --- | --- | --- |
| Treasury | `2_000` | 200_000 |
| Operator | `3_000` | 300_000 |
| Creator | `5_000` | 500_000 |
| **Total** | **10_000** | **1_000_000** |

If the sum were `9_999` or `10_001`, `require_valid_percent_split` rejects before any transfer.

---

## Relation to Existing Helpers

| Helper / doc | Role |
| --- | --- |
| `credence_math::BPS_DENOMINATOR` (`10_000`) | Canonical full-scale denominator |
| `credence_math::split_bps` | Two-way fee/net split |
| `credence_math::bps` / `bps_round_up` | Single-share extraction |
| [fees.md](fees.md) | Bond creation fee config (`fee_bps`) |
| [decimal-handling.md](decimal-handling.md) | Rounding and `mul_div_i128` |
| Treasury proportional lanes | Splits **sources** (protocol fee vs slash), not arbitrary recipients — see `contracts/credence_treasury/docs/accounting-model.md` |

---

## Integrator Checklist

- [ ] Encode shares in **bps**, never floating-point percents.
- [ ] Assert `sum(bps) == 10_000` off-chain before submitting.
- [ ] Prefer `mul_div_i128` for amount × bps application.
- [ ] Do not assume residual dust is auto-assigned unless the entrypoint docs say so.
- [ ] Treat empty or zero-bps vectors as invalid.

---

## Version History

| Version | Date | Notes |
| --- | --- | --- |
| 1.0 | 2026-07-26 | Initial multi-recipient percent-split model |
