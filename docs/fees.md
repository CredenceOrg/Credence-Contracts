# Bond Creation Fee Mechanism

## Overview

A configurable fee is charged when creating a bond, as a percentage of the bonded amount. The fee is accumulated in the contract and can be collected to the protocol treasury. Fee waiver is supported when fee is 0 or amount is 0.

## Configuration

- **Treasury**: Address that receives collected fees (set with fee config).
- **Fee rate**: Basis points (e.g. 100 = 1%, 10_000 = 100%). Max 10_000.

| Function | Auth | Description |
|----------|------|-------------|
| `set_fee_config(admin, treasury, fee_bps)` | Admin | Set treasury and fee in basis points. |
| `get_fee_config()` | — | Returns (Option<treasury>, fee_bps). |

## Behavior

- On `create_bond(identity, amount, ...)`: fee = `amount * fee_bps / 10_000`, net = `amount - fee`. The bond is created with `bonded_amount = net`. The fee is added to the contract’s fee pool and a `bond_creation_fee` event is emitted.
- If `fee_bps` is 0 or no treasury is set, no fee is applied (net = amount).
- Admin can withdraw accumulated fees via `collect_fees(admin)` (existing API).

## Events

- `bond_creation_fee`: (identity, bond_amount, fee_amount, treasury)

## Edge Cases

- **Zero fee**: fee_bps = 0 or amount ≤ 0 → fee = 0, net = amount.
- **Max fee**: fee_bps = 10_000 → fee = amount, net = 0.
- **Overflow**: Fee and net use checked arithmetic.

## Security

- Only admin can set fee config.
- fee_bps is capped at 10_000.
