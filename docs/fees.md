# Bond Creation Fee Mechanism

## Overview

A configurable fee is charged when creating a bond, as a percentage of the bonded amount. The fee is accumulated in the contract and can be collected to the protocol treasury. Fee waiver is supported when fee is 0 or amount is 0.

## Configuration

- **Treasury**: Address that receives collected fees (set with fee config).
- **Fee rate**: Basis points (e.g. 100 = 1%, 1_000 = 10%). Capped at
  `MAX_FEE_BPS` (issue #1027 governance safety rail).

| Function | Auth | Description |
|----------|------|-------------|
| `set_fee_config(admin, treasury, fee_bps)` | Admin | Set treasury and fee in basis points. Enforces `[MIN_FEE_BPS, MAX_FEE_BPS] = [0, 1_000]`. |
| `get_fee_config()` | — | Returns `(Option<treasury>, fee_bps)`. |

## Governance bounds (issue #1027)

The bond-creation fee is bounded to `[MIN_FEE_BPS, MAX_FEE_BPS]` =
`[0, 1_000]` basis points (0%..10%). Out-of-range proposals are rejected
with `panic!("fee_bps out of bounds")` and the storage is left untouched.
The bounds mirror
[`MAX_PROTOCOL_FEE_BPS`](parameters.md#fee-rates) and
[`fee.rs::MAX_FEE_BPS`](../../contracts/credence_bond/src/fee.rs) so all
fee rails share one consistent ceiling.

## Behavior

- On `create_bond(identity, amount, ...)`: fee = `amount * fee_bps / 10_000`, net = `amount - fee`. The bond is created with `bonded_amount = net`. The fee is added to the contract’s fee pool and a `bond_creation_fee` event is emitted.
- If `fee_bps` is 0 or no treasury is set, no fee is applied (net = amount).
- Admin can withdraw accumulated fees via `collect_fees(admin)` (existing API).

## Events

- `bond_creation_fee`: `(identity, bond_amount, fee_amount, treasury)` — emitted
  every time a fee amount is recorded against a bond.
- `fee_config_updated` (issue #1027): topics
  `(Symbol("fee_config_updated"), admin: Address)`; data
  `(old_treasury: Option<Address>, new_treasury: Address, old_fee_bps: u32,
  new_fee_bps: u32)`. **One event per successful governance call**, regardless
  of whether both fields changed — indexers can treat `old == new` as a
  no-op re-emission. Rejected (out-of-range) calls do NOT emit this event.

## Edge Cases

- **Zero fee**: fee_bps = 0 or amount ≤ 0 → fee = 0, net = amount.
- **Max fee**: fee_bps = `MAX_FEE_BPS` (1_000 = 10%) → fee = `amount / 10`, net = `9 × amount / 10`. The legacy `fee_bps = 10_000` is no longer reachable — the contract caps at `MAX_FEE_BPS`.
- **Overflow**: Fee and net use checked arithmetic.

## Security

- Only admin can set fee config.
- fee_bps is bounded to `[MIN_FEE_BPS, MAX_FEE_BPS] = [0, 1_000]`; out-of-range
  values are rejected with `"fee_bps out of bounds"` and storage is unchanged.
- Every successful fee-config change emits a `fee_config_updated` event with
  old/new values for governance transparency.
