# Token Integration (USDC)

This document describes how the Credence bond contract integrates with Stellar token contracts for USDC-denominated bonds.

## Overview

The bond contract uses Soroban token interfaces for all value movements. The
token-custody layer is implemented in `contracts/credence_bond/src/token_integration.rs`
and is the single integration point for every entrypoint that touches
USDC. All real on-chain custody flows follow the Checks–Effects–Interactions
(CEI) pattern: bond storage is updated *before* the external token call.

- `initialize(admin)` registers the admin; `set_token(admin, token)` is
  the canonical way to configure the USDC contract address (stored at
  `DataKey::BondToken` after `set_accepted_tokens` has whitelisted it).
- `create_bond` and `top_up` pull `amount` USDC from `identity` to the bond
  contract via `TokenClient::transfer_from`, after `identity.require_auth()`
  and a `token.allowance(owner, contract) >= amount` pre-check.
- `withdraw` pushes `amount` USDC from the bond contract back to `identity`
  via `TokenClient::transfer`. The entrypoint is gated by `has_token()` so
  phantom-balance deployments remain supported.
- `withdraw_early` pushes two transfers bucketed by source: `penalty` to
  the early-exit treasury (FundSource::ProtocolFee, emitting
  `bond_fund_transfer`) and `net_amount = amount - penalty` back to
  `identity`.

Every transfer (inbound or outbound) is wrapped in a balance-delta guard
that compares `token.balance(contract)` before and after the call; any
mismatch aborts the transaction with
`unsupported token: transfer amount mismatch (code 213)`. See
[bond-token-custody.md](bond-token-custody.md) for the full custody
invariant, edge cases, and validation tests.

## Contract API

- `initialize(admin, token)`
  - Stores the custody token during contract setup.
- `get_token()`
  - Returns the currently configured token address.

## Security Model

Token handling is centralized in `contracts/credence_bond/src/token_integration.rs` with the following controls:

1. **Admin-gated token configuration**
   - Only stored admin can set token address.
2. **Allowance pre-checks**
   - Before `transfer_from`, contract checks `allowance(owner, contract)`.
   - If allowance is insufficient, call fails with `insufficient token allowance`.
3. **Positive amount validation**
   - `create_bond`, `top_up`, `withdraw`, and `withdraw_early` reject `amount <= 0`.
4. **Checks-effects-interactions**
   - Exit paths persist the reduced bond state before transferring tokens out.
5. **Single integration layer**
   - Prevents duplicated transfer logic and keeps security review surface small.

## Assumptions

- Admin initializes the contract with a valid token contract address.
- Identity accounts grant approvals to the bond contract before `create_bond` and `top_up`.
- Token contract adheres to Soroban token interface semantics.

## Test Coverage (Integration-Specific)

Root custody tests cover:

- Token configuration and retrieval.
- Successful token movement into contract during `create_bond`.
- Failure on missing allowance for `create_bond`.
- Failure when `top_up` exceeds remaining allowance.
- Successful token movement back to identity on `withdraw`.
- Treasury and identity routing during `withdraw_early`.

Run targeted tests:

```bash
cargo test -p credence_bond token_integration_test -- --nocapture
```

Run full package tests:

```bash
cargo test -p credence_bond -- --nocapture
```

## Custody Invariant

The contract's USDC balance attributable to bonded identities is the
running sum of `(bonded_amount - slashed_amount)` plus treasury
accumulations (slashed funds and early-exit penalties not yet routed
out).

```text
token.balance(bond_contract)
    == Σ(bonded_amount − slashed_amount) over active identities
      + treasuries[slash_treasury]
      + treasuries[early_exit_treasury]
    − Σ(amount) for historical `withdraw` / `withdraw_early` payouts
```

For the unslashed, unslashed-penalty case:

```text
token.balance(bond_contract) == bonded_amount - slashed_amount
```

See [bond-token-custody.md](bond-token-custody.md) for the full
broken-down invariant, the validation tests that exercise it, and the
failure-mode rollback semantics.
