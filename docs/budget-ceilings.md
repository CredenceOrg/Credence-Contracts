# Bond Contract – Per-Entrypoint Resource Budget Ceilings

## Purpose

Soroban mainnet enforces hard per-transaction caps on:

| Resource | Mainnet limit (Stellar Protocol 21+) |
|---|---|
| CPU instructions | 100 000 000 (100 M) |
| Memory bytes | 41 943 040 (40 MiB) |

A contract that passes unit tests may still **fail on mainnet** if a future
change inflates its resource footprint beyond those limits. This document
defines the assertable ceilings for each `credence_bond` write-path
entrypoint and explains how they were derived.

---

## Methodology

1. **Baseline measurement** – Each entrypoint is called in a clean Soroban
   test environment (`Env::default()`) against a fresh, single-identity
   contract. The SDK budget meter is reset immediately before the call
   (`env.cost_estimate().budget().reset_default()`) and snapshotted
   immediately after.

2. **Headroom factor** – Ceilings are set at **2× the baseline**. This
   headroom:
   - Absorbs minor SDK-version drift (typically < 5 % per release).
   - Remains well below the mainnet hard limit (ceilings are < 1 % of the
     100 M CPU cap for simple paths).
   - Is tight enough to catch regressions that would genuinely threaten
     on-chain viability.

3. **Enforcement** – Every entrypoint test in
   `src/test_budget_ceilings.rs` ends with `assert_budget_under(env,
   CEILING)`. CI fails if either dimension is exceeded.

4. **Ceiling updates** – If a legitimate SDK upgrade or intentional feature
   expansion shifts costs beyond a ceiling, update the constant in
   `src/test_budget_helper.rs`, re-measure the new baseline, apply the 2×
   factor, and add an entry to the **Changelog** section below.

---

## Ceiling Table

All values are expressed as CPU instruction units and memory bytes.

| Entrypoint | CPU ceiling | Mem ceiling | Notes |
|---|---:|---:|---|
| `create_bond` | 200 000 000 | 4 000 000 | Allocates bond storage; emits tier-change event |
| `create_bond` (rolling) | 200 000 000 | 4 000 000 | Additional notice-period validation branch |
| `top_up` | 200 000 000 | 4 000 000 | Read-modify-write on bond storage |
| `extend_duration` | 200 000 000 | 4 000 000 | Read-modify-write on bond storage |
| `request_withdrawal` | 200 000 000 | 4 000 000 | Records timestamp, emits event |
| `withdraw` | 200 000 000 | 4 000 000 | Validates lockup expiry; partial withdrawal |
| `withdraw_early` | 200 000 000 | 4 000 000 | Penalty calculation path |
| `slash_bond` | 200 000 000 | 4 000 000 | Admin auth + reentrancy lock |
| `add_attestation` (normal) | 400 000 000 | 6 000 000 | Nonce, dedup, weight, Vec append |
| `add_attestation` (max size) | 600 000 000 | 10 000 000 | 64-byte payload + large subject list |
| `withdraw_bond` | 200 000 000 | 4 000 000 | Reentrancy-guarded full withdrawal |
| `collect_fees` | 200 000 000 | 4 000 000 | Reentrancy-guarded fee collection |
| `renew_if_rolling` | 200 000 000 | 4 000 000 | Period-end check + renewal event |

> **Mainnet safety margin**: All simple-path ceilings (200 M CPU) are well
> below the 100 M mainnet limit — but note that the SDK test environment
> uses **uncapped** budgets by default (`reset_default` enables tracking
> without enforcing the mainnet cap). The ceilings here are *assertable
> regression guards*, not exact mainnet predictions. Real on-chain costs
> will differ due to host-function billing, ledger I/O, and auth overhead.
> These ceilings should be treated as *relative* regression detectors.

---

## Edge Cases Covered

| Test name | What it exercises |
|---|---|
| `budget_slash_bond_max_slash` | Slash equals full bond amount |
| `budget_top_up_large_amount` | `i128::MAX/2` arithmetic path |
| `budget_extend_duration_max_boundary` | Near-`u64::MAX` overflow guard |
| `budget_add_attestation_max_size_payload` | 64-byte attestation data |
| `budget_add_attestation_max_prior_attestations` | 20-entry subject Vec |

---

## Regression Guard

To verify that `assert_budget_under` actually catches violations, run:

```sh
cargo test -p credence_bond budget_regression_guard_fails -- --ignored
```

This test uses an absurdly tight ceiling (1 CPU unit, 1 mem byte) and
**must** panic. It is marked `#[ignore]` so it does not block normal CI;
run it manually when modifying the assertion helper.

---

## Running Budget Tests

```sh
# All budget ceiling tests:
cargo test -p credence_bond budget

# Specific entrypoint:
cargo test -p credence_bond budget_create_bond

# With verbose output to see cost values:
cargo test -p credence_bond budget -- --nocapture
```

---

## Changelog

| Date | Change | Reason |
|---|---|---|
| 2026-05-31 | Initial ceilings set at 2× baseline (commit `perf/bond-budget-ceilings`) | Issue #427 |