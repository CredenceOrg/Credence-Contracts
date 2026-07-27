# Contracts Architecture Overview

This document is a contracts-only architecture overview for the Credence Soroban workspace. It focuses on the on-chain contracts, their primary responsibilities, the state each contract owns, the events they emit, and the integration points that matter to a backend or reputation engine.

For the broader crate dependency map, see [crates.md](crates.md). For detailed per-crate internals, see [architecture.md](architecture.md).

---

## 1. Architectural shape

The workspace is organized around a small set of deployable contracts:

- `credence_bond` is the primary source of truth for identity bonds, attestation state, slashing, tier transitions, and supply accounting.
- `credence_registry` is the discovery layer that maps identities to the bond contract instance that serves them.
- `credence_delegation` models delegated attestation or management rights for bond owners.
- `credence_treasury` records protocol fee revenue and withdrawal proposals.
- `credence_arbitration` tracks dispute lifecycle and weighted arbitrator votes.
- `admin`, `credence_multisig`, and `timelock` provide administrative and governance control surfaces.

In practice, the backend should treat the bond contract as the main stateful authority for bond and attestation data, and use the other contracts as supporting systems that expose lookup, governance, or dispute state.

---

## 2. Crate-to-responsibility map

| Crate | Primary responsibility | Main state ownership | Key events | Backend use |
|---|---|---|---|---|
| `contracts/credence_bond` | Identity bond lifecycle, attestation tracking, slashing, tiers, emergency controls | Bond records, attestation records, supply cap/supply totals, governance proposals, upgrade authorization | `bond_created_v2`, `bond_increased_v2`, `bond_withdrawn_v2`, `bond_slashed_v2`, `tier_changed`, `attestation_added`, `attestation_revoked`, `emergency_withdrawal` | Primary source for bond ledger, tier history, attestation graph, slash history, and market utilization |
| `contracts/credence_registry` | Identity-to-bond-contract discovery | Identity → bond address mapping and reverse lookup | `identity_registered`, `identity_deactivated`, `identity_reactivated` | Discovery layer for resolving which bond contract belongs to an identity |
| `contracts/credence_delegation` | Delegated attestation or management rights | Delegation records keyed by owner/delegate/type | `delegation_created`, `delegation_revoked` | Active delegation graph and delegated-permission checks |
| `contracts/credence_treasury` | Fee accounting and governance withdrawals | Proposal state, signer/threshold state, fee balances | Fee receipt and withdrawal proposal events | Revenue accounting and governance transparency |
| `contracts/credence_arbitration` | Dispute lifecycle and voting | Dispute records, vote weights, status transitions | `dispute_created`, `status_transition`, `vote_cast`, `dispute_resolved` | Reputation and moderation workflows driven by dispute outcomes |
| `contracts/admin` | Role and ownership management | Admin roster, role metadata, pending ownership transfer | `admin_added`, `admin_removed`, `admin_role_updated`, `ownership_transfer_*` | Access-control snapshot for off-chain authorization and audit |
| `contracts/credence_multisig` | Multi-signer proposals | Signer set, threshold, proposal records | Proposal creation/approval/execution events | Pending governance action visibility |
| `contracts/timelock` | Delayed execution | Queued operations with execution windows | Queue/execute/cancel events | Governance change forecasting and audit |
| `contracts/fixed_duration_bond` | Simple fixed-term bond variant | Per-owner fixed bond record and fee config | `bond_created`, `bond_withdrawn`, `bond_early_exit`, `fees_collected` | Alternative bond model and fee tracking |

---

## 3. State ownership and storage model

The contracts mostly follow a simple pattern:

- `instance()` storage holds configuration, counters, and contract-wide state.
- `persistent()` storage holds per-record state such as bond records, attestations, disputes, and votes.
- The bond contract is the biggest and most stateful contract in the workspace and holds the protocol's core business state.

### Core bond contract state

The bond contract owns the following categories of state:

- Bond state: current principal, rolling status, duration, tier, and lifecycle status.
- Attestation state: attestation records and subject-to-attestation indexes.
- Governance and slash state: slash proposals, governor votes, quorum configuration, and upgrade authorization.
- Supply state: total bonded amount and optional supply cap.
- Emergency state: emergency mode toggle and audit records.

### Registry state

The registry stores:

- Identity → bond contract address mapping.
- Bond contract → identity reverse mapping.
- A list of registered identities for discovery and enumeration.

### Delegation state

The delegation contract stores delegation records keyed by owner, delegate, and delegation type so callers can evaluate whether a delegate is currently authorized.

---

## 4. Event model and backend expectations

The contracts emit structured events so off-chain services can index state without polling every read-only entrypoint. The backend should prefer events for change tracking and use contract read methods for point-in-time state inspection.

### Event patterns to follow

- Use the `_v2` bond events whenever possible. They carry richer indexed data for efficient filtering and are the preferred integration surface.
- Treat state-changing events as the primary source of truth for incremental indexing.
- Use read-only entrypoints such as `get_identity_state()`, `get_total_supply()`, and `get_bond_contract()` for current-state lookups and backfills.

### Bond contract event highlights

The bond contract is the main event source for the backend:

- `bond_created_v2` and `bond_increased_v2` for bond lifecycle and amount changes.
- `bond_withdrawn_v2` for redemption and early-exit flows.
- `bond_slashed_v2` for slash history and risk monitoring.
- `tier_changed` for reputation or scoring updates.
- `attestation_added` and `attestation_revoked` for attestation graph updates.
- `emergency_withdrawal` for incident response and audit.

### Supporting contract event highlights

- `credence_registry`: index `identity_registered` and related lifecycle events for identity discovery.
- `credence_delegation`: index delegation create/revoke events to maintain the active delegation graph.
- `credence_arbitration`: index `status_transition` and `dispute_resolved` for dispute outcomes.
- `credence_treasury`: index fee and withdrawal proposal events for treasury accounting.

---

## 5. Runtime flow

A typical end-to-end flow looks like this:

1. An identity is registered in the registry and mapped to its bond contract.
2. The bond contract creates or updates the bond, emits the bond lifecycle event, and updates supply state.
3. The backend indexes the event stream to build the bond ledger, tier view, attestation graph, and slash history.
4. Delegation, disputes, and treasury actions layer on top of that core bond state without replacing it.

This makes the bond contract the center of gravity for protocol state, while the registry and delegation contracts provide lookup and authorization overlays.

---

## 6. Practical integration guidance

When building or extending a backend against these contracts:

- Prefer event indexing for historical data and audit trails.
- Use contract read methods for current state and reconciliation.
- Treat the bond contract as the canonical state source for balances, tiers, slashes, and attestations.
- Keep registry and delegation lookups separate from the bond ledger so contract ownership and authorization remain explicit.

For more detail on dependency structure and why the crates are split this way, see [crates.md](crates.md). For deeper runtime call paths and authorization flow, see [cross-contract-call-graph.md](cross-contract-call-graph.md).
