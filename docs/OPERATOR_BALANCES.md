# Operator Balances Guide

**Audience:** Operator (person or bot running on-chain administrative tasks)  
**Last updated:** 2026-07-25

This document explains what each operation costs and how to keep your signing
account and protocol balances healthy.

---

## 1. Who Is an Operator

An Operator holds level-1 (`Operator`) role in the admin hierarchy
([admin-roles.md](admin-roles.md)). Operators are the automated agents that run
day-to-day protocol maintenance:

- Configure contract parameters (fee rates, accepted tokens, attesters, …)
- Slash and liquidate bonds
- Collect accumulated protocol fees
- Scan for liquidation candidates and expire stale claims
- Propose and execute treasury withdrawals

Every operation an operator submits consumes XLM from the signing account and
may affect the bond contract's fee pool or the treasury balance.

---

## 2. The Three Balance Pools

### 2.1 Signing-account XLM

The Stellar account that signs transactions must maintain:

| Requirement | Amount | Notes |
|---|---|---|
| Base reserve | 10 XLM | Minimum for any Stellar account |
| Transaction fee | Determined by Soroban `simulateTransaction` | Varies per entrypoint (see §3) |
| Storage rent | Embedded in the resource fee | `bump_instance_ttl` extends TTL on every call |

If the balance drops below the sum of (base reserve + next transaction's
resource fee) the network will reject the transaction with
`tx_insufficient_balance`.

### 2.2 Bond-contract fee pool

The bond contract accumulates protocol fees on every `create_bond` call:

```
fee = bond_amount * fee_bps / 10_000
```

Fees sit in the contract until an operator calls `collect_fees`, which
transfers them to the treasury. Monitoring the fee pool size and collecting
before the contract is upgraded or migrated is the operator's responsibility.

### 2.3 Treasury balance

The treasury ([treasury.md](treasury.md)) holds all collected protocol fees and
slashed funds, tracked by source (`ProtocolFee`, `SlashedFunds`). Withdrawals
require a multi-sig proposal + threshold approvals.

The treasury enforces a **minimum liquidity floor** (`min_liquidity`) — no
withdrawal may reduce the balance below this level. Operators who are treasury
signers must check `get_min_liquidity()` before proposing a withdrawal.

---

## 3. Per-Operation Resource Costs

All values are from `budget-ceilings.md` (bond contract) and integration tests.
Multiply the CPU ceiling by the network's per-instruction fee to estimate XLM
cost.

| Entrypoint | CPU ceiling | Mem ceiling | Auth required | Notes |
|---|---|---|---|---|
| `create_bond` | 200 M | 4 MB | Owner | Fee deducted from bonded amount |
| `top_up` | 200 M | 4 MB | Owner | |
| `request_withdrawal` | 200 M | 4 MB | Owner | |
| `withdraw` | 200 M | 4 MB | Owner | |
| `withdraw_early` | 200 M | 4 MB | Owner | Penalty path |
| `slash_bond` | 200 M | 4 MB | Admin | + idempotency key storage |
| `liquidate` | 200 M | 4 MB | Admin | |
| `collect_fees` | 200 M | 4 MB | Admin | Transfers fee pool → treasury |
| `add_attestation` (normal) | 400 M | 6 MB | Attester | |
| `add_attestation` (max) | 600 M | 10 MB | Attester | 64-byte payload, 20 subjects |
| `renew_if_rolling` | 200 M | 4 MB | Owner | |
| `set_early_exit_config` | ~50 M | ~1 MB | Admin | CLI-dispatchable |
| `set_weight_config` | ~50 M | ~1 MB | Admin | CLI-dispatchable |
| `set_pause_signer` | ~50 M | ~1 MB | Admin | CLI-dispatchable |
| `scan_liquidation_candidates` | ~100 M | ~2 MB | Keeper | Paginated; cost ≈ page size |
| `expire_claims` | ~100 M | ~2 MB | None | Hard-capped at 50 claims |
| `approve_withdrawal` | ~50 M | ~1 MB | Signer | Treasury multi-sig |
| `execute_withdrawal` | ~100 M | ~2 MB | None | Anyone once threshold met |

> **Real costs vary** — the table shows 2×-baseline ceilings from
> `test_budget_helper.rs`. Use `simulateTransaction` for accurate cost before
> submitting. The `credence-admin` CLI (see [admin-cli.md](admin-cli.md)) runs
> simulation automatically in `--submit` mode.

---

## 4. XLM Balance Health

### 4.1 Calculate minimum balance

```
minimum_balance = base_reserve(10 XLM)
                + max_daily_transactions × avg_tx_fee
                + buffer(20 %)
```

For a bot submitting 50 admin transactions/day at ~0.01 XLM each:

```
minimum = 10 + (50 × 0.01) × 1.2 = 10.6 XLM
```

### 4.2 Monitor

Check the signing account balance on StellarExpert or via RPC:

```sh
curl -X POST https://soroban-testnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":"1","method":"getAccount","params":{"accountId":"G…"}}'
```

Alert when balance drops below `minimum_balance + 5 XLM` so there is time to
refill before transactions fail.

### 4.3 Every call extends TTL

Every entrypoint calls `bump_instance_ttl(&e)` as its first statement,
extending the contract instance storage to ~1 year from now. This cost is
bundled into the resource fee returned by `simulateTransaction` — the operator
does not need to schedule separate TTL-maintenance transactions.

---

## 5. Fee Pool Health

### 5.1 Check the pool

```rust
// The bond contract stores accumulated fees. There is no public getter
// for the raw pool balance on every deployment, but the amount transferred
// by collect_fees is visible in the emitted event.
```

Monitor `bond_creation_fee` events from the bond contract. Each event carries
`(identity, bond_amount, fee_amount, treasury)`.

### 5.2 Collect fees

Call `collect_fees` when the pool is worth the gas cost:

```sh
credence-admin \
  --contract C… \
  --signer   S… \
  --submit \
  bond-collect-fees \
  --admin G…
```

The entrypoint is reentrancy-guarded and accepts an `idempotency_salt` so
retries are safe.

### 5.3 Fee configuration

Check current fee rates with:

```rust
// credence_bond entrypoints:
get_protocol_fee_bps()  // default 50 (0.5 %), max 1000 (10 %)
get_attestation_fee()   // default 10 (0.1 %), max 500 (5 %)
```

---

## 6. Treasury Balance Health

### 6.1 Check the balance

```rust
get_balance()                         // total
get_balance_by_source(ProtocolFee)    // fee revenue
get_balance_by_source(SlashedFunds)   // slashed bond funds
get_min_liquidity()                   // floor that must remain
```

### 6.2 Propose a withdrawal

Only treasury signers can propose. The amount must leave at least
`min_liquidity` in the treasury.

```sh
credence-admin \
  --contract C… \
  --signer   S… \
  --submit \
  treasury-propose-withdrawal \
  --recipient G… \
  --amount    1000_0000000  # 1000 units in 7-decimal token
```

### 6.3 Approve and execute

Once `approval_count ≥ threshold`, anyone can execute:

```sh
credence-admin \
  --contract C… \
  --submit \
  treasury-execute-withdrawal \
  --proposal-id 42 \
  --min-amount-out 0
```

### 6.4 Proposal TTL

Proposals expire after `proposal_ttl` (default 7 days). Expired proposals
cannot be approved or executed. The operator should either execute before
expiry or have the proposer re-submit.

---

## 7. Keeper Operations

Some operations are permissionless but are typically run by operators.

### 7.1 Liquidation scanning

`scan_liquidation_candidates` is paginated. A keeper workflow:

```
cursor = 0
loop:
  result = scan_liquidation_candidates(keeper, cursor, max_iter)
  for each candidate in result.candidates:
    liquidate(candidate)
  if result.done: break
  cursor = result.next_cursor
```

- `max_iter` controls the page size (and therefore the per-transaction cost).
  Start with 10 and adjust based on the Soroban simulation results.
- The keeper cursor is tamper-resistant — you cannot skip positions.
- See [liquidation_scanner.rs](../contracts/credence_bond/src/liquidation_scanner.rs).

### 7.2 Claim expiry sweep

`expire_claims` processes up to 50 expired claims per call. It is
permissionless and has no cursor — call it periodically to keep storage lean.

---

## 8. Storage TTL Maintenance

All contracts self-manage their TTL (see [storage-ttl.md](storage-ttl.md)):

- **Instance storage**: Extended to ~1 year on every public entrypoint call.
  As long as the contract receives traffic, instance storage stays alive.
- **Persistent storage**: Claims, delegations, nonces, and audit records are
  expiry-aware. The TTL is computed from the record's `expires_at` timestamp.

Operators do **not** need to schedule separate TTL maintenance transactions.
However, if a contract goes untouched for >6 months, its instance storage may
be archived. Operators running periodic health-check calls (e.g. `version()`)
prevent this.

---

## 9. Operational Checklist

### Daily

- [ ] Check signing-account XLM balance is above minimum threshold
- [ ] Review pending treasury proposals nearing TTL expiry
- [ ] Run one page of `scan_liquidation_candidates`

### Weekly

- [ ] Collect accumulated fees from the bond contract (`collect_fees`)
- [ ] Execute approved treasury withdrawals before TTL expiry
- [ ] Run `expire_claims` to sweep expired claim storage

### Monthly

- [ ] Review protocol fee rate and attestation fee against operational costs
- [ ] Verify treasury `min_liquidity` is still appropriate
- [ ] Rotate signing keys if any showed degraded performance
- [ ] Run a full pass of liquidation scanning across all bond holders

### On parameter change

- [ ] Verify the new `fee_bps` and `min_liquidity` still cover operational XLM
      costs (estimate via `simulateTransaction` on each entrypoint)

---

## 10. Related Documents

- [admin-cli.md](admin-cli.md) — CLI usage and dry-run mode
- [admin-roles.md](admin-roles.md) — role hierarchy and permissions
- [budget-ceilings.md](budget-ceilings.md) — per-entrypoint resource budgets
- [fees.md](fees.md) — bond creation fee mechanism
- [treasury.md](treasury.md) — treasury multi-sig withdrawal flow
- [storage-ttl.md](storage-ttl.md) — storage TTL policy across all contracts
- [fund-flow.md](fund-flow.md) — token custody trace
- [liquidation.md](liquidation.md) — bond liquidation flow
