# Bond Token Custody

This document specifies the on-chain custody invariant for the `CredenceBond`
contract: how the real USDC token balance held by the contract relates to the
logical bond state, and how the four lifecycle entrypoints (`create_bond`,
`top_up`, `withdraw`, `withdraw_early`) move funds on and off custody.

## Scope

The bond contract is the source of truth for staked amounts and is consumed
by the backend reputation engine. Prior to this work, `IdentityBond` recorded
phantom balances — the contract tracked `bonded_amount` and `slashed_amount`
in storage but never actually moved USDC, so a deployer running the contract
on Stellar could read `bonded_amount == 1000` while the contract held zero
tokens. Tiers, slashing, and liquidation all operated on those phantom
balances.

This document describes the integration that closes that gap. Every custody
movement now goes through a single helper module
(`contracts/credence_bond/src/token_integration.rs`) that calls
`soroban_sdk::token::TokenClient` against the configured USDC contract.

## Entrypoints

The four lifecycle entrypoints that touch custody are:

| Entrypoint            | Direction            | USDC movement                                         |
|-----------------------|----------------------|-------------------------------------------------------|
| `create_bond`         | identity → contract  | `transfer_from` `amount` USDC after identity signs    |
| `top_up`              | identity → contract  | `transfer_from` `amount` USDC after identity signs    |
| `withdraw`            | contract → identity  | `transfer` `amount` USDC after lock-up ends           |
| `withdraw_early`      | contract → split     | `transfer` `penalty` to treasury, `net_amount` to bond owner |

## Checks–Effects–Interactions

Every entrypoint follows the same invariants:

1. **Checks** — all validation (allowance pre-check, balance pre-check,
   arithmetic bounds, lock-up state) happens first.
2. **Effects** — `IdentityBond` storage and any tier/parameter events are
   written **before** the external token call.
3. **Interactions** — the Soroban token call (`transfer_from` or `transfer`)
   happens last, only after state mutation succeeds.

This ordering rules out reentrancy: a hostile token contract re-entering the
bond during the external call observes the post-mutation state, so it cannot
double-spend.

## Fee-on-transfer rejection

`token_integration::transfer_into_contract` and
`token_integration::transfer_from_contract` both wrap
`try_transfer_from` / `try_transfer` in a balance-delta guard:

1. Read `token.balance(bond_contract)` before the call.
2. Call the token transfer.
3. Read `token.balance(bond_contract)` after the call.
4. Assert `actual_received == amount` (inbound) or `actual_sent == amount`
   (outbound).

Any mismatch panics with
`unsupported token: transfer amount mismatch (code 213)`, and the entire
transaction reverts atomically — no state change is persisted.

## Custody invariant

For unslashed bonds, the contract's USDC balance attributable to bonded
identities is the sum of `(bonded_amount - slashed_amount)`:

```text
token.balance(bond_contract)
    == Σ over active identities of (bonded_amount − slashed_amount)
      + Σ slashed funds that left the contract via the slash-treasury sweep
      + early-exit penalties routed to the early-exit treasury
      − sums paid out to identities via `withdraw` / `withdraw_early`
```

Loosely: **plus everything that has ever escrowed less everything that has
ever been transferred out**, partitioned by source (bond principal vs. slash
treasury vs. early-exit treasury).

### Validation

The `bond_lifecycle.rs` integration suite and the
`test_bond_token_transfers.rs` invariant test enforce this contract:

| Step                     | Pre-state                  | Action           | Post-state                     |
|--------------------------|----------------------------|------------------|--------------------------------|
| `create_bond(1_000)`     | contract balance = 0       | pull 1_000       | contract balance = 1_000       |
| `top_up(500)`            | contract balance = 1_000   | pull 500         | contract balance = 1_500       |
| `slash(200)`             | bonded = 1_500, slashed = 0 | mark slashed 200 | bonded = 1_500, slashed = 200; treasury += 200 on sweep |
| `withdraw(400)`          | bonded = 1_300 (available) | push 400         | contract balance = 1_100, identity balance += 400 |

After each step the contract asserts:

```text
token.balance(bond_contract) == bonded_amount − slashed_amount
    + (treasury accumulations not yet swept)
```

`invariants::assert_self_consistent` additionally guards bounds:
`bonded_amount >= slashed_amount`, `bonded_amount >= 0`, `slashed_amount >= 0`.

## Phantom-balance deployments

If the contract is initialised without calling `set_token` (i.e.
`DataKey::BondToken` is unset), all four entrypoints operate in
**phantom-balance mode**: state writes succeed, no token movement happens,
and an off-chain operator is expected to reconcile the on-paper balances
against external token movements. This mode preserves backward compatibility
with the pre-integration behaviour for non-token deployments and is the same
fallback the slashing/claims subsystems use.

`token_integration::has_token(&e)` is the single source of truth for which
mode is active.

## Failure modes (atomic rollback)

Every custody flow is atomic. Any panic from the helper module aborts the
Soroban transaction and reverts all state. Failure modes:

| Failure                       | Panic string                                              | Source module |
|-------------------------------|-----------------------------------------------------------|---------------|
| No configured token           | (gated out — flow proceeds without attempting a transfer)| `token_integration.rs` |
| Negative amount               | `amount must be non-negative`                             | `safe_token.rs` (also `token_integration.rs` pre-checks) |
| Zero amount                   | (early-return; zero-amount transfers skipped)             | `token_integration.rs` |
| Insufficient allowance        | `insufficient token allowance`                            | `safe_token.rs` / `token_integration.rs` |
| Token call failed             | `token transfer failed`                                   | `safe_token.rs` |
| Fee-on-transfer mismatch      | `unsupported token: transfer amount mismatch (code 213)`  | `token_integration.rs` (balance-delta guard) |
| Bond balance underflow        | `balance underflow`                                       | `token_integration.rs` (balance-delta fallback) |

In every failure mode, no Soroban storage entry (`IdentityBond`,
`LastCollateralIncreaseLedger`, tier events, etc.) is persisted.

## Cross-references

- `contracts/credence_bond/src/token_integration.rs` — single integration
  layer with `transfer_into_contract` / `transfer_from_contract` / fee-on-
  transfer guards.
- `contracts/credence_bond/src/storage.rs` — `DataKey::BondToken` (and the
  `set_accepted_tokens` allow-list).
- `contracts/credence_bond/src/events.rs` — emitter for the
  `bond_fund_transfer` event raised by `transfer_from_contract_with_source`.
- `docs/token-integration.md` — companion document with the
  configuration-and-API surface.
- `docs/credence_bond_api.md` — full entrypoint reference.
- `contracts/credence_bond/docs/ATTACK_TREE.md` §3 — withdrawal-path
  attacks.
