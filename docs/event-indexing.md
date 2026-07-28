# Event Indexing Guidance for Backend Consumers

## Overview

This document describes which Credence Contract events backends should index, the recommended keys for each event, how to handle event versioning, and how to ensure idempotent processing.

Soroban events use a multi-topic scheme:
- `topic[0]`: event name (e.g. `Symbol::new("bond_created_v2")`)
- `topic[1]` and beyond: indexed fields for efficient filtering
- `data`: the event payload (unindexed, XDR-encoded)

---

## Quick Reference: Events by Contract

| Contract | Event | Priority | Notes |
|----------|-------|----------|-------|
| credence_bond | `bond_created_v2` | **MUST** | Primary source for bond genesis |
| credence_bond | `bond_increased_v2` | **MUST** | Tracks bond top-ups |
| credence_bond | `bond_withdrawn_v2` | **MUST** | Tracks all withdrawals |
| credence_bond | `bond_slashed_v2` | **MUST** | Tracks slashing actions |
| credence_bond | `bond_liquidated` | **MUST** | Final bond state |
| credence_bond | `claim_added` | Should | Reward queueing |
| credence_bond | `claims_processed` | Should | Reward claiming |
| credence_bond | `param_updated` | Should | Governance parameter changes |
| credence_bond | `admin_rotated` | Should | Admin lifecycle |
| credence_treasury | `treasury_deposit` | **MUST** | Fund inflows |
| credence_treasury | `treasury_withdrawal_executed` | **MUST** | Fund outflows |
| credence_treasury | `treasury_corridor_settled` | **MUST** | Settlement routing |
| arbitration | `dispute_created` | Should | Dispute lifecycle |
| arbitration | `dispute_resolved` | Should | Dispute outcomes |
| arbitration | `status_transition` | Optional | Detailed status audit trail |
| admin | `admin_rotated` | Should | Admin transfers |
| fixed_duration_bond | `bond_created` | Optional | Non-rolling bond variant |

---

## Events to Index

### Credence Bond Contract

#### `bond_created_v2`

**Emitted by**: `credence_bond` → `create_bond()`

**Topics**:
- `topic[0]`: `Symbol("bond_created_v2")` — event name
- `topic[1]`: `Address` — identity owner
- `topic[2]`: `i128` — initial bonded amount (indexed)
- `topic[3]`: `u64` — bond start timestamp (indexed)

**Data**: `(duration: u64, is_rolling: bool, end_timestamp: u64)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| identity | Address | Yes (topic[1]) | For per-identity queries |
| amount | i128 | Yes (topic[2]) | For amount-based slicing |
| start_timestamp | u64 | Yes (topic[3]) | For time-range queries |
| duration | u64 | No | Stored in data |
| is_rolling | bool | No | Stored in data |
| end_timestamp | u64 | No | Calculated in data |

**Recommended index keys**:
- Primary: `(identity, start_timestamp)` — reconstruct bond genesis per identity
- Secondary: `(amount, start_timestamp)` — historical amount changes
- Tertiary: `start_timestamp` — scan by ledger time

**Idempotency key**: `(ledger, txHash, eventIndex)` — global uniqueness across all events

**Example query** (TypeScript SDK):
```typescript
const events = await server.getEvents({
  startLedger: fromLedger,
  filters: [{
    type: 'contract',
    contractIds: [BOND_CONTRACT_ID],
    topics: [
      ['*', xdr.ScVal.scvSymbol('bond_created_v2')],
      ['*'], // any identity
    ],
  }],
});
```

---

#### `bond_increased_v2`

**Emitted by**: `credence_bond` → `top_up()`

**Topics**:
- `topic[0]`: `Symbol("bond_increased_v2")`
- `topic[1]`: `Address` — identity owner
- `topic[2]`: `i128` — additional amount added (indexed)
- `topic[3]`: `i128` — new total bonded amount (indexed)
- `topic[4]`: `u64` — increase timestamp (indexed)

**Data**: `(tier_changed: bool, new_tier: BondTier)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| identity | Address | Yes (topic[1]) | For per-identity filter |
| added_amount | i128 | Yes (topic[2]) | For activity analysis |
| new_total | i128 | Yes (topic[3]) | For balance reconstruction |
| timestamp | u64 | Yes (topic[4]) | For time-range queries |
| tier_changed | bool | No | Stored in data |
| new_tier | BondTier | No | Stored in data |

**Recommended index keys**:
- Primary: `(identity, timestamp)` — reconstruct balance over time
- Secondary: `new_total` — balance snapshots at key timestamps

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `bond_withdrawn_v2`

**Emitted by**: `credence_bond` → `withdraw()` or `withdraw_early()`

**Topics**:
- `topic[0]`: `Symbol("bond_withdrawn_v2")`
- `topic[1]`: `Address` — identity owner
- `topic[2]`: `i128` — amount withdrawn (indexed)
- `topic[3]`: `i128` — remaining bonded amount (indexed)
- `topic[4]`: `u64` — withdrawal timestamp (indexed)

**Data**: `(is_early: bool, penalty_amount: i128)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| identity | Address | Yes (topic[1]) | For per-identity filter |
| amount_withdrawn | i128 | Yes (topic[2]) | For activity volume |
| remaining | i128 | Yes (topic[3]) | Authoritative post-withdrawal balance |
| timestamp | u64 | Yes (topic[4]) | For time queries |
| is_early | bool | No | Penalty flag |
| penalty_amount | i128 | No | Penalty detail |

**Recommended index keys**:
- Primary: `(identity, timestamp)` — reconstruct withdrawal history
- Secondary: `(remaining, timestamp)` — balance time-series

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `bond_slashed_v2`

**Emitted by**: `credence_bond` → `slash()` or admin-initiated slashing

**Topics**:
- `topic[0]`: `Symbol("bond_slashed_v2")`
- `topic[1]`: `Address` — identity owner
- `topic[2]`: `i128` — amount slashed this call (indexed)
- `topic[3]`: `i128` — total lifetime slashed amount (indexed)
- `topic[4]`: `u64` — slash timestamp (indexed)
- `topic[5]`: `Address` — admin who performed slash (indexed)

**Data**: `(reason: String, is_full_slash: bool)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| identity | Address | Yes (topic[1]) | For identity-scoped queries |
| slash_amount | i128 | Yes (topic[2]) | Per-event slashed amount |
| total_slashed | i128 | Yes (topic[3]) | Cumulative slashed (authoritative) |
| timestamp | u64 | Yes (topic[4]) | For time-range audit |
| admin | Address | Yes (topic[5]) | For admin accountability |
| reason | String | No | Stored in data |
| is_full_slash | bool | No | Stored in data |

**Recommended index keys**:
- Primary: `(identity, timestamp)` — slash history per bond
- Secondary: `(admin, timestamp)` — audit by admin
- Tertiary: `total_slashed` — cumulative slash tracking

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `bond_liquidated`

**Emitted by**: `credence_bond` → `liquidate()`

**Topics**:
- `topic[0]`: `Symbol("bond_liquidated")`
- `topic[1]`: `Address` — identity owner

**Data**: `(residual: i128, reason: Symbol, timestamp: u64, admin: Address)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| identity | Address | Yes (topic[1]) | For per-identity finalization |
| residual | i128 | No | Swept to treasury |
| reason | Symbol | No | `"fully_slashed"` or `"expired_unrenewed"` |
| timestamp | u64 | No | Liquidation time |
| admin | Address | No | Who performed liquidation |

**Recommended index keys**:
- Primary: `(identity, timestamp)` — final settlement marker

**Idempotency key**: `(ledger, txHash, eventIndex)`

**Note**: This event marks the end of a bond's lifecycle. Exactly one per bond.

---

#### `claim_added`

**Emitted by**: `credence_bond` → internal reward queueing

**Topics**:
- `topic[0]`: `Symbol("claim_added")`
- `topic[1]`: `Address` — user who can claim

**Data**: `(claim_type: ClaimType, amount: i128, source_id: u64)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| user | Address | Yes (topic[1]) | For per-user reward lookups |
| claim_type | ClaimType | No | Type of reward |
| amount | i128 | No | Claimable amount |
| source_id | u64 | No | Event source (e.g., proposal ID) |

**Recommended index keys**:
- Primary: `(user, source_id)` — per-user reward tracking
- Secondary: `source_id` — rewards by source

**Idempotency key**: `(ledger, txHash, eventIndex)`

**Priority**: Should index (reward state machine dependency)

---

#### `param_updated`

**Emitted by**: `credence_bond` → `set_*_param()` governance setters

**Topics**:
- `topic[0]`: `Symbol("param_updated")`
- `topic[1]`: `Symbol` — parameter key (e.g., `"fee_prot"`, `"th_brnz"`)
- `topic[2]`: `Symbol` — category (`"fee"`, `"cooldown"`, `"tier"`, `"risk"`)
- `topic[3]`: `Address` — admin authorizing the change

**Data**: `(old_value: i128, new_value: i128)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| key | Symbol | Yes (topic[1]) | Which parameter changed |
| category | Symbol | Yes (topic[2]) | Grouped by category |
| admin | Address | Yes (topic[3]) | Who approved the change |
| old_value | i128 | No | Before value |
| new_value | i128 | No | After value |

**Recommended index keys**:
- Primary: `(key, timestamp)` — parameter change history
- Secondary: `(category, timestamp)` — category-level audit
- Tertiary: `(admin, timestamp)` — admin action audit

**Idempotency key**: `(ledger, txHash, eventIndex)`

**Priority**: Should index (governance audit trail)

---

### Credence Treasury Contract

#### `treasury_deposit`

**Emitted by**: `credence_treasury` → `receive_fee()`

**Topics**:
- `topic[0]`: `Symbol("treasury_deposit")`
- `topic[1]`: `Address` — depositor (fund source)

**Data**: `(amount: i128, source: FundSource)` where `FundSource` is `ProtocolFee(0)` or `SlashedFunds(1)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| depositor | Address | Yes (topic[1]) | For per-depositor volume |
| amount | i128 | No | Deposit amount |
| source | FundSource | No | Origin category |

**Recommended index keys**:
- Primary: `(source, timestamp)` — inflow by category
- Secondary: `depositor` — contributions by entity

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `treasury_withdrawal_executed`

**Emitted by**: `credence_treasury` → `execute_proposal()`

**Topics**:
- `topic[0]`: `Symbol("treasury_withdrawal_executed")`
- `topic[1]`: `u64` — proposal ID

**Data**: `(recipient: Address, min_amount_out: i128, actual_amount: i128)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| proposal_id | u64 | Yes (topic[1]) | Links to proposal event |
| recipient | Address | No | Withdrawal destination |
| min_amount_out | i128 | No | Minimum acceptable |
| actual_amount | i128 | No | Actual outflow |

**Recommended index keys**:
- Primary: `(proposal_id, timestamp)` — settlement tracking
- Secondary: `recipient` — outflows by recipient

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `treasury_corridor_settled`

**Emitted by**: `credence_treasury` → `settle()`

**Topics**:
- `topic[0]`: `Symbol("treasury_corridor_settled")`
- `topic[1]`: `Address` — destination

**Data**: `(amount: i128, actual_amount: i128, admin: Address)`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| destination | Address | Yes (topic[1]) | Settlement route |
| amount | i128 | No | Requested amount |
| actual_amount | i128 | No | Actual transferred |
| admin | Address | No | Who initiated |

**Recommended index keys**:
- Primary: `(destination, timestamp)` — per-route settlement history
- Secondary: `admin` — settlement by operator

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

### Arbitration Contract

#### `dispute_created`

**Emitted by**: `arbitration` → `create_dispute()`

**Topics**:
- `topic[0]`: `Symbol("dispute_created")`
- `topic[1]`: `u64` — dispute ID

**Data**: `creator: Address`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| dispute_id | u64 | Yes (topic[1]) | Unique dispute identifier |
| creator | Address | No | Who initiated dispute |

**Recommended index keys**:
- Primary: `dispute_id` — lookup by ID
- Secondary: `creator` — disputes by initiator

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

#### `dispute_resolved`

**Emitted by**: `arbitration` → `resolve()` (after voting concludes)

**Topics**:
- `topic[0]`: `Symbol("dispute_resolved")`
- `topic[1]`: `u64` — dispute ID

**Data**: `winning_outcome: u32`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| dispute_id | u64 | Yes (topic[1]) | Links to `dispute_created` |
| winning_outcome | u32 | No | Outcome code (0, 1, 2, etc.) |

**Recommended index keys**:
- Primary: `dispute_id` — final outcome lookup

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

### Admin Contract

#### `admin_rotated`

**Emitted by**: `admin` → `accept_ownership_transfer()` or `credence_bond` → `rotate_admin()`

**Topics**:
- `topic[0]`: `Symbol("admin_rotated")`
- `topic[1]`: `Address` — previous admin
- `topic[2]`: `Address` — new admin

**Data**: `ledger_sequence: u32`

| Field | Type | Index? | Notes |
|-------|------|--------|-------|
| previous_admin | Address | Yes (topic[1]) | Prior admin |
| new_admin | Address | Yes (topic[2]) | Current admin |
| ledger_sequence | u32 | No | Ledger at rotation |

**Recommended index keys**:
- Primary: `(new_admin, timestamp)` — current admin chain
- Secondary: `(previous_admin, timestamp)` — audit trail

**Idempotency key**: `(ledger, txHash, eventIndex)`

---

## Indexing Priority

| Priority | Event | Reason |
|----------|-------|--------|
| **MUST** | `bond_created_v2` | Sole source of truth for bond genesis; required for any bond state reconstruction |
| **MUST** | `bond_increased_v2` | Required to track bonded amount over time |
| **MUST** | `bond_withdrawn_v2` | Required to track bonded amount over time |
| **MUST** | `bond_slashed_v2` | Required to track slashed amount and total balance |
| **MUST** | `bond_liquidated` | Required to mark bond finalization |
| **MUST** | `treasury_deposit` | Required for treasury balance tracking |
| **MUST** | `treasury_withdrawal_executed` | Required for treasury outflow audit |
| **MUST** | `treasury_corridor_settled` | Required for treasury routing audit |
| Should | `claim_added` | Reward queue state dependency |
| Should | `claims_processed` | Reward distribution tracking |
| Should | `param_updated` | Governance audit trail |
| Should | `admin_rotated` | Admin lifecycle tracking |
| Should | `dispute_created` | Dispute state tracking |
| Should | `dispute_resolved` | Dispute outcome audit |
| Optional | `status_transition` | Detailed audit trail only |
| Optional | `treasury_withdrawal_proposed` | Proposal lifecycle (optional detail) |
| Optional | `treasury_proposal_expired` | Proposal cleanup (optional detail) |

---

## Event Versioning

### Current Version: v1 (all events)

All current events shown above are implicitly v1. The bond contract emits both v1 and v2 events for compatibility:
- **v1 events** (deprecated): `bond_created`, `bond_increased`, `bond_withdrawn`, `bond_slashed`
- **v2 events** (preferred): `bond_created_v2`, `bond_increased_v2`, `bond_withdrawn_v2`, `bond_slashed_v2`

**Indexers should prefer v2 events.** v1 events are emitted for backwards compatibility only and will be removed in a future release.

---

### Versioning Strategy

#### Breaking vs Non-Breaking Changes

**Non-breaking** (no version bump needed):
- Adding new optional fields to data payload
- Adding new event types
- Adding new indexed topics at the end

**Breaking** (requires version bump):
- Removing or renaming event fields
- Changing field types
- Changing topic structure (reordering, removing indexed topics)
- Changing idempotency key composition

#### Migration Strategy

When a breaking change requires a v2 event:

1. **Dual-emit window** (1 release cycle):
   - The contract emits **both** v1 and v2 events in the same transaction
   - Backends migrate during this window
   - Both indexers can coexist

2. **Migration** (during dual-emit window):
   - Indexers update to parse v2 and apply new logic
   - Tests confirm v1 and v2 produce identical state
   - Verify no duplication in de-duplication logic

3. **Deprecation** (following release):
   - v1 emission is removed
   - v1 events are marked deprecated in CHANGELOG
   - Migration window announced in advance

#### Version Check Pattern (TypeScript)

```typescript
function processEvent(event: SorobanEvent) {
  const eventName = event.topics[0];
  
  // Check version suffix or parse from symbol
  const version = eventName.includes('_v2') ? 2 : 1;
  
  switch (version) {
    case 1:
      return processV1Event(event);
    case 2:
      return processV2Event(event);
    default:
      console.warn(`Unknown event version — skipping`);
      return null;
  }
}
```

---

## Idempotent Event Processing

Soroban events can be replayed during re-indexing or node restarts. Backends **must** handle duplicate events gracefully.

### Idempotency Key Construction

The combination of `(ledger, txHash, eventIndex)` is **globally unique** for any Soroban event and is the recommended idempotency key:

```typescript
function getIdempotencyKey(event: SorobanEvent): string {
  return `${event.ledger}:${event.txHash}:${event.eventIndex}`;
}
```

- `ledger` — ledger sequence number where event was emitted
- `txHash` — transaction hash
- `eventIndex` — zero-based index of this event in the transaction

**Properties**:
- Globally unique across all events and all time
- Stable across re-indexing
- Deterministic (no random components)

---

### Upsert Pattern

Use upsert (insert or ignore + skip update) rather than plain insert:

```typescript
async function indexEvent(event: SorobanEvent) {
  const idempotencyKey = getIdempotencyKey(event);
  const parsed = parseEvent(event);
  
  await db.events.upsert({
    where: { idempotencyKey },
    create: {
      idempotencyKey,
      ...parsed,
    },
    update: {}, // no-op on duplicate
  });
}
```

**Behavior**:
- First index: row created
- Re-index (same ledger, txHash, eventIndex): row unchanged (no update)
- Re-index with new events: new rows created, old rows untouched

---

## Ledger Cursor Management

Track the last successfully processed ledger to enable resumable indexing:

```typescript
async function indexerState() {
  // After successful batch processing
  await db.indexerState.upsert({
    where: { id: 'credence_bond_indexer' },
    create: {
      id: 'credence_bond_indexer',
      lastProcessedLedger: processedLedger,
    },
    update: {
      lastProcessedLedger: processedLedger,
    },
  });
  
  // On indexer restart
  const state = await db.indexerState.findFirst({
    where: { id: 'credence_bond_indexer' },
  });
  const startLedger = (state?.lastProcessedLedger ?? 0) + 1;
  
  // Resume from cursor
  const events = await server.getEvents({
    startLedger,
  });
}
```

**Best practices**:
- Update cursor **after** successful batch commit
- Batch size: 100–1000 events (adjust based on throughput and error recovery tolerance)
- On error: do NOT advance cursor; retry from same position
- On network timeout: do NOT advance cursor; retry with exponential backoff

---

## Re-indexing

To re-index from scratch:

1. **Clear idempotency tracking** (optional):
   - Delete all rows from `events` table (or truncate)
   - Or leave rows and rely on upsert deduplication

2. **Reset cursor**:
   ```typescript
   await db.indexerState.update({
     where: { id: 'credence_bond_indexer' },
     data: { lastProcessedLedger: <deployment_ledger> - 1 },
   });
   ```

3. **Re-index from deployment ledger**:
   - Indexer will re-fetch all events from `startLedger`
   - Idempotency key prevents duplicate rows
   - Safe to run against live database

4. **Verify**:
   - Spot-check bond balances against on-chain state
   - Verify cumulative slashed amounts match treasury records
   - Validate no duplicate event rows (should have unique constraint on idempotencyKey)

---

## Contract Deployment Ledgers

| Contract | Network | Status | Deployment Ledger | Contract ID |
|----------|---------|--------|-------------------|-------------|
| credence_bond | Testnet | Active | [fill from deployment records] | [fill contract ID] |
| credence_bond | Mainnet | Planned | [TBD] | [TBD] |
| credence_treasury | Testnet | Active | [fill from deployment records] | [fill contract ID] |
| credence_treasury | Mainnet | Planned | [TBD] | [TBD] |
| arbitration | Testnet | Active | [fill from deployment records] | [fill contract ID] |
| arbitration | Mainnet | Planned | [TBD] | [TBD] |

---

## Indexer Query Patterns

### List all bonds for a user

```typescript
const events = await server.getEvents({
  startLedger: deploymentLedger,
  filters: [{
    type: 'contract',
    contractIds: [BOND_CONTRACT_ID],
    topics: [
      ['*', xdr.ScVal.scvSymbol('bond_created_v2')],
      [xdr.ScVal.scvAddress(userAddress)], // identity in topic[1]
    ],
  }],
});
```

### Track bond balance at a specific time

```typescript
const createdEvents = await db.query(
  `SELECT * FROM events WHERE eventName = 'bond_created_v2' 
   AND topics[1] = $1 AND ledger <= $2 ORDER BY ledger DESC LIMIT 1`,
  [userAddress, ledgerAtTime]
);

const increasedEvents = await db.query(
  `SELECT * FROM events WHERE eventName = 'bond_increased_v2' 
   AND topics[1] = $1 AND ledger <= $2 ORDER BY ledger DESC LIMIT 1`,
  [userAddress, ledgerAtTime]
);

const withdrawnEvents = await db.query(
  `SELECT * FROM events WHERE eventName = 'bond_withdrawn_v2' 
   AND topics[1] = $1 AND ledger <= $2 ORDER BY ledger DESC LIMIT 1`,
  [userAddress, ledgerAtTime]
);

const slashedEvents = await db.query(
  `SELECT * FROM events WHERE eventName = 'bond_slashed_v2' 
   AND topics[1] = $1 AND ledger <= $2 ORDER BY ledger DESC LIMIT 1`,
  [userAddress, ledgerAtTime]
);

// Reconstruct: bonded_amount = created.amount + increased.new_total - withdrawn.remaining
//              slashed_amount = slashed.total_slashed
//              available = bonded_amount - slashed_amount
```

### Audit admin actions

```typescript
const paramUpdates = await db.query(
  `SELECT * FROM events WHERE eventName = 'param_updated' 
   AND topics[3] = $1 ORDER BY ledger ASC`,
  [adminAddress]
);

const slashes = await db.query(
  `SELECT * FROM events WHERE eventName = 'bond_slashed_v2' 
   AND topics[5] = $1 ORDER BY ledger ASC`,
  [adminAddress]
);
```

### Detect anomalies

```typescript
// Withdrawals exceeding bonded amount (sanity check)
const anomalies = await db.query(
  `SELECT identity, ledger, data->>'remaining' FROM events 
   WHERE eventName = 'bond_withdrawn_v2' 
   AND CAST(data->>'remaining' AS BIGINT) < 0`
);

// Zero or negative balance transitions (should not happen)
const negativeTransitions = await db.query(
  `SELECT identity, ledger FROM events 
   WHERE eventName IN ('bond_increased_v2', 'bond_withdrawn_v2', 'bond_slashed_v2') 
   AND data->>'new_total' < 0`
);
```

---

## See Also

- [EVENTS.md](EVENTS.md) — Canonical event catalog with complete field reference
- [known-simplifications.md](known-simplifications.md) — Known gaps in current event coverage
- [Soroban Events Documentation](https://developers.stellar.org/docs/smart-contracts/events) — Official Soroban event model
- [credence_bond_api.md](credence_bond_api.md) — Bond contract API reference
- [treasury.md](treasury.md) — Treasury contract design and operations
