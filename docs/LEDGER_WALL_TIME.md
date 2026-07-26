# Ledger Time vs. Wall Time

This document describes how time progresses on the Soroban ledger and how it relates to real-world "wall clock" time.

---

## Audience: Integrator

This document is written for **downstream integrators** (e.g., developers building frontends, indexers, or off-chain services) interacting with Credence contracts. It explains the nuances of ledger time to help you correctly interpret timestamps, compute expirations, and debug time-related issues in your applications.

---

## Core Concepts

In Soroban, time is represented by the ledger's timestamp (`env.ledger().timestamp()`), which returns the number of seconds since the Unix epoch (January 1, 1970 00:00:00 UTC).

While this timestamp is meant to track real-world time, it has a few important characteristics:

1. **Step-wise Progression**: Ledger time does not flow continuously. It updates discretely with every new ledger close, which occurs approximately every 5 to 6 seconds on the Stellar network. During the processing of a single ledger, all transactions share the exact same timestamp.
2. **Monotonicity**: The ledger timestamp is strictly increasing. A new ledger will always have a timestamp strictly greater than the previous ledger's timestamp.
3. **Acceptable Drift**: The consensus protocol ensures that the ledger timestamp is close to true wall clock time, but slight discrepancies (drift) can occur due to network propagation and consensus delays. You should not expect millisecond-level precision or perfect alignment with an NTP-synchronized local server clock.

---

## Examples in Practice

### Interpreting Output Timestamps

When you query a contract (for instance, reading a bond's expiration time) or observe an event, the timestamp is based on the ledger time when the transaction was applied.

**Example Request (Off-chain):**
```json
{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "getEvents",
  "params": {
    "startLedger": 1234567
  }
}
```

**Example Event Output:**
```json
{
  "type": "contract",
  "ledger": "1234567",
  "ledgerClosedAt": "2023-11-20T14:32:05Z",
  "contractId": "C...",
  "topic": ["bond_created"],
  "value": {
    "expires_at": 1700490725
  }
}
```

In this output, `1700490725` corresponds to the ledger timestamp (`env.ledger().timestamp()`) in seconds, while `ledgerClosedAt` is an ISO 8601 string of the same event. If your off-chain system compares this expiration time with its local wall clock, remember that they might differ by a few seconds. 

### Triggering Time-Sensitive Actions

If a contract specifies a `cooldown` or `expires_at` threshold, you must wait for a ledger whose timestamp is strictly greater than or equal to that threshold before submitting a transaction.

For example, if a bond expires at `1700490725`, submitting an `early_exit` transaction from your backend at exactly `1700490725` on your local machine might fail if the Stellar network is currently building a ledger with timestamp `1700490724`. 

**Best Practice**: Build a small buffer (e.g., 5-10 seconds) into your client-side logic to avoid transaction failures due to slight time drifts between your local wall clock and the ledger time.

---

## Summary for Integrators

- **Use Unix timestamps in seconds** when interacting with contract parameters.
- **Account for ledger close times (~5s)**; time does not update continuously.
- **Add a small buffer** when executing transactions right at a time boundary to tolerate network drift.
