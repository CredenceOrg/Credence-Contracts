# Bond Liquidation Entrypoint

This document describes the bond `liquidate` entrypoint introduced for
[issue #366](https://github.com/CredenceOrg/Credence-Contracts/issues/366).
Liquidation complements the existing `slash` and `withdraw_bond` paths by
giving the protocol admin (and any delegated keeper) an explicit, idempotent
way to **finalize** a bond whose withdrawable stake has already been zeroed
out (the bond was fully slashed) or whose lock-up window has long since ended
without renewal.

The protocol treats `liquidate` as the canonical "this bond is closed, sweep
what is left, and emit an event so off-chain indexers can move on" operation.
For background on the bond lifecycle and the slashed-funds accounting that
motivates this entrypoint, see:

  - [`docs/credence-bond.md`](./credence-bond.md) — bond lifecycle overview
  - [`docs/slashing.md`](./slashing.md) — how `slash` and `bond_slashed_v2`
    events relate to liquidation
  - [`docs/liquidation_scanner.md`](./liquidation_scanner.md) — paginated
    scanner that surfaces liquidation candidates to keepers (companion work)

---

## Goals

- **Secure:** callable by the admin (acting as their own keeper, or via a
  delegated signer). Reverts cleanly on unauthorized invocation; honors the
  existing reentrancy guard used by `withdraw_bond`, `slash_bond`, and
  `collect_fees`.
- **Idempotent:** a bond can only be liquidated once. Re-liquidating an
  already-inactive bond (whether it exited through `liquidate` or through
  `withdraw_bond`) reverts with `ContractError::BondNotActive`.
- **Auditable:** emits a `bond_liquidated` event carrying the residual
  amount, the reason symbol, the liquidation timestamp, and the admin that
  drove the call. The legacy `bond_liquidated` topic name is reused to give
  off-chain indexers a stable filter.
- **Composable with token integration:** when a treasury address and a
  bond-token are configured, sweeps the residual (`bonded_amount -
  slashed_amount`) to the treasury. When neither or only one is configured,
  the bond is still finalized on storage and the residual stays in the
  contract — indexers can replay the sweep off-chain.

---

## Entry surface

```text
// Reads (no auth)
bond = client.get_identity_state()                    // existing
client.is_liquidated(identity) -> bool                // new
client.get_liquidation_treasury() -> Option<Address>  // new

// Writes (admin-only)
client.set_liquidation_treasury(admin, treasury)      // new
client.liquidate(admin) -> IdentityBond               // new
```

`set_liquidation_treasury` is a one-shot configuration setter: it stores a
single treasury address at `DataKey::LiquidationTreasury`. Setting it
multiple times is allowed and overwrites the prior value. The companion
event `liquidation_treasury_set` is emitted so off-chain tooling can
reconcile configuration changes. There is intentionally **no** separate
treasury per-bond: one treasury handles residual sweep for every bond this
contract ever finalizes.

---

## Eligibility

`liquidate` accepts the bond only when at least one of the following holds:

| Path                 | Condition                                                    |
| -------------------- | ------------------------------------------------------------ |
| `fully_slashed`      | `bond.slashed_amount >= bond.bonded_amount`                  |
| `expired_unrenewed`  | `!bond.is_rolling && now >= bond.bond_start + bond_duration` |

A fixed-duration bond whose lock-up window has elapsed is unambiguously
eligible through `expired_unrenewed`. A rolling bond lock-up that has
elapsed is **not** eligible through this path because
[`renew_if_rolling`](./rolling-bonds.md) would have already rolled the
period forward — the canonical close for rolling bonds is still
`withdraw_bond`. Hooks for additional rolling-bond paths (such as "withdraw
requested but not finalized within X ledgers") are deliberately deferred
to follow-up work — extending the eligibility check is a one-line change
in [`contracts/credence_bond/src/lib.rs`](../contracts/credence_bond/src/lib.rs).

A bond that is healthy, partially slashed, or still inside its lock-up
window reverts with the descriptive panic message:

```text
bond is not eligible for liquidation: must be fully slashed
or expired (non-rolling) without renewal
```

---

## State transition

```text
                                eligibility check
            ┌─────────────────────────────────────────────────┐
            │                                                 │
   active=true  ──►  fully_slashed  ─►  active=false         ──► record
            │       or expired          DataKey::Liquidated(true) outcome + event
            │       unrenewed            bonded/slashed preserved
                                          residual swept (best-effort)
```

`IdentityBond.active` becomes `false`. `bonded_amount` and `slashed_amount`
are left untouched so the audit trail remains consistent for replays.
`DataKey::Liquidated(identity)` flips to `true`. Two events are emitted:

1. The canonical `bond_liquidated` event, with topics
   `(Symbol("bond_liquidated"), identity)` and body
   `(residual, reason, timestamp, admin)`.
2. The legacy same-shape event already in use for indexers; the inner
   `Symbol` reason is one of `"fully_slashed"` or `"expired_unrenewed"`.

The pre-write invariants self-check
([`invariants::assert_self_consistent`](../contracts/credence_bond/src/invariants.rs))
runs immediately before the event emission so storage drift is caught
**before** the canonical event ever reaches the ledger.

---

## Reentrancy and authorization

The reentrancy lock from `withdraw_bond` / `slash_bond` / `collect_fees`
(now also covering `liquidate`) is acquired at the top of the entrypoint
and released on every error path before the function exits. Each early
return calls `Self::release_lock(&e)` explicitly so a failed admin check
or missing-bond panic cannot strand the contract in a half-locked state.

Authorization requires the caller to match the configured
`DataKey::Admin`. As with `slash_bond`, `require_auth` runs first so a
missing-signature submission costs the caller no gas beyond signature
verification. A non-admin invocation panics with
`ContractError::NotAdmin`.

---

## Token integration sweep

The sweep is **best-effort**. When the call to
[`token_integration::transfer_from_contract`](../contracts/credence_bond/src/token_integration.rs)
cannot fire — because no treasury address is configured, no bond token is
configured, or the contract holds zero of that token — the bond is still
finalized and the residual amount is reported through the event. The
reason we keep the sweep optional:

  1. Bond ownership in this contract is ledger-side accounting; tokens
     may not be physically escrowed in the contract on every testnet.
  2. Allowing the admin to **mark** a bond liquidated without coupling to
     a token transfer keeps `liquidate` usable in upgrade windows,
     rollback scenarios, and governance-driven write-offs.

The bond-token and treasury read happen after the storage write so failures
cannot roll back the state transition. The order is:

1. Authorization + lock
2. Eligibility / idempotency checks
3. Storage write (`IdentityBond.active = false`, `Liquidated = true`)
4. Post-write invariant self-check
5. Best-effort on-token sweep
6. Event emission
7. Lock release, return final bond

---

## Event reference

```text
Symbol   : bond_liquidated
Topics   : (Symbol, Address) = ("bond_liquidated", identity)
Data     : (i128 residual, Symbol reason, u64 timestamp, Address admin)
```

| Field      | Meaning                                                                                  |
| ---------- | ---------------------------------------------------------------------------------------- |
| residual   | `bonded_amount - slashed_amount` at the moment of liquidation. `0` for fully-slashed bonds. |
| reason     | `"fully_slashed"` or `"expired_unrenewed"`.                                              |
| timestamp  | Ledger timestamp recorded by the entrypoint.                                             |
| admin      | Address whose signature authorized the call (matches `DataKey::Admin`).                 |

A replayer should set `IdentityBond.active = false` and
`DataKey::Liquidated(identity) = true` on first observation, ignore any
duplicate events for the same identity, and never infer a transfer
occurrence without confirming the corresponding token transfer event
from the bond-token contract.

---

## Test coverage

[`contracts/credence_bond/src/test_liquidate.rs`](../contracts/credence_bond/src/test_liquidate.rs)
covers the following scenarios and should be considered the source of
truth for the entrypoint's contract:

| Test                                              | Scenario                                                  |
| ------------------------------------------------- | --------------------------------------------------------- |
| `set_liquidation_treasury_admin_only`             | Non-admin cannot configure the treasury.                  |
| `set_liquidation_treasury_round_trip`             | Set → get returns the same address; overwrites are allowed.|
| `get_liquidation_treasury_unset_returns_none`    | Config-free state reads as `Option::None`.                |
| `set_liquidation_treasury_emits_event`            | Setters emit their own event for indexer reconciliation.  |
| `liquidate_fully_slashed_bond_succeeds`           | `slashed == bonded` → bond is finalized.                   |
| `liquidate_expired_unrenewed_bond_succeeds`       | Lockup elapsed → bond is finalized.                       |
| `liquidate_expired_at_exact_boundary_succeeds`    | `now == bond_start + bond_duration` accepted as expired.  |
| `liquidate_one_second_before_expiry_rejected`     | Boundary check — one second before end is still locked.   |
| `liquidate_healthy_bond_rejected`                 | Healthy in-progress bonds cannot be liquidated.          |
| `liquidate_partially_slashed_bond_rejected`       | Partial slash without expiry is not eligible.             |
| `liquidate_rolling_bond_past_lockup_rejected`     | Rolling bonds off-path; require `withdraw_bond` instead. |
| `liquidate_no_bond_rejected`                      | `BondNotFound` panic when no bond exists.                 |
| `liquidate_non_admin_rejected`                    | `NotAdmin` panic for unauthorized callers.                |
| `liquidate_twice_rejected`                        | Idempotent: second call reverts with `BondNotActive`.     |
| `liquidate_after_withdraw_bond_rejected`          | Withdrawn bonds are not eligible for liquidation.         |
| `liquidate_preserves_slashed_and_bonded_amounts`  | Amounts preserved for audit replay.                       |
| `liquidate_full_slash_records_zero_residual_eventually` | Fully-slashed bonds report zero residual in the event. |
| `liquidate_emits_bond_liquidated_event_with_residual` | Event order and shape.                                  |
| `liquidate_fully_slashed_event_uses_fully_slashed_reason` | Reason symbol on the event matches the actual path.  |
| `liquidate_without_treasury_still_marks_bond_inactive` | Best-effort sweep doesn't block the state transition. |
| `liquidate_with_treasury_no_token_configured_does_not_panic` | Treasury without token integration is a graceful skip. |

---

## Future work

- Add an explicit `set_liquidation_keeper` to support delegated signers
  without re-typing the admin's signature.
- Wire the bond-token credit ledger so liquidation transfers update the
  escrow balance atomically (rather than the current best-effort hook).
- Extend the `expired_unrenewed` path to recognize rolling bonds whose
  notice period has elapsed but whose `withdraw_bond` was never invoked,
  possibly after a configurable grace window.

These items are tracked under the same umbrella issue; the entrypoint
itself is intentionally minimal and predictable so future enhancements
do not require breaking the existing event shape.
