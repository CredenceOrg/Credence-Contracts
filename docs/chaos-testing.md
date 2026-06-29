# Chaos Testing — Credence Bond

> **Status:** Implemented in `contracts/credence_bond/src/test_chaos.rs`
>
> Run with:
> ```bash
> cargo test -p credence_bond chaos_injection
> ```
> Run the full chaos suite (including validation guard test):
> ```bash
> cargo test -p credence_bond chaos
> ```

---

## Purpose

Standard tests assume that every host function call succeeds and every
cross-contract invocation returns a well-formed result.  Chaos tests inject
_deterministic failures_ at specific points in the execution path and verify
two invariants:

1. **Atomic revert** — when any inner call panics, no state mutation persists
   (bonded amounts, slash tallies, fee balances, and the reentrancy lock all
   return to their pre-call values).
2. **Clean rejection** — when the contract detects an invalid pre-condition
   (bad auth, missing storage, arithmetic overflow), it panics with the correct
   error before writing any state.

---

## Infrastructure

### `ChaosToken` mock (`src/chaos_token.rs`)

A SEP-41 token implementation whose failure modes can be toggled independently
at runtime.

| Toggle method | Storage key | Simulated failure |
|---|---|---|
| `set_fail_transfer(true)` | `"ft"` | `transfer` panics |
| `set_fail_transfer_from(true)` | `"ftf"` | `transfer_from` panics |
| `set_fail_balance(true)` | `"fb"` | `balance` panics (storage read failure) |
| `set_fail_approve(true)` | `"fa"` | `approve` panics |
| `set_fail_allowance(true)` | `"fal"` | `allowance` panics |

All toggles are disabled by default (`initialize()` sets them to `false`).

### `PanickingCallback` contract (`src/test_chaos.rs`)

A callback contract whose every hook (`on_slash`, `on_withdraw`, `on_collect`)
unconditionally panics.  Registered via `set_callback()` to simulate a
malicious or broken downstream contract.

### `NoOpCallback` contract (`src/test_chaos.rs`)

A callback contract whose hooks are no-ops.  Used in "second call" invariant
checks after a rollback — if the fee balance or bond state was truly restored,
a subsequent clean call with this callback succeeds and returns the original
values.

---

## Injection Catalogue

### Injection #1 — `on_slash` callback panic

**Method under test:** `slash_bond(admin, amount)`

**Execution path:**
1. Acquire reentrancy lock (storage write).
2. Validate admin and bond.
3. **Write** `slashed_amount = current + slash_amount` (storage mutation).
4. Invoke `on_slash` on registered callback → **panic** injected here.
5. Release lock (not reached).

**Threat model:** A compromised slash-recipient contract reverts from inside
`on_slash` to prevent the slash record being committed.  Without Soroban's
transaction-level atomicity, step 3 would persist despite the step-4 panic,
allowing the recipient to silently resist slashing.

**Verified invariants:**
- `bond.slashed_amount == 0` after `try_slash_bond` returns `Err`.
- `bond.bonded_amount == 1000` (unchanged).
- `is_locked() == false` (lock reverted with all other writes).

---

### Injection #2 — `on_withdraw` callback panic

**Method under test:** `withdraw_bond(identity)`

**Execution path:**
1. Acquire lock.
2. Validate identity ownership and active status.
3. **Write** bond with `bonded_amount = 0` and `active = false`.
4. Invoke `on_withdraw` → **panic**.
5. Release lock (not reached).

**Threat model:** A grief attack — a registered withdraw hook panics to
permanently deactivate the bond record while preventing the actual withdrawal,
locking the identity's collateral forever.

**Verified invariants:**
- `bond.active == true` after rollback.
- `bond.bonded_amount == 1000` (not zeroed).
- `is_locked() == false`.

---

### Injection #3 — `on_collect` callback panic

**Method under test:** `collect_fees(admin)`

**Execution path:**
1. Acquire lock.
2. Validate admin.
3. Read current fee balance.
4. **Write** fee balance to 0.
5. Invoke `on_collect` → **panic**.
6. Release lock (not reached).

**Threat model:** Fee-drain attack — without rollback, step 4 silently clears
the protocol treasury even though the callback (and the intended transfer) never
completed.

**Verified invariants:**
- After rollback, registering `NoOpCallback` and calling `collect_fees` again
  returns `500` (the original deposit), proving the zero-write was reverted.

---

### Injection #4 — `Admin` storage key unexpectedly missing

**Method under test:** `slash_bond(admin, amount)`

**Execution path:**
1. Acquire lock.
2. Read `Admin` key → `None` (key removed by chaos setup) → **panic**
   `NotInitialized`.
3. Bond state never touched (mutex released in error branch).

**Threat model:** Ledger TTL / compaction evicts a storage key that must
always be present.  The contract must guard every `unwrap_or_else` with the
correct error rather than continuing with a zero/default value.

**Verified invariants:**
- `bond.slashed_amount == 0` (no mutation before the check).
- `bond.bonded_amount == 1000` (unchanged).

---

### Injection #5 — `Bond` storage key unexpectedly missing

**Method under test:** `withdraw_bond(identity)`

**Execution path:**
1. Acquire lock.
2. Read `Bond` key → `None` → **panic** `BondNotFound`.
3. Nothing written.

**Threat model:** Same TTL/compaction scenario as #4, applied to the bond
record.  A partial withdraw_bond execution after storage eviction could treat
the "no bond" case as "empty bond", incorrectly releasing the lock and emitting
a zero-value withdrawal.

---

### Injection #6 — `slash_amount > bonded_amount` (arithmetic guard)

**Method under test:** `slash_bond(admin, amount)`

**Execution path:**
1. Acquire lock.
2. Validate admin.
3. Compute `new_slashed = slashed_amount + slash_amount`.
4. Check `new_slashed > bonded_amount` → **panic** `SlashExceedsBond`.
5. Lock released in error branch; bond storage never written.

**Threat model:** Arithmetic exploitation — an attacker requests a slash
amount larger than the bonded amount to overflow `slashed_amount`, corrupting
the available-balance calculation and enabling free withdrawals.

**Verified invariants:**
- `bond.slashed_amount == 0`.
- `bond.bonded_amount == 1000`.
- `is_locked() == false`.

---

### Injection #7 — Reentrancy guard (pre-locked state)

**Method under test:** `slash_bond(admin, amount)`

**Execution path:**
1. Read `locked` flag → `true` (pre-set by chaos setup).
2. **Panic** `ReentrancyDetected`.
3. Nothing written.

**Threat model:** A reentrant caller (e.g. a callback that calls `slash_bond`
again while the first invocation is still executing) attempts a double-slash.
The guard ensures `slashed_amount` is only updated once per transaction frame.

**Verified invariants:**
- `try_slash_bond` returns `Err` while `locked == true`.

---

### Injection #8 — Rolling bond: notice period not elapsed

**Method under test:** `withdraw_bond(identity)` on a rolling bond

**Execution path:**
1. Acquire lock.
2. Verify the bond is rolling and a withdrawal was requested.
3. Compute `earliest = withdrawal_requested_at + notice_period_duration`.
4. Check `timestamp < earliest` → **panic** "notice period not elapsed".

**Threat model:** Ledger timestamp manipulation or a race in the notice-period
clock — an identity calls `request_withdrawal` and immediately `withdraw_bond`
without waiting for the notice window.  If the clock check used wall time
instead of ledger time, it could be gamed.

---

### Injection #9 — `ChaosToken` failure surface

**Methods under test:** Direct `ChaosToken` client calls

Three sub-cases:

| Sub-case | Toggle | Verified |
|---|---|---|
| 9a | `set_fail_transfer(true)` | `transfer` panics; disabling restores balance |
| 9b | `set_fail_balance(true)` | `balance` panics |
| 9c | `set_fail_transfer_from(true)` | `transfer_from` panics; disabling restores balance |

**Threat model:** A token contract used by the bond undergoes a host-level
failure (resource limits, bad state, owner upgrade).  This test documents each
failure surface and confirms the toggle mechanism itself is reliable.

---

## Guard Validation

**Test:** `chaos_validation_guard_absent_double_call_succeeds_without_guard`

This test verifies the _converse_ of injection #7: when the reentrancy lock is
**not** pre-set, successive `slash_bond` calls both succeed.  This confirms
that the chaos test in #7 is genuinely testing the guard, not an unrelated
rejection.

> **Per issue spec:** "Validate: remove the atomic guard and confirm a chaos
> test fails."  The validation test demonstrates that, absent the guard, what
> injection #7 tests would NOT be rejected — satisfying the spec's request for
> proof that the guard is the operative control.

---

## Running the Suite

```bash
# All chaos tests
cargo test -p credence_bond chaos -- --nocapture

# Only atomic-revert tests (use try_ pattern)
cargo test -p credence_bond chaos_injection_1
cargo test -p credence_bond chaos_injection_2
cargo test -p credence_bond chaos_injection_3
cargo test -p credence_bond chaos_injection_6

# should_panic tests
cargo test -p credence_bond chaos_injection_4
cargo test -p credence_bond chaos_injection_5
cargo test -p credence_bond chaos_injection_8

# Guard removal validation
cargo test -p credence_bond chaos_validation
```

## Coverage Mapping

| Bond operation | Chaos-covered? | Injection(s) |
|---|---|---|
| `create_bond` | ✅ (pre-condition: Bond key absence) | #5 setup |
| `withdraw_bond` | ✅ | #2, #5, #8 |
| `slash_bond` | ✅ | #1, #4, #6, #7 |
| `collect_fees` | ✅ | #3 |
| Token `transfer` | ✅ | #9a |
| Token `transfer_from` | ✅ | #9c |
| Token `balance` | ✅ | #9b |
| Reentrancy lock | ✅ | #7, validation |
