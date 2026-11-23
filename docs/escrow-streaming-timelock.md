# Escrow, Streaming, and Timelock-Release Patterns

Audience: **contributors** working on or reviewing `contracts/credence_bond`,
`contracts/credence_treasury`, and `contracts/timelock`.

This document explains how the three core fund-custody patterns used in Credence
Contracts work, where each pattern lives in the codebase, and how they interact.
Read it before touching any entrypoint that moves tokens or schedules a
protocol change.

---

## Table of Contents

1. [Escrow — Bond Custody](#1-escrow--bond-custody)
2. [Streaming — Time-Proportional Release](#2-streaming--time-proportional-release)
3. [Timelock-Release — Delayed Administrative Execution](#3-timelock-release--delayed-administrative-execution)
4. [How the Three Patterns Connect](#4-how-the-three-patterns-connect)
5. [Cross-Reference Map](#5-cross-reference-map)

---

## 1. Escrow — Bond Custody

### What it is

A bond is an escrow: a participant deposits tokens into the contract at creation
time, and those tokens are held under the contract's address until an explicit
release (withdrawal or liquidation) or enforcement (slash) event fires.

The contract is the custodian. The depositor retains *in-protocol* ownership of
the unslashed balance, but has **no direct token-transfer right** until the
lock-up passes or a rolling-bond notice period elapses.

### Where it lives

| File | Entrypoints |
|------|-------------|
| `contracts/credence_bond/src/lib.rs` | `create_bond`, `top_up`, `withdraw`, `withdraw_bond`, `withdraw_early` |
| `contracts/credence_bond/src/token_integration.rs` | `transfer_in`, `transfer_out` (internal helpers) |
| `contracts/credence_bond/src/safe_token.rs` | balance-delta verification wrappers |

### Lifecycle

```
Depositor
    │
    │  create_bond(identity, amount, duration, is_rolling, notice_period)
    │       ↓
    │  token_client.transfer(caller → contract)      ← tokens enter escrow
    │
    │  [lock-up period, slashing window, notice period]
    │
    │  withdraw_bond(amount)  or  withdraw_early(amount)
    │       ↓
    │  token_client.transfer(contract → identity)    ← tokens leave escrow
```

### Concrete example (fixed-duration bond)

The test in `src/test_create_bond.rs` that sets up a standard escrow looks like
this:

```rust
// contracts/credence_bond/src/test_create_bond.rs
env.mock_all_auths();
let bond_amount: i128 = 1_000_000;          // 1 USDC (6 decimals)
let duration: u64 = 30 * 86_400;            // 30 days

client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);

// Tokens are now held by the contract. Identity cannot withdraw yet.
let bond = client.get_bond(&identity);
assert_eq!(bond.bonded_amount, bond_amount);
assert!(bond.active);

// Advance time past lock-up
env.ledger().with_mut(|li| li.timestamp = bond.bond_start + duration);

// Now withdrawal releases the escrow
client.withdraw_bond(&bond_amount);
```

### Key invariants (see `src/invariants.rs`)

- `bonded_amount >= slashed_amount` always holds.
- Available balance = `bonded_amount - slashed_amount`.
- Withdrawal amount ≤ available balance.
- A bond can only be withdrawn or liquidated when `active == true`.

### Partial withdrawal

`withdraw_bond` supports partial releases. The caller can drain the available
balance incrementally across multiple ledgers. Each call:

1. Checks the lock-up / notice constraints.
2. Deducts `amount` from `bonded_amount` (state write — CEI pattern).
3. Calls `token_client.transfer(contract → identity, amount)`.

Partial slash-then-withdraw sequences are covered by `test_slashing.rs`.

### Fee-on-transfer token guard

`token_integration.rs` records the contract's balance *before* and *after* every
inbound transfer and rejects any token that delivers less than the requested
amount. This prevents fee-on-transfer token attacks at the escrow entry point.

---

## 2. Streaming — Time-Proportional Release

Credence does not implement a push-based streaming contract (i.e. it does not
drip tokens to recipients on each ledger). Instead it uses a **pull-based
time-proportional penalty model**: the longer an identity stays in the bond, the
smaller the penalty fraction when they exit early. The net effect for the
identity is equivalent to a linearly vesting position.

### Where it lives

| File | Entrypoints |
|------|-------------|
| `contracts/credence_bond/src/early_exit_penalty.rs` | `calculate_penalty` |
| `contracts/credence_bond/src/lib.rs` | `withdraw_early`, `set_early_exit_config` |

### Penalty formula

```
penalty = (amount × penalty_bps / 10_000) × (remaining_time / total_duration)
```

Where:
- `remaining_time = bond_end - now`
- `total_duration = bond_duration` (set at creation)
- `penalty_bps` is the early-exit rate configured by admin, 0–10 000 (0–100 %).

As `now` approaches `bond_end`, `remaining_time → 0` and `penalty → 0`. The
identity's *effective accessible value* therefore grows linearly over the bond
period — the streaming behaviour.

### Concrete example

```rust
// contracts/credence_bond/src/test_early_exit_penalty.rs
let bond_amount: i128 = 1_000_000;   // 1 USDC
let duration: u64 = 365 * 86_400;    // 1 year
let penalty_bps: u32 = 1_000;        // 10 %

client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
client.set_early_exit_config(&admin, &treasury, &penalty_bps);

// Advance halfway through the bond
env.ledger().with_mut(|li| {
    li.timestamp = bond_start + duration / 2;
});

// Withdraw early: penalty ≈ 5 % (10 % rate × 50 % time remaining)
client.withdraw_early(&(bond_amount / 2));
// identity receives ~475 000 stroops, treasury receives ~25 000 stroops
```

### Treasury routing

The penalty fraction is transferred to the configured treasury address as a
`ProtocolFee`-tagged deposit. The treasury's `receive_fee` entrypoint records the
inbound amount and calls `token_client.transfer` to move tokens. See
[treasury.md](treasury.md) for treasury accounting details.

```
withdraw_early(amount)
    │
    ├── penalty = calculate_penalty(amount, remaining, total, bps)
    │
    ├── token_client.transfer(contract → treasury, penalty)
    │       emits early_exit_penalty
    │
    └── token_client.transfer(contract → identity, amount - penalty)
```

Events emitted:

| Event | Payload |
|-------|---------|
| `early_exit_penalty` | `(identity, withdraw_amount, penalty_amount, treasury)` |
| `bond_fund_transfer` | `(treasury, penalty_amount, FundSource::ProtocolFee)` |

### Mutual exclusivity with normal withdrawal

`withdraw` / `withdraw_bond` guard against calling before lock-up:

```rust
// contracts/credence_bond/src/lib.rs
let end = bond.bond_start
    .checked_add(bond.bond_duration)
    .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
if now < end {
    panic_with_error!(&e, ContractError::LockupNotExpired);
}
```

`withdraw_early` guards against calling after lock-up:

```rust
if now >= end {
    panic_with_error!(&e, ContractError::LockupNotExpired);
}
```

The two paths are mutually exclusive. There is no window where both are valid or
where neither is valid. See [early-exit.md](early-exit.md) for the full matrix.

### Rolling-bond notice period as a streaming gate

For rolling bonds the "when can funds leave?" question is governed by the notice
period, not a fixed expiry. The notice period serves the same **slashing window**
function as the lock-up: it keeps funds in escrow long enough for the protocol
to detect and respond to misbehaviour before a withdrawal can complete.

```
request_withdrawal()          ← starts notice clock
    │  notice_period_duration seconds must pass
    ↓
withdraw_bond(amount)         ← escrow releases
```

`rolling_bond.rs` enforces:

```rust
let notice_end = bond.withdrawal_requested_at
    .checked_add(bond.notice_period_duration)
    .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
if now < notice_end {
    panic_with_error!(&e, ContractError::NoticeNotElapsed);
}
```

See [rolling-bonds.md](rolling-bonds.md) for the full state machine.

---

## 3. Timelock-Release — Delayed Administrative Execution

### What it is

Sensitive protocol changes (parameter updates, contract upgrades, admin
replacements) are queued on-chain and cannot execute until a mandatory delay has
passed. This gives the community a transparent observation window and the admin a
cancellation window before any change lands.

### Where it lives

| File | Entrypoints |
|------|-------------|
| `contracts/timelock/src/lib.rs` | `initialize`, `queue_operation`, `execute_operation`, `cancel_operation`, `get_operation`, `is_operation_executed` |

### Data structures

```rust
// contracts/timelock/src/lib.rs
pub struct QueuedOperation {
    pub id: u64,
    pub op_hash: BytesN<32>,   // deterministic hash of the operation payload
    pub eta: u64,              // earliest execution timestamp
    pub expires_at: u64,       // latest execution timestamp (eta + GRACE_PERIOD)
    pub status: OperationStatus,
}

pub enum OperationStatus {
    Pending   = 0,
    Executed  = 1,
    Cancelled = 2,
}
```

### Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `min_delay_seconds()` | `86_400` (24 h) | Minimum queue delay |
| `GRACE_PERIOD` | `86_400` (24 h) | Window after ETA during which execution is still valid |

### Lifecycle

```
Admin
  │
  │  queue_operation(proposer, op_hash, delay)
  │       delay >= min_delay_seconds()
  │       eta = now + delay
  │       expires_at = eta + GRACE_PERIOD
  │       ↓
  │  [Pending — visible on-chain, community can review]
  │
  │  (optional) cancel_operation(admin, op_id)
  │       status → Cancelled (terminal)
  │
  │  (after eta, before expires_at)
  │  execute_operation(op_id)
  │       status → Executed (terminal)
  │       op_hash recorded in ExecutedOp replay guard
```

### Concrete example from the test suite

```rust
// contracts/timelock/src/lib.rs — #[cfg(test)]
let env = Env::default();
env.mock_all_auths();
let admin = Address::generate(&env);
let contract_id = env.register(TimelockContract, ());
let client = TimelockContractClient::new(&env, &contract_id);
client.initialize(&admin);

let op_hash = BytesN::from_array(&env, &[0u8; 32]);
let delay: u64 = 86_400; // minimum 24-hour delay

// Queue the operation
let op_id = client.queue_operation(&admin, &op_hash, &delay);

let op = client.get_operation(&op_id).unwrap();
assert_eq!(op.status, OperationStatus::Pending);

// Attempting to execute before ETA must fail
env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
assert!(client.try_execute_operation(&op_id).is_err());

// Executing at ETA must succeed
env.ledger().with_mut(|li| li.timestamp = op.eta);
client.execute_operation(&op_id);

let op = client.get_operation(&op_id).unwrap();
assert_eq!(op.status, OperationStatus::Executed);
```

### Execution time boundaries (inclusive)

| Timestamp | `execute_operation` result |
|-----------|---------------------------|
| `now < eta` | Fails — `TimelockNotReady` |
| `now == eta` | Succeeds |
| `now == expires_at` | Succeeds |
| `now > expires_at` | Fails — `SignatureExpired` |

### Replay guard

Executed operation hashes are permanently recorded under `DataKey::ExecutedOp`.
Attempting to queue the same `op_hash` a second time fails with
`ProposalAlreadyExecuted`. This prevents reuse of stale approvals.

### Events

| Event | Topics | Body |
|-------|--------|------|
| `operation_queued` | `(tag, op_id)` | `(proposer, op_hash, eta, expires_at)` |
| `operation_executed` | `(tag, op_id)` | `op_hash` |
| `operation_cancelled` | `(tag, op_id)` | `op_hash` |

### Integrating with other contracts

The timelock is a standalone gate contract, not a proxy. The caller is responsible
for reading the queued `op_hash`, verifying it matches the intended payload off-chain,
and then dispatching the actual operation through the target contract only after
`execute_operation` confirms the delay has passed. The timelock does not hold
any tokens or call other contracts on its own.

Typical integration flow:

```
1. Admin proposes change off-chain, hashes the payload → op_hash
2. Admin calls timelock.queue_operation(op_hash, delay)
3. Indexer emits "Upcoming Change" alert with eta timestamp
4. (optional) Community reviews; admin can cancel within the window
5. After eta: anyone calls timelock.execute_operation(op_id)
6. Admin dispatches the actual contract call (upgrade, set_param, etc.)
   knowing the timelock gate has passed
```

---

## 4. How the Three Patterns Connect

```
               ┌──────────────────────────────────┐
               │         Timelock Contract         │
               │  queue → [min 24 h delay] → exec  │
               └───────────────┬──────────────────┘
                               │ gate passes: admin
                               │ dispatches change to
               ┌───────────────▼──────────────────┐
               │         Credence Bond              │
               │  create_bond → [escrow held]       │
               │  withdraw_early → penalty streaming │
               │  request_withdrawal → notice period │
               │  withdraw_bond → escrow released    │
               └───────────────┬──────────────────┘
                               │ penalty flows to
               ┌───────────────▼──────────────────┐
               │        Credence Treasury           │
               │  receive_fee (ProtocolFee/Slash)   │
               │  propose_withdrawal → [multi-sig]  │
               │  execute_withdrawal → tokens sent  │
               └──────────────────────────────────┘
```

- **Escrow** enforces custody from `create_bond` until release conditions are met.
- **Streaming** determines how much of that escrowed value is accessible early
  (the penalty decreases linearly as time passes).
- **Timelock** governs when administrative changes — including updates to penalty
  rates, notice periods, or slashing parameters — can take effect.

A common reviewer question is *"can the admin change the penalty rate mid-bond to
retroactively penalise withdrawals?"* The timelock's 24-hour minimum delay means
that even if admin queues a parameter change immediately after a bond is created,
the depositor has at least 24 hours of advance notice before the new parameters
could apply to their next call.

---

## 5. Cross-Reference Map

| Topic | Document |
|-------|----------|
| Bond state machine | [bond-state-transitions.md](bond-state-transitions.md) |
| Rolling-bond notice period | [rolling-bonds.md](rolling-bonds.md) |
| Early-exit penalty detail | [early-exit.md](early-exit.md) |
| Token custody trace | [fund-flow.md](fund-flow.md) |
| Treasury multi-sig withdrawals | [treasury.md](treasury.md) |
| Timelock function reference | [timelock.md](timelock.md) |
| Slashing | [slashing.md](slashing.md) |
| Cooldown | [cooldown.md](cooldown.md) |
| Known simplifications | [known-simplifications.md](known-simplifications.md) |

### Relevant source files

| Source file | Pattern |
|-------------|---------|
| `contracts/credence_bond/src/lib.rs` | Escrow creation and release |
| `contracts/credence_bond/src/early_exit_penalty.rs` | Streaming penalty formula |
| `contracts/credence_bond/src/rolling_bond.rs` | Notice-period streaming gate |
| `contracts/credence_bond/src/token_integration.rs` | Balance-delta inbound guard |
| `contracts/credence_bond/src/safe_token.rs` | Transfer helpers (CEI wrappers) |
| `contracts/credence_bond/src/invariants.rs` | Escrow invariant assertions |
| `contracts/credence_treasury/src/treasury.rs` | Penalty receipt and multi-sig withdrawal |
| `contracts/timelock/src/lib.rs` | Queue/execute/cancel + replay guard |
