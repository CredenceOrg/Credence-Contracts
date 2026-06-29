# Differential Testing Guide

## Purpose

The repository carries three-way merge artifacts (`ours.rs`, `base.rs`, `theirs.rs`) at the
root alongside the canonical `contracts/credence_bond` crate.  Issue #351 tracks the
consolidation of those artifacts.  **This document describes the safety-net harness that
runs alongside consolidation:** a differential test suite that replays the same scripted bond
lifecycles against every fork and asserts byte-identical state and event streams.

> **Security rule:** the harness and all divergent fork modules compile **only** under
> `#[cfg(test)]`.  They are never shipped to mainnet.

## Architecture

### Location

- `contracts/credence_bond/tests/differential.rs` — integration test harness.
- `contracts/credence_bond/src/fork_ours.rs`
- `contracts/credence_bond/src/fork_base.rs`
- `contracts/credence_bond/src/fork_theirs.rs`
- `contracts/credence_bond/src/fork_divergent.rs` — deliberate-bug variant used to prove the
  harness can catch divergence.

### Fork gating

Each fork module is declared in `lib.rs` behind `#[cfg(test)]`:

```rust
#[cfg(test)]
pub mod fork_ours;
#[cfg(test)]
pub mod fork_base;
#[cfg(test)]
pub mod fork_theirs;
#[cfg(test)]
pub mod fork_divergent;
```

### Single-Env registration

All four contracts are registered in **one** `soroban_sdk::Env` instance:

```rust
let env = Env::default();
let canonical_id = env.register_contract(None, credence_bond::CredenceBond);
let ours_id      = env.register_contract(None, credence_bond::fork_ours::CredenceBond);
// ...
```

This guarantees that `Val` equality comparisons and event filtering are deterministic.

### Normalisation layer

Each fork defines its own `IdentityBond` type (they are structurally identical but distinct
Rust types).  The harness converts every fork’s bond into a local `BondSnapshot` using
`From` impls, then asserts `PartialEq` across the four variants.

Events are filtered by `contract_id`, normalised into `EventSnapshot { symbol, topics, data }`,
and compared element-by-element.

### Scenario DSL

A scenario is a `Vec<Step>` where each `Step` is an enum variant:

| Step variant                | Invariant category verified                              |
|----------------------------|----------------------------------------------------------|
| `Initialize`               | Admin storage consistency                                |
| `CreateBond`               | Positive amount, positive duration, rolling params valid |
| `TopUp`                    | Monotonic increase of `bonded_amount`                    |
| `RequestWithdrawal`        | Request timestamp recorded                               |
| `Withdraw`                 | Post-lock-up balance reduction, available-bound check    |
| `WithdrawEarly`            | Time-decayed penalty, pre-expiry rejection               |
| `Slash`                    | Monotonic `slashed_amount`, cap at `bonded_amount`       |
| `SlashBond`                | Reentrancy-guarded path parity with `Slash`              |
| `ExtendDuration`           | `bond_duration` increase                                 |
| `RenewIfRolling`           | Period-end start reset when no withdrawal requested      |
| `CheckTier`                | Deterministic tier mapping from `bonded_amount`          |
| `AddAttestation`           | Storage mutation + event emission                        |
| `RevokeAttestation`        | Revocation flag + event emission                         |
| `AdvanceTime`              | Ledger-time shift for time-dependent checks              |

### Running the harness

```bash
cargo test -p credence_bond --tests differential -- --nocapture
```

To run a single scenario:

```bash
cargo test -p credence_bond scenario_full_bond_lifecycle -- --nocapture
```

## Edge-case coverage

The built-in scenarios exercise:

1. **Zero-amount slash** — `Slash { amount: 0 }` must not mutate state.
2. **Tier boundary** — amounts exactly at Bronze→Silver (1_000), Silver→Gold (5_000),
   and Gold→Platinum (20_000) thresholds.
3. **Rolling renew at exact expiry** — ledger time advanced to `bond_start + duration`
   and then `+ 1` to verify renewal happens exactly at the boundary.

## Deliberate-divergence test

`deliberate_divergence_is_caught` registers a **fifth** contract (`fork_divergent`) whose
`get_tier` always returns `Gold` for any amount ≥ 1.  The harness compares the tier
snapshot against canonical and **must panic** with a divergence message.  The test is
annotated `#[should_panic(expected = "divergent")]`; if the harness ever stops
detecting the bug, the test fails.

## Interpreting failures

When a scenario fails, the assertion message includes the fork name and the step that
diverged:

```
[base] bond state diverged: withdraw
[theirs] event 2 diverged: SlashBond { ... }
```

Fix the canonical crate or the relevant fork so that all four snapshots match before
merging the consolidation PR.

## Commit checklist for #351

- [ ] Run `cargo test -p credence_bond --tests differential -- --nocapture` and observe **all green**.
- [ ] If a fork is intentionally preserved (e.g. for historical reference), move it to
      `tests/differential/forks/` and update the harness registration.
- [ ] Delete the root-level merge artifacts (`ours.rs`, `base.rs`, `theirs.rs`) once the
      consolidation PR is merged.
- [ ] Remove `fork_divergent` from `lib.rs` before shipping to production (it is already
      `#[cfg(test)]` gated, but explicit removal is safer).
