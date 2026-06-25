# Credence Contracts: Event Specification

## Overview
This document specifies all events emitted by Credence smart contracts, including their topic names, indexed parameters, and data payload schemas. It serves as the single source of truth for off-chain indexers, client applications, and integrators.

## Architecture
Every event in Soroban consists of two components:
- **Topics**: An ordered list of indexed values that support efficient filtering/querying.
- **Data**: The full payload of unindexed values.

Where both a `v1` and `v2` variant exist, **both are emitted** on every call for backwards compatibility. Indexers should prefer the `v2` variant for new integrations.

---

## Contracts
- [Credence Bond](#credence-bond)
- [Credence Delegation](#credence-delegation)

---

## Credence Bond
Identity bond contract that handles staking, slashing, attestations, and more.

### Bond Lifecycle
#### `bond_created`
Emitted when an identity opens a new bond.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_created"` |
| Topics | 1 | Address | Identity owner |
| Data | 0 | i128 | Initial bonded amount |
| Data | 1 | u64 | Lock-up duration in seconds |
| Data | 2 | bool | Auto-renewal flag |

#### `bond_created_v2`
Enhanced variant with amount and timestamp indexed for range queries.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_created_v2"` |
| Topics | 1 | Address | Identity owner |
| Topics | 2 | i128 | Initial bonded amount (indexed) |
| Topics | 3 | u64 | Bond start timestamp (indexed) |
| Data | 0 | u64 | Lock-up duration in seconds |
| Data | 1 | bool | Auto-renewal flag |
| Data | 2 | u64 | Bond end timestamp |

#### `bond_increased`
Emitted when an identity tops up an existing bond.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_increased"` |
| Topics | 1 | Address | Identity owner |
| Data | 0 | i128 | Additional amount deposited |
| Data | 1 | i128 | New total bonded amount |

#### `bond_increased_v2`
Enhanced variant with amount and timestamp indexed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_increased_v2"` |
| Topics | 1 | Address | Identity owner |
| Topics | 2 | i128 | Additional amount deposited (indexed) |
| Topics | 3 | i128 | New total bonded amount (indexed) |
| Topics | 4 | u64 | Increase timestamp (indexed) |
| Data | 0 | bool | Whether tier changed |
| Data | 1 | BondTier | New bond tier |

#### `bond_withdrawn`
Emitted on any successful withdrawal (normal, early, or full closure).

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_withdrawn"` |
| Topics | 1 | Address | Identity owner |
| Data | 0 | i128 | Amount withdrawn |
| Data | 1 | i128 | Remaining bonded amount |

#### `bond_withdrawn_v2`
Enhanced variant with amount and timestamp indexed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_withdrawn_v2"` |
| Topics | 1 | Address | Identity owner |
| Topics | 2 | i128 | Amount withdrawn (indexed) |
| Topics | 3 | i128 | Remaining bonded amount (indexed) |
| Topics | 4 | u64 | Withdrawal timestamp (indexed) |
| Data | 0 | bool | Early withdrawal flag |
| Data | 1 | i128 | Penalty amount |

#### `bond_liquidated`
Emitted when a bond is finalized through `liquidate()`.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_liquidated"` |
| Topics | 1 | Address | Identity owner |
| Data | 0 | i128 | Residual amount swept to treasury |
| Data | 1 | Symbol | Reason (`"fully_slashed"` or `"expired_unrenewed"`) |
| Data | 2 | u64 | Liquidation timestamp |
| Data | 3 | Address | Admin/keeper that performed liquidation |

### Slashing
#### `bond_slashed`
Emitted when an admin penalizes a bond.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_slashed"` |
| Topics | 1 | Address | Identity owner |
| Data | 0 | i128 | Amount slashed this call |
| Data | 1 | i128 | Total lifetime slashed amount |

#### `bond_slashed_v2`
Enhanced variant with admin address and reason included.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_slashed_v2"` |
| Topics | 1 | Address | Identity owner |
| Topics | 2 | i128 | Amount slashed this call (indexed) |
| Topics | 3 | i128 | Total lifetime slashed amount (indexed) |
| Topics | 4 | u64 | Slash timestamp (indexed) |
| Topics | 5 | Address | Admin that performed slash (indexed) |
| Data | 0 | String | Reason for slash |
| Data | 1 | bool | Full slash flag |

### Attestations
#### `attestation_added`
Emitted when an authorized attester submits a new attestation.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"attestation_added"` |
| Topics | 1 | Address | Attestation subject |
| Data | 0 | u64 | Attestation ID |
| Data | 1 | Address | Attester address |
| Data | 2 | String | Attestation data |

#### `attestation_revoked`
Emitted when the original attester revokes an attestation.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"attestation_revoked"` |
| Topics | 1 | Address | Attestation subject |
| Data | 0 | u64 | Attestation ID |
| Data | 1 | Address | Attester address |

### Tier System
#### `tier_changed`
Emitted when a bond crosses a tier threshold.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"tier_changed"` |
| Data | 0 | Address | Identity owner |
| Data | 1 | BondTier | New tier |

### Governance (Slash Proposals)
All governance events share similar data layout.

#### `slash_proposed`
Emitted when a new slash proposal is created.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"slash_proposed"` |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | Proposer |
| Data | 2 | i128 | Proposed slash amount |

#### `governance_vote`
Emitted when a governor casts a vote.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"governance_vote"` |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | Voter |
| Data | 2 | i128 | Vote (1 = approve, 0 = reject) |

#### `governance_delegate`
Emitted when a governor delegates voting power.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"governance_delegate"` |
| Data | 0 | u64 | (unused) |
| Data | 1 | Address | Governor delegating |
| Data | 2 | i128 | (unused) |

#### `slash_proposal_executed`
Emitted when a slash proposal passes and is executed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"slash_proposal_executed"` |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | Proposer |
| Data | 2 | i128 | Slash amount |

#### `slash_proposal_rejected`
Emitted when a slash proposal fails to pass.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"slash_proposal_rejected"` |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | Proposer |
| Data | 2 | i128 | Slash amount |

### Evidence
#### `evidence_submitted`
Emitted when evidence is submitted for a slash proposal.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"evidence_submitted"` |
| Topics | 1 | u64 | Evidence ID (indexed) |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | Submitter |
| Data | 2 | String | Evidence hash |

### Claims (Pull-Payment)
#### `claim_added`
Emitted when a reward is queued for a user.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"claim_added"` |
| Topics | 1 | Address | User |
| Data | 0 | ClaimType | Claim type |
| Data | 1 | i128 | Claim amount |
| Data | 2 | u64 | Source ID |

#### `claims_processed`
Emitted when a user pulls their pending rewards.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"claims_processed"` |
| Topics | 1 | Address | User |
| Data | 0 | u32 | Number of claims processed |
| Data | 1 | i128 | Total amount claimed |
| Data | 2 | Vec<ClaimType> | Types of claims processed |

#### `claims_expired`
Emitted when expired claims are cleaned up.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"claims_expired"` |
| Topics | 1 | Address | User |
| Data | 0 | u32 | Number of expired claims |
| Data | 1 | i128 | Total expired amount |

### Fees
#### `bond_creation_fee`
Emitted when a bond creation fee is collected.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_creation_fee"` |
| Data | 0 | Address | Identity owner |
| Data | 1 | i128 | Bond amount |
| Data | 2 | i128 | Fee amount |
| Data | 3 | Address | Treasury |

### Early Exit Penalty
#### `early_exit_config_set`
Emitted when early exit configuration is set.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"early_exit_config_set"` |
| Data | 0 | Address | Treasury |
| Data | 1 | u32 | Penalty basis points |

#### `early_exit_penalty`
Emitted when an early exit penalty is applied.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"early_exit_penalty"` |
| Data | 0 | Address | Identity owner |
| Data | 1 | i128 | Withdrawal amount |
| Data | 2 | i128 | Penalty amount |
| Data | 3 | Address | Treasury |

### Cooldown
#### `cooldown_requested`
Emitted when a cooldown withdrawal is requested.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"cooldown_requested"` |
| Data | 0 | Address | Requester |
| Data | 1 | i128 | Requested amount |

#### `cooldown_executed`
Emitted when a cooldown withdrawal is executed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"cooldown_executed"` |
| Data | 0 | Address | Requester |
| Data | 1 | i128 | Executed amount |

#### `cooldown_cancelled`
Emitted when a cooldown withdrawal is cancelled.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"cooldown_cancelled"` |
| Data | 0 | Address | Requester |

#### `cooldown_period_updated`
Emitted when the cooldown period is updated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"cooldown_period_updated"` |
| Data | 0 | u64 | Old period |
| Data | 1 | u64 | New period |

### Emergency
#### `emergency_mode_changed`
Emitted when emergency mode is toggled.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"emergency_mode_changed"` |
| Data | 0 | bool | Enabled flag |
| Data | 1 | Address | Admin |
| Data | 2 | Address | Governance approver |
| Data | 3 | Symbol | Reason |

#### `emergency_withdrawal`
Emitted when an emergency withdrawal is executed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"emergency_withdrawal"` |
| Topics | 1 | u64 | Record ID (indexed) |
| Topics | 2 | Address | Identity owner (indexed) |
| Data | 0 | i128 | Gross amount |
| Data | 1 | i128 | Fee amount |
| Data | 2 | i128 | Net amount |
| Data | 3 | Symbol | Reason |

### Verifiers
#### `verifier_config_updated`
Emitted when verifier configuration is updated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_config_updated"` |
| Data | 0 | i128 | New minimum stake |

#### `verifier_registered`
Emitted when a new verifier is registered.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_registered"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | Symbol | Kind (`"new"` or `"legacy"`) |
| Data | 1 | i128 | Stake deposited |
| Data | 2 | i128 | Total stake |
| Data | 3 | i128 | Minimum stake |

#### `verifier_reactivated`
Emitted when an inactive verifier is reactivated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_reactivated"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | Symbol | Kind (`"reactivated"`) |
| Data | 1 | i128 | Stake deposited |
| Data | 2 | i128 | Total stake |
| Data | 3 | i128 | Minimum stake |

#### `verifier_stake_deposited`
Emitted when an active verifier tops up their stake.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_stake_deposited"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | Symbol | Kind (`"top_up"`) |
| Data | 1 | i128 | Stake deposited |
| Data | 2 | i128 | Total stake |
| Data | 3 | i128 | Minimum stake |

#### `verifier_deactivated`
Emitted when a verifier is deactivated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_deactivated"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | Symbol | Reason |
| Data | 1 | u64 | Timestamp |
| Data | 2 | i128 | Stake |

#### `verifier_stake_withdrawn`
Emitted when a deactivated verifier withdraws stake.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_stake_withdrawn"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | i128 | Withdrawn amount |
| Data | 1 | i128 | Remaining stake |

#### `verifier_reputation_updated`
Emitted when a verifier's reputation changes.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier_reputation_updated"` |
| Topics | 1 | Address | Verifier address (indexed) |
| Data | 0 | i128 | Reputation delta |
| Data | 1 | i128 | New reputation |
| Data | 2 | u32 | Attestations issued |
| Data | 3 | u32 | Attestations revoked |
| Data | 4 | Symbol | Reason |

### Parameters
#### `param_updated`
Emitted when a protocol parameter is updated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"param_updated"` |
| Topics | 1 | Symbol | Parameter key |
| Topics | 2 | Symbol | Category |
| Topics | 3 | Address | Admin |
| Data | 0 | i128 | Old value |
| Data | 1 | i128 | New value |

### Upgrade Authorization
#### `upgrade_auth_init`
Emitted when upgrade auth is initialized.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_auth_init"` |
| Topics | 1 | Address | Admin |
| Data | - | - | Empty |

#### `upgrade_auth_granted`
Emitted when upgrade auth is granted.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_auth_granted"` |
| Topics | 1 | Address | Admin |
| Data | 0 | Address | Grantee |
| Data | 1 | UpgradeRole | Role |

#### `upgrade_auth_revoked`
Emitted when upgrade auth is revoked.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_auth_revoked"` |
| Topics | 1 | Address | Admin |
| Data | 0 | Address | Revokee |

#### `upgrade_proposed`
Emitted when an upgrade is proposed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_proposed"` |
| Topics | 1 | Address | Proposer |
| Data | 0 | u64 | Proposal ID |
| Data | 1 | Address | New implementation |

#### `upgrade_approved`
Emitted when an upgrade proposal is approved.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_approved"` |
| Topics | 1 | Address | Approver |
| Data | 0 | u64 | Proposal ID |

#### `upgrade_executed`
Emitted when an upgrade is executed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_executed"` |
| Topics | 1 | Address | Executor |
| Data | 0 | Address | New implementation |
| Data | 1 | Option<u64> | Proposal ID |

### Admin Transfers
#### `admin_transfer_started`
Emitted when an admin transfer is initiated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"admin_transfer_started"` |
| Topics | 1 | Address | Current admin |
| Data | 0 | Address | Pending admin |

#### `admin_transfer_completed`
Emitted when an admin transfer is completed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"admin_transfer_completed"` |
| Topics | 1 | Address | Old admin |
| Data | 0 | Address | New admin |

#### `upgrade_admin_transfer_started`
Emitted when an upgrade admin transfer is initiated.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_admin_transfer_started"` |
| Topics | 1 | Address | Current admin |
| Data | 0 | Address | Pending admin |

#### `upgrade_admin_transfer_completed`
Emitted when an upgrade admin transfer is completed.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"upgrade_admin_transfer_completed"` |
| Topics | 1 | Address | Old admin |
| Data | 0 | Address | New admin |

### Bond Drift
#### `bond_drift_detected`
Emitted when inconsistent bond or attestation state is detected.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"bond_drift_detected"` |
| Topics | 1 | Address | Subject identity |
| Data | 0 | BondDriftKind | Kind of drift |
| Data | 1 | i128 | Bonded amount |
| Data | 2 | i128 | Slashed amount |
| Data | 3 | u32 | Subject attestation count |
| Data | 4 | u32 | Subject attestation list length |

---

## Credence Delegation
Delegation contract that handles delegated actions and signature verification.

### Verifier Registry
#### `("verifier", "registered")`
Emitted when a signature verifier is registered.

| Component | Position | Type | Description |
|-----------|----------|------|-------------|
| Topics | 0 | Symbol | `"verifier"` |
| Topics | 1 | Symbol | `"registered"` |
| Data | 0 | VerifierRegisteredEvent | Event payload |

---

## Indexer Query Patterns
- **All events for an identity**: Filter `topics[1] == identity` across all relevant event names.
- **Large bonds created**: Filter `bond_created_v2` where `topics[2] >= threshold`.
- **Recent activity**: Filter any `*_v2` event where the timestamp topic is within range.
- **Admin accountability**: Filter `bond_slashed_v2` where `topics[5] == admin_address`.
- **Governance audit trail**: Collect `slash_proposed` → `governance_vote` → `slash_proposal_executed` / `slash_proposal_rejected` grouped by `data[0]` (proposal ID).

---

## Additional Resources
- [Credence Bond Docs](./credence-bond.md)
- [Credence Delegation Docs](./credence-delegation.md)
- [README](../README.md)
