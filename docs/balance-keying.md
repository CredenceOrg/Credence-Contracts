# Balance Keying Model

Audience: contributors and reviewers changing accounting, custody, or indexer code.

Credence uses two different balance models. The bond contract tracks per-identity logical stake, while the treasury contract tracks pooled funds by source. Keeping those key shapes distinct prevents indexers from joining unrelated balances under a single address-only key.

## Bond Contract: Per-Identity Logical Balances

`contracts/credence_bond/src/lib.rs::DataKey` stores the bond ledger for one identity under address-scoped keys:

| Key | Value | Meaning |
| --- | --- | --- |
| `DataKey::Bond(Address)` | `IdentityBond` | The logical bonded amount, slashed amount, owner, status, and timing for one identity address. |
| `DataKey::AttesterStake(Address)` | `i128` | Stake tracked for an attester address. |
| `DataKey::ClaimableAmount(Address)` | `i128` | Total pull-payment amount claimable by one recipient. |
| `DataKey::PendingClaims(Address)` | `Vec<claims::PendingClaim>` | Pending claim records for one recipient. |
| `DataKey::Liquidated(Address)` | `bool` | Whether one identity has already been liquidated. |
| `DataKey::CooldownRequest(Address)` | `CooldownRequest` | Pending cooldown withdrawal request for one identity. |

The bond contract is not a shared token ledger. It records the contract's logical obligation for an identity and then uses `contracts/credence_bond/src/token_integration.rs` to compare that logical state to the configured token contract's real custody balance.

## Treasury Contract: Pooled Source Balances

`contracts/credence_treasury/src/treasury.rs::DataKey` stores pooled accounting for funds received by the protocol treasury:

| Key | Value | Meaning |
| --- | --- | --- |
| `DataKey::TotalBalance` | `i128` | Current available treasury balance across all sources. |
| `DataKey::BalanceBySource(FundSource)` | `i128` | Current available balance for `ProtocolFee` or `SlashedFunds`. |
| `DataKey::CumulativeReceived` | `CumulativeAmount` | Lifetime amount received across all sources. |
| `DataKey::CumulativeReceivedBySource(FundSource)` | `CumulativeAmount` | Lifetime amount received for one source. |
| `DataKey::Proposal(u64)` | `WithdrawalProposal` | Withdrawal proposal keyed by proposal id, not by recipient. |
| `DataKey::Approval(u64, Address)` | `bool` | Approval keyed by proposal id and signer address. |

Treasury balances are source-keyed, not user-keyed. A withdrawal proposal has a recipient, but the balance buckets remain `TotalBalance` plus `BalanceBySource(FundSource)` so protocol fees and slashed funds can be reported separately.

## Cumulative Amount Encoding

`CumulativeAmount` stores lifetime receipts as:

```text
rollovers * (i128::MAX + 1) + remainder
```

Use `cumulative_to_u256(env, amount)` when exporting or comparing these values. Do not reconstruct cumulative receipts by casting `rollovers` and `remainder` independently in off-chain code; that invites overflow and inconsistent reporting.

## Indexer Rules

- Use `(contract_id, DataKey::Bond(identity))` for bond state; do not group bond balances only by identity address across contracts.
- Use `(contract_id, FundSource)` for treasury source balances; do not infer per-user balances from `BalanceBySource`.
- Treat token custody (`token.balance(contract)`) as an external token-contract read, not as a Credence storage key.
- When adding a new balance-like key, document whether it is identity-keyed, source-keyed, proposal-keyed, or global before adding indexer support.

## Review Checklist

- New bond balance keys should include the identity or recipient address in the key shape unless they intentionally represent one contract-wide setting.
- New treasury balance keys should explain whether they affect `TotalBalance`, `BalanceBySource(FundSource)`, or cumulative receipt reporting.
- Tests should assert both the happy path and the key boundary that prevents one account/source from overwriting another.
- Docs should link back to `docs/STORAGE_KEY_LAYOUT.md` when the change adds or renames a `DataKey` variant.