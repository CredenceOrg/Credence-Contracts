# Bond Creation Fee Mechanism

## Overview

A configurable fee is charged when creating a bond, as a percentage of the bonded amount. The fee is accrued in the contract and can be collected to the protocol treasury. Fee waiver is supported when fee is 0 or amount is 0.

## Configuration

- **Treasury**: Address that receives collected fees (set with fee config).
- **Fee rate**: Basis opoints (e.g. 100 = 1%, 1_000 = 10%). Capped at
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

- On `create_bond(identity, amount, ...)`: the contract pulls the full `amount` from the identity, computes fee = `amount * fee_bps / 10_000` (rounded down) and net = `amount - fee`. The bond is created with `bonded_amount = net`, the fee is accrued in the contract as protocol fees, and a `bond_creation_fee` event is emitted.
- If `fee_bps` is 0 or no treasury is set, no fee is applied (net = amount).
- Admin can withdraw accrued fees via `collect_fees(admin, salt)`. The admin-only call reads the accrued fee amount, zeros the accrual **before** the transfer, then transfers the accrued amount to the configured fee treasury via `token_integration::transfer_from_contract_with_source(…, FundSource::ProtocolFee)`, and returns the amount. If no treasury is configured the call fails with a typed error. When the accrual is 0 the call returns 0 and transfers nothing.
- `collect_fees` is idempotent per salt: when a non-empty `salt` is supplied, a repeated call with the same salt fails with `DuplicateIdempotencyKey`.

## Depositing fees

The legacy public, unauthenticated `deposit_fees` entry point has been **removed**. Fees are now accrued only as a side effect of `create_bond`, so protocol fee accounting cannot be inflated by arbitrary callers. Any additional fees must flow through a real token transfer and admin authorization.

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
- **Rounding**: fee is computed with integer division and rounds down. With `fee_bps = 1`, an amount of 9,999 yields fee 0 and bond 9,999; an amount of 10,000 yields fee 1.
- **Max fee**: fee_bps = `MAX_FEE_BPS` (1_000 = 10%) → fee = `amount / 10`, net = `9 × amount / 10`. The legacy `fee_bps = 10_000` is no longer reachable — the contract caps at `MAX_FEE_BPS`.
- **Overflow**: Fee and net use checked arithmetic. Accrual uses `checked_add`. cmath::split_bps` is tested at `amount = i128::MAX / 10_000` to ensure no silent clamp happens.

## Custody invariant

The contract token balance must always satisfy:

```
contract_balance ≥ Σ(bonded − slashed) + accrued_fees + pending_claims
```

This is asserted in the test suite after the sequence create, create, slash, `collect_fees`, withdraw.

## Security

- Only admin can set fee config.
- Only admin can collect fees, and collection transfers real tokens to the configured treasury.
- The unauthenticated `deposit_fees` write path has been removed; fees can no longer be forged.
- fee_bps is bounded to `[MIN_FEE_BPS, MAX_FEE_BPS] = [0, 1_000]`; out-of-range
  values are rejected with `"fee_bps out of bounds"` and storage is unchanged.
- Every successful fee-config change emits a `fee_config_updated` event with
  old/new values for governance transparency.
