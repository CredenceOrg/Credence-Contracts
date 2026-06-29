# Emergency Drain Runbook — `emergency_drain_to_treasury`

> **Audience**: On-call engineers, security leads, contract administrators.  
> **Last updated**: 2026-06-23  
> **Relevant contract**: `credence_bond`  
> **Issue tracker reference**: #246 (emergency mode), this runbook covers the
> drain extension introduced in feature/bond-emergency-drain.

---

## 1. Purpose

In a catastrophic incident the admin may need to drain residual USDC from the
bond contract to the treasury for safekeeping **while the contract is paused**.
`emergency_drain_to_treasury(admin, amount, recipient)` is the controlled,
auditable path for this operation.

Normal `emergency_withdraw` (issue #246) is for identity-level withdrawals;
`emergency_drain_to_treasury` is for admin-initiated bulk residual drains.

---

## 2. Security gates

All of the following must hold **simultaneously**.  Failure at any gate aborts
the transaction with a descriptive error — no partial state changes.

| # | Gate | Error on failure |
|---|------|-----------------|
| 1 | **Contract is paused** (`is_paused == true`) | `EmergencyDrainNotPermitted` |
| 2 | **Timelock elapsed** — `schedule_emergency_drain` was called ≥ 24 h ago | `TimelockNotReady` |
| 3 | **Admin auth** — caller is the stored admin and has signed | `NotAdmin` |
| 4 | **Recipient is treasury** — `recipient == emergency_config.treasury` | panic: "recipient must be treasury" |
| 5 | **Positive amount** — `amount > 0` | panic: "amount must be positive" |

---

## 3. Step-by-step runbook

### Step 1 — Declare incident and pause the contract

```bash
# Verify current state
soroban contract invoke --id $BOND_ID -- is_paused

# Pause (only admin can do this)
soroban contract invoke --id $BOND_ID -- pause \
  --caller $ADMIN_ADDRESS
```

Verify the contract is paused:
```bash
soroban contract invoke --id $BOND_ID -- is_paused
# Expected: true
```

### Step 2 — Schedule the drain

The drain has a **24-hour mandatory delay** (enforced on-chain).  Scheduling
records an ETA = `now + delay`.  Minimum delay: 86 400 seconds.

```bash
soroban contract invoke --id $BOND_ID -- schedule_emergency_drain \
  --admin $ADMIN_ADDRESS \
  --delay 86400
```

Confirm the ETA:
```bash
soroban contract invoke --id $BOND_ID -- get_drain_eta
# Expected: a timestamp ~24 h in the future
```

Record the ETA timestamp.  **Do not proceed before this time.**

### Step 3 — Wait for the timelock window

Wait until the current ledger timestamp ≥ scheduled ETA (at least 24 hours).
You can re-check:

```bash
soroban contract invoke --id $BOND_ID -- get_drain_eta
```

Cross-reference with the current Stellar ledger time (Horizon or CLI):
```bash
stellar ledger info --network mainnet
```

### Step 4 — Verify the treasury address

Before executing, confirm the treasury address stored in the contract matches
your intended destination:

```bash
soroban contract invoke --id $BOND_ID -- get_emergency_config
# Check the `treasury` field
```

> [!CAUTION]
> The `recipient` argument to `emergency_drain_to_treasury` **must exactly
> match** `emergency_config.treasury`.  Any other address causes an immediate
> revert.  There is no override path.

### Step 5 — Execute the drain

```bash
soroban contract invoke --id $BOND_ID -- emergency_drain_to_treasury \
  --admin $ADMIN_ADDRESS \
  --amount $AMOUNT_USDC_STROOPS \
  --recipient $TREASURY_ADDRESS
```

A successful call returns the **drain record id** (monotonic integer starting
at 1).

### Step 6 — Verify the audit record

```bash
DRAIN_ID=<returned_id>

soroban contract invoke --id $BOND_ID -- get_drain_record \
  --id $DRAIN_ID
```

Expected fields:

| Field | Expected value |
|-------|---------------|
| `id` | Monotonic (1, 2, 3 …) |
| `amount` | Matches `--amount` |
| `recipient` | Treasury address |
| `admin` | Admin address |
| `timestamp` | Ledger time at execution |
| `scheduled_eta` | The ETA from step 2 |

### Step 7 — Post-drain actions

- Confirm the treasury balance has increased by `amount`.
- File a post-incident report referencing the drain record id.
- Decide whether to unpause the contract or leave it paused pending a fix.

```bash
# To unpause when safe:
soroban contract invoke --id $BOND_ID -- unpause \
  --caller $ADMIN_ADDRESS
```

---

## 4. Cancelling a pending drain

If the incident is resolved before the timelock elapses, cancel the scheduled
drain:

```bash
soroban contract invoke --id $BOND_ID -- cancel_emergency_drain \
  --admin $ADMIN_ADDRESS
```

Verify cancellation:
```bash
soroban contract invoke --id $BOND_ID -- get_drain_eta
# Expected: null / None
```

---

## 5. Multiple sequential drains

After each successful drain the on-chain ETA is **cleared**.  A second drain
requires a new `schedule_emergency_drain` call (another 24-hour wait).  This is
intentional — it prevents a single compromised key from draining the entire
contract in one transaction.

---

## 6. Event log

Every successful drain emits an `emergency_drain` event:

```
topic:  ("emergency_drain", drain_id, admin_address)
data:   (amount, recipient_address, timestamp)
```

Monitor your event stream / Horizon endpoint for this event during and after an
incident.

---

## 7. Audit trail query

List all drain records by iterating from id 1 to `get_latest_drain_id`:

```bash
LATEST=$(soroban contract invoke --id $BOND_ID -- get_latest_drain_id)
for i in $(seq 1 $LATEST); do
  soroban contract invoke --id $BOND_ID -- get_drain_record --id $i
done
```

Records are stored in **persistent storage** and survive contract upgrades.

---

## 8. Error reference

| Error | Meaning | Action |
|-------|---------|--------|
| `EmergencyDrainNotPermitted` (113) | Contract not paused, or no ETA scheduled | Call `pause` first; ensure `schedule_emergency_drain` was called |
| `TimelockNotReady` (112) | ETA not yet reached | Wait until ledger timestamp ≥ ETA |
| `NotAdmin` (100) | Caller is not the configured admin | Use correct admin keypair |
| `NotInitialized` (1) | Contract not initialized | Contact protocol team |
| panic: "recipient must be treasury" | `recipient` ≠ `emergency_config.treasury` | Use the treasury address from `get_emergency_config` |
| panic: "amount must be positive" | `amount ≤ 0` | Specify a positive USDC stroop amount |

---

## 9. Security notes

- The 24-hour minimum delay gives time to **detect and cancel** a rogue drain
  scheduled by a compromised key.
- The treasury-only recipient constraint ensures funds can only go to the
  pre-authorised multisig treasury, not an arbitrary attacker address.
- Each drain is a separate immutable ledger entry — there is no way to delete
  or modify past records.
- All state changes are atomic: if the token transfer fails the ETA is not
  cleared and the record is not written.

---

*See also: `docs/emergency-mode.md` (issue #246), `contracts/credence_bond/src/emergency_drain.rs`.*
