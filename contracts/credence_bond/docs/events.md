# Credence Bond — Event Catalog

Complete reference for all events emitted by the `credence_bond` contract.
Off-chain indexers and client applications should use this document to reconstruct state and build efficient queries.

---

## Architecture

Every event tuple is `(topics, data)`.

- **Topics** are indexed on-chain and support efficient filtering.
- **Data** carries the full payload but is not indexed.

Topic layout convention:

| Index | Type | Meaning |
|-------|------|---------|
| 0 | `Symbol` | Event name (routing key) |
| 1 | `Address` | Primary identity / actor |
| 2+ | varies | Additional indexed fields (v2 events only) |

Where both a `v1` and `v2` variant exist, **both are emitted** on every call for backward compatibility. Indexers should prefer the `v2` variant for new integrations.

---

## Bond Lifecycle

### `bond_created`

Emitted when an identity opens a new bond.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_created"` |
| topics[1] | `Address` | `identity` |
| data.0 | `i128` | `amount` — tokens bonded |
| data.1 | `u64` | `duration` — lock-up in seconds |
| data.2 | `bool` | `is_rolling` — auto-renews at expiry |

### `bond_created_v2`

Enhanced variant with amount and timestamp indexed for range queries.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_created_v2"` |
| topics[1] | `Address` | `identity` |
| topics[2] | `i128` | `amount` — indexed for amount-range queries |
| topics[3] | `u64` | `start_timestamp` — indexed for time-range queries |
| data.0 | `u64` | `duration` |
| data.1 | `bool` | `is_rolling` |
| data.2 | `u64` | `end_timestamp` = `start_timestamp + duration` |

---

### `bond_increased`

Emitted when an identity tops up an existing bond.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_increased"` |
| topics[1] | `Address` | `identity` |
| data.0 | `i128` | `added_amount` |
| data.1 | `i128` | `new_total` — bonded amount after top-up |

### `bond_increased_v2`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_increased_v2"` |
| topics[1] | `Address` | `identity` |
| topics[2] | `i128` | `added_amount` — indexed |
| topics[3] | `i128` | `new_total` — indexed |
| topics[4] | `u64` | `timestamp` — indexed |
| data.0 | `bool` | `tier_changed` — true if tier threshold crossed |
| data.1 | `BondTier` | `new_tier` — Bronze / Silver / Gold / Platinum |

---

### `bond_withdrawn`

Emitted on any successful withdrawal (normal, early, or full closure).

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_withdrawn"` |
| topics[1] | `Address` | `identity` |
| data.0 | `i128` | `amount_withdrawn` — tokens sent to wallet |
| data.1 | `i128` | `remaining` — tokens still bonded (0 on full close) |

### `bond_withdrawn_v2`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_withdrawn_v2"` |
| topics[1] | `Address` | `identity` |
| topics[2] | `i128` | `amount_withdrawn` — indexed |
| topics[3] | `i128` | `remaining` — indexed |
| topics[4] | `u64` | `timestamp` — indexed |
| data.0 | `bool` | `is_early` — true if penalty was applied |
| data.1 | `i128` | `penalty_amount` — 0 when `is_early = false` |

---

## Slashing

### `bond_slashed`

Emitted when an admin penalises a bond.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_slashed"` |
| topics[1] | `Address` | `identity` |
| data.0 | `i128` | `slash_amount` — amount slashed this call |
| data.1 | `i128` | `total_slashed` — cumulative lifetime total |

### `bond_slashed_v2`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"bond_slashed_v2"` |
| topics[1] | `Address` | `identity` |
| topics[2] | `i128` | `slash_amount` — indexed |
| topics[3] | `i128` | `total_slashed` — indexed |
| topics[4] | `u64` | `timestamp` — indexed |
| topics[5] | `Address` | `admin` — indexed for accountability |
| data.0 | `String` | `reason` |
| data.1 | `bool` | `is_full_slash` — true when bond fully liquidated |

---

## Attestation

### `attestation_added`

Emitted when an authorised attester submits a new attestation.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"attestation_added"` |
| topics[1] | `Address` | `subject` — the attested identity |
| data.0 | `u64` | `attestation_id` |
| data.1 | `Address` | `attester` |
| data.2 | `String` | `attestation_data` |

### `attestation_revoked`

Emitted when the original attester revokes an attestation.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"attestation_revoked"` |
| topics[1] | `Address` | `subject` |
| data.0 | `u64` | `attestation_id` |
| data.1 | `Address` | `attester` |

---

## Governance (Slash Proposals)

All governance events share the same data layout: `(proposal_id, actor, amount)`.

### `slash_proposed`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"slash_proposed"` |
| data.0 | `u64` | `proposal_id` |
| data.1 | `Address` | `proposer` |
| data.2 | `i128` | `amount` |

### `governance_vote`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"governance_vote"` |
| data.0 | `u64` | `proposal_id` |
| data.1 | `Address` | `voter` |
| data.2 | `i128` | `approve` — `1` = approve, `0` = reject |

### `governance_delegate`

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"governance_delegate"` |
| data.0 | `u64` | `0` (unused) |
| data.1 | `Address` | `governor` delegating |
| data.2 | `i128` | `0` (unused) |

### `slash_proposal_executed`

Emitted when quorum is met and the slash is applied.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"slash_proposal_executed"` |
| data.0 | `u64` | `proposal_id` |
| data.1 | `Address` | `proposer` |
| data.2 | `i128` | `amount` |

### `slash_proposal_rejected`

Emitted when quorum is not met or majority voted against.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"slash_proposal_rejected"` |
| data.0 | `u64` | `proposal_id` |
| data.1 | `Address` | `proposer` |
| data.2 | `i128` | `amount` |

---

## Claims (Pull-Payment)

### `claim_added`

Emitted when a reward is queued for a user (e.g. 10% slashing reward for admin).

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"claim_added"` |
| topics[1] | `Address` | `user` |
| data.0 | `ClaimType` | `VerifierReward \| SlashingReward \| PenaltyRefund \| FeeRebate \| DisputeReward` |
| data.1 | `i128` | `amount` |
| data.2 | `u64` | `source_id` — originating slash / attestation ID |

### `claims_processed`

Emitted when a user pulls their pending rewards.

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"claims_processed"` |
| topics[1] | `Address` | `user` |
| data.0 | `u32` | `processed_count` |
| data.1 | `i128` | `total_amount` |
| data.2 | `Vec<ClaimType>` | `claim_types` processed |

### `claims_expired`

Emitted when expired claims are cleaned up (30-day TTL).

| | Type | Value |
|-|------|-------|
| topics[0] | `Symbol` | `"claims_expired"` |
| topics[1] | `Address` | `user` |
| data.0 | `u32` | `expired_count` |
| data.1 | `i128` | `expired_amount` |

---

## Upgrade Authorization

| Event | topics[0] | Data |
|-------|-----------|------|
| `upgrade_auth_init` | `"upgrade_auth_init"` | `()` |
| `upgrade_auth_granted` | `"upgrade_auth_granted"` | `(address, UpgradeRole)` |
| `upgrade_auth_revoked` | `"upgrade_auth_revoked"` | `address` |
| `upgrade_proposed` | `"upgrade_proposed"` | `(proposal_id, new_implementation)` |
| `upgrade_approved` | `"upgrade_approved"` | `proposal_id` |
| `upgrade_executed` | `"upgrade_executed"` | `(new_implementation, Option<proposal_id>)` |

All upgrade events have `topics[1] = admin / proposer / approver / executor`.

---

## Indexer Query Patterns

**All events for an identity** — filter `topics[1] == identity` across all event names.

**Large bonds created** — filter `bond_created_v2` where `topics[2] >= threshold`.

**Recent activity** — filter any `*_v2` event where the timestamp topic is within range.

**Admin accountability** — filter `bond_slashed_v2` where `topics[5] == admin_address`.

**Governance audit trail** — collect `slash_proposed` → `governance_vote` → `slash_proposal_executed | slash_proposal_rejected` grouped by `data.0` (proposal_id).
