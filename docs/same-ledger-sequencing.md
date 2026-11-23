# Same-Ledger Sequencing Guardrails (Anti-Sandwich)

> Issue: #996 — Bond: same-ledger sequencing guardrails for sensitive operations (anti-sandwich)
> Module: [`contracts/credence_bond/src/same_ledger_liquidation_guard.rs`](../contracts/credence_bond/src/same_ledger_liquidation_guard.rs)
> Tests: [`contracts/credence_bond/src/test_same_ledger_liquidation_guard.rs`](../contracts/credence_bond/src/test_same_ledger_liquidation_guard.rs)

## 1. Purpose

In a single Soroban ledger entry, transaction ordering is decided by the
host. A malicious actor can craft orderings where `slash` runs in the same
block as a collateral increase (`create_bond` / `top_up`). When that
happens the bond holder effectively loses stake against a deposit that did
not yet exist at the moment the slash was logically committed — a classic
sandwich.

This guard closes that hole by:

1. Recording the ledger sequence whenever collateral is added.
2. Rejecting slash entry points whose current ledger sequence still matches
   the recorded one.

The PR introduces **two pieces**:

| Symbol | Purpose |
|---|---|
| `record_collateral_increase(e)` | Persist `e.ledger().sequence()` under `DataKey::LastCollateralIncreaseLedger`. |
| `require_slash_allowed_after_collateral_increase(e)` | Panic if the recorded ledger equals the current one. |

Both are wired into the canonical `slashing::slash_bond` and into the
canonical `create_bond` / `top_up` entry points in
[`contracts/credence_bond/src/lib.rs`](../contracts/credence_bond/src/lib.rs).

## 2. Threat model (T-024)

| Threat id | Description | Mitigation |
|---|---|---|
| `T-024` | Hostile admin sandwiches a top-up + slash in the same ledger. | Guard rejects same-ledger slash after collateral increase. |
| `T-024a` | Same as `T-024` but using `create_bond` instead of `top_up`. | Guard rejects same-ledger slash after `create_bond`. |
| `T-024b` | Cross-ledger repeated sandwich (slash L1, slash L1 again). | Out of scope: ordering across ledgers is monotonic and observing hosted events is sufficient. |
| `T-024c` | Same-ledger sandwich using an account other than the bond holder. | Out of scope: only the bond holder's collateral increase matters. |

## 3. Scope

The guard is **slash-only and ledger-scoped**. It does NOT block:

- Withdrawals (locked-up bond capital returning to the holder).
- Attestations (verifier writes).
- Parameter / governance updates.
- Cross-ledger slashing (slash in ledger N+1 after a same-ledger top-up in
  ledger N is allowed).
- Any operation that did not increase the bond's collateral amount.

## 4. Storage

A new variant on `DataKey`:

```rust
LastCollateralIncreaseLedger,  // u32 ledger sequence
```

Lifetime is bound to instance storage and therefore inherits the
`bump_instance_ttl` extension from the canonical flows. The key is
**appended** to the enum rather than renamed or refactored, preserving
existing on-chain storage as documented in
[`datakey-fingerprint.md`](datakey-fingerprint.md).

## 5. Backwards compatibility

If the storage key has never been written (pre-upgrade contract, or a
freshly deployed contract whose first action is a slash with no prior
collateral increase), the guard is a **silent no-op** so legacy slashing
paths keep working.

After the first `create_bond` or `top_up`, the guard becomes active for
the rest of the contract's lifetime. No migration is required.

## 6. Operator notes

- The panic message is the literal string `"slash blocked: collateral
  increased in this ledger"`. Indexers / dashboards should treat a
  transaction matching this contract panic reason as a successful
  guard-trip, not a contract bug.
- The public helper
  `same_ledger_liquidation_guard::last_collateral_increase_ledger(e)`
  exposes the recorded sequence in a read-only fashion for observability.
- The guard does not introduce a rate-limit on slashing in general. A
  legitimate admin can still slash once per ledger entry at most.

## 7. Test coverage

The test suite
[`contracts/credence_bond/src/test_same_ledger_liquidation_guard.rs`](../contracts/credence_bond/src/test_same_ledger_liquidation_guard.rs)
covers:

| Test | Path exercised |
|---|---|
| `test_guard_noop_when_no_prior_collateral_increase` | Legacy / fresh deploy |
| `test_guard_panics_same_ledger_after_record` | Guard string match |
| `test_guard_allows_after_ledger_advance` | Recurrence after advance |
| `test_slash_same_ledger_after_create_bond_rejected` | T-024 (create_bond) |
| `test_create_bond_then_advance_then_slash_allowed` | Cross-ledger happy path |
| `test_top_up_then_slash_same_ledger_rejected` | T-024 (top_up) |
| `test_consecutive_topups_allow_slash_after_last_advance` | Stress, monotonic |
| `test_blocked_message_matches_public_constant` | Reason string contract |
| `test_guard_does_not_inspect_withdraw_state` | Module-level invariant |
| `test_many_records_overwrite_cleanly` | 1000-write stress |
| `test_guard_isolated_between_envs` | Per-instance isolation |

Target line coverage of the module: **≥95%**.

## 8. Out of scope

- Same-ledger sequencing for non-slash flows (kept intentionally narrow).
- Cross-contract same-ledger sequencing (would require protocol-level
  timestamp synchronization; not in this PR).
- Admin pause/escalation shortcuts (covered by existing pause module
  docs).
