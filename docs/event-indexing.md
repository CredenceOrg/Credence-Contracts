# Event Indexing & Consumer Guidance (v2)

This document provides technical specifications for backend services indexing events from the Credence protocol.

## 1. Event Versioning Strategy

The protocol is currently transitioning from **v1 (Legacy)** to **v2 (Indexer-Grade)**.

| Feature            | v1 (Legacy)                 | v2 (High-Fidelity)                                        |
| :----------------- | :-------------------------- | :-------------------------------------------------------- |
| **Identification** | Topic 0 is a general name.  | Topic 0 identifies the event; Topic 1 & 2 are keys.       |
| **Data Types**     | Variable/Mixed.             | Normalized (usually `i128` pairs or specialized structs). |
| **Filtering**      | Requires full data parsing. | Filterable at the ledger level via topics.                |

**Backend Recommendation:** Consumers should support "Double-Read" logic during the migration period or implement a version-aware parser that checks Topic 0 for the `_v2` suffix or the specific new Symbol name (e.g., `param_updated` vs `parameter_changed`).

---

## 2. Parameter Updates (`param_updated`)

The most critical events for protocol health.

### Topic Structure (Indexed)

1. **Event Name:** `param_updated` (Symbol)
2. **Parameter Key:** Specific identifier (e.g., `fee_prot`, `max_lev`, `th_gold`)
3. **Category:** Grouping for filtering (e.g., `fee`, `risk`, `tier`, `cooldown`)
4. **Admin:** The `Address` that authorized the change.

### Data Payload (Unindexed)

- `old_value`: `i128`
- `new_value`: `i128`

**Indexing Tip:** Use the **Category** topic to build specialized dashboards. For example, a "Risk Dashboard" should only subscribe to events where Topic 2 == `risk`.

---

## 3. Recommended Keys & Symbols

To maintain consistency across the ecosystem, use these standardized `symbol_short!` keys:

### Fee Category (`fee`)

- `fee_prot`: Protocol-wide fees.
- `fee_att`: Attestation/Validator fees.

### Risk Category (`risk`)

- `max_lev`: Maximum allowed leverage.
- `slsh_p`: Slashing penalty percentages.

### Tier Category (`tier`)

- `th_brnz`, `th_slvr`, `th_gold`, `th_plat`: Collateral/Bond thresholds.

---

## 4. Idempotency & Reliable Processing

To avoid double-counting or missing events during re-orgs or service restarts:

1. **The Unique Identity:** Every event's unique ID is a combination of:
   `LedgerSequence` + `TransactionHash` + `EventIndexWithinTx`
2. **Order of Truth:** Always use the `LedgerTimestamp` provided in the event data as the canonical time of the state change.
3. **Re-org Handling:** Only mark an event as "Final" after it has reached a depth of 12+ ledgers (Standard Stellar Finality).

---

## 5. Backend Schema Example (JSON)

When indexing into a database (PostgreSQL/MongoDB), normalize to this structure:

```json
{
  "contract": "C...",
  "version": "v2",
  "event_type": "param_updated",
  "meta": {
    "key": "fee_prot",
    "category": "fee",
    "admin": "G..."
  },
  "values": {
    "old": "50",
    "new": "100",
    "delta": "50"
  },
  "blockchain": {
    "ledger": 123456,
    "tx_hash": "...",
    "timestamp": 1713985850
  }
}
```
