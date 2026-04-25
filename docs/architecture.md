# Contracts Architecture Overview

This document maps every crate in the workspace to its responsibility, state layout, events, and backend consumption points. It is the canonical reference for anyone integrating with or extending the Credence contracts.

---

## Workspace Crates

| Crate | Package name | Purpose |
|---|---|---|
| `contracts/credence_bond` | `credence_bond` | Core identity bond, attestations, slashing, governance |
| `contracts/credence_registry` | `credence_registry` | Identity ↔ bond-contract address mapping |
| `contracts/credence_treasury` | `credence_treasury` | Fee accounting and multi-sig withdrawal |
| `contracts/credence_delegation` | `credence_delegation` | Delegated attestation and management rights |
| `contracts/credence_arbitration` | `credence_arbitration` | Weighted-vote dispute resolution |
| `contracts/dispute_resolution` | `dispute_resolution` | Stake-backed slash dispute with arbitrator voting |
| `contracts/admin` | `admin` | Hierarchical role management (SuperAdmin/Admin/Operator) |
| `contracts/credence_multisig` | `credence_multisig` | Generic M-of-N multi-signature proposals |
| `contracts/timelock` | `timelock` | Time-delayed operation execution |
| `contracts/fixed_duration_bond` | `fixed_duration_bond` | Simple fixed-term bond with optional early-exit penalty |
| `contracts/credence_errors` | `credence_errors` | Shared `ContractError` enum used across crates |
| `contracts/credence_math` | `credence_math` | Overflow-safe arithmetic helpers (`add_i128`, `split_bps`, …) |

---

## Crate Details

### `credence_bond`

**Responsibility:** The protocol's primary contract. Manages the full lifecycle of an identity bond, the attestation system, slashing with governance approval, tiered bond levels, rolling bonds, early-exit penalties, fee collection, batch operations, and upgrade authorization.

**Internal modules:**

| Module | Role |
|---|---|
| `access_control` | Verifier role add/remove |
| `batch` | Atomic multi-bond creation |
| `claims` | Pending verifier reward claims |
| `cooldown` | Configurable withdrawal cooldown period |
| `early_exit_penalty` | Penalty calculation and treasury transfer |
| `emergency` | Dual-auth emergency withdrawal with audit records |
| `events` | All event emission helpers |
| `evidence` | IPFS/hash evidence storage for slash proposals |
| `fees` | Bond-creation fee calculation and accumulation |
| `governance_approval` | Governor-based slash proposal voting |
| `leverage` | Max-leverage validation |
| `liquidation_scanner` | Same-ledger collateral-increase guard |
| `math` | Internal arithmetic (wraps `credence_math`) |
| `nonce` | Permit-style replay prevention |
| `normalization` | Amount normalization utilities |
| `parameters` | Configurable tier thresholds and max leverage |
| `pausable` | Multi-sig pause mechanism |
| `rolling_bond` | Notice-period withdrawal for rolling bonds |
| `safe_token` | Token transfer helpers |
| `same_ledger_liquidation_guard` | Prevents same-ledger collateral manipulation |
| `slash_history` | Immutable slash history log |
| `slashing` | Core slash logic |
| `tiered_bond` | Bronze/Silver/Gold/Platinum tier derivation |
| `token_integration` | USDC transfer with balance-delta verification |
| `types` | Shared `Attestation` type |
| `upgrade_auth` | Proposer/approver upgrade authorization |
| `validation` | Amount and duration validation |
| `verifier` | Verifier stake, reputation, and attestation tracking |
| `weighted_attestation` | Weight computation from verifier stake |

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `Admin` | `Address` | Contract administrator |
| `Bond` | `IdentityBond` | Single bond per contract instance |
| `Token` / `BondToken` | `Address` | Configured USDC token |
| `Attester(addr)` | `bool` | Registered attester flag |
| `Attestation(u64)` | `Attestation` | Attestation record by ID |
| `AttestationCounter` | `u64` | Auto-incrementing attestation ID |
| `SubjectAttestations(addr)` | `Vec<u64>` | Attestation IDs for a subject |
| `Nonce(addr)` | `u64` | Per-identity replay-prevention nonce |
| `GovernanceProposal(u64)` | `SlashProposal` | Slash proposal record |
| `GovernanceVote(u64, addr)` | `bool` | Vote record per (proposal, governor) |
| `GovernanceGovernors` | `Vec<Address>` | Active governor list |
| `GovernanceQuorumBps` | `u32` | Quorum threshold in basis points |
| `FeeTreasury` | `Address` | Treasury address for fees |
| `FeeBps` | `u32` | Creation fee in basis points |
| `Evidence(u64)` | `Evidence` | Evidence record by ID |
| `SupplyCap` | `i128` | Maximum total bonded amount (0 = uncapped) |
| `TotalSupply` | `i128` | Current total bonded amount |
| `UpgradeAuth(addr)` | `UpgradeRole` | Upgrade role per address |
| `Paused` | `bool` | Pause flag |

**Events emitted:**

| Symbol | Topics | Data | Trigger |
|---|---|---|---|
| `bond_created` | `(symbol, identity)` | `(amount, duration, is_rolling)` | Bond created (legacy) |
| `bond_created_v2` | `(symbol, identity, amount, start_ts)` | `(duration, is_rolling, end_ts)` | Bond created |
| `bond_increased_v2` | `(symbol, identity, added, new_total, ts)` | `(tier_changed, new_tier)` | Top-up |
| `bond_withdrawn_v2` | `(symbol, identity, withdrawn, remaining, ts)` | `(is_early, penalty)` | Withdrawal |
| `bond_slashed_v2` | `(symbol, identity, slash_amt, total_slashed, ts, admin)` | `(reason, is_full_slash)` | Slash executed |
| `tier_changed` | `(symbol, identity)` | `new_tier` | Tier upgrade/downgrade |
| `attestation_added` | `(symbol, subject)` | `(id, attester, data)` | Attestation created |
| `attestation_revoked` | `(symbol, subject)` | `(id, attester)` | Attestation revoked |
| `claim_added` | `(symbol, user)` | `(claim_type, amount, source_id)` | Verifier reward queued |
| `claims_processed` | `(symbol, user)` | `(count, total, types)` | Claims paid out |
| `supply_cap_updated` | `(symbol,)` | `(admin, cap)` | Supply cap changed |
| `emergency_mode` | — | `(enabled, admin, governance, ts)` | Emergency mode toggled |
| `emergency_withdrawal` | — | `(record_id, identity, gross, fee, net, reason, ts)` | Emergency withdrawal |
| `upgrade_proposed` | `(symbol, proposer)` | `(proposal_id, new_impl)` | Upgrade proposed |
| `upgrade_approved` | `(symbol, approver)` | `proposal_id` | Upgrade approved |
| `upgrade_executed` | `(symbol, executor)` | `(new_impl, proposal_id)` | Upgrade executed |
| `attester_registered` | `(symbol,)` | `attester` | Attester added |
| `attester_unregistered` | `(symbol,)` | `attester` | Attester removed |

**Backend consumption points:**

- Index `bond_created_v2` to build the bond ledger (identity → bonded amount, start, end).
- Index `bond_slashed_v2` to track slash history per identity.
- Index `tier_changed` to maintain current tier per identity for reputation scoring.
- Index `attestation_added` / `attestation_revoked` to build the attestation graph.
- Poll `get_identity_state()` to read current bond state for a given contract instance.
- Poll `get_total_supply()` / `get_supply_cap()` for market utilization metrics.
- Index `emergency_withdrawal` for incident response and audit.

---

### `credence_registry`

**Responsibility:** Bidirectional mapping between identity addresses and their deployed bond contract addresses. The backend uses this as the discovery layer to find which bond contract belongs to which identity.

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `Admin` | `Address` | Registry administrator |
| `IdentityToBond(addr)` | `RegistryEntry` | Forward mapping: identity → entry |
| `BondToIdentity(addr)` | `Address` | Reverse mapping: bond contract → identity |
| `RegisteredIdentities` | `Vec<Address>` | Ordered list of all registered identities |

**Events emitted:**

| Symbol | Data | Trigger |
|---|---|---|
| `registry_initialized` | `admin` | Contract initialized |
| `identity_registered` | `(entry, allow_non_interface)` | New identity registered |
| `identity_deactivated` | `entry` | Registration deactivated |
| `identity_reactivated` | `entry` | Registration reactivated |
| `admin_transferred` | `new_admin` | Admin changed |

**Backend consumption points:**

- Index `identity_registered` to maintain the identity→bond-contract lookup table.
- Use `get_bond_contract(identity)` for on-demand lookups.
- Use `get_all_identities()` for full enumeration (note: unbounded — prefer event-based indexing at scale; see [known-simplifications.md](known-simplifications.md#7-get_all_identities-has-no-pagination)).

---

### `credence_treasury`

**Responsibility:** Pure accounting system for protocol fee revenue. Tracks fee balances and multi-sig withdrawal proposals. Does not hold tokens directly in the current implementation (see [known-simplifications.md](known-simplifications.md#3-treasury-is-a-pure-accounting-system-no-token-custody)).

**State:** Defined in `contracts/credence_treasury/src/treasury.rs`. Includes signer list, threshold, proposal records, and approval tracking.

**Events emitted:** Fee receipt, withdrawal proposal creation, approval, and execution events (emitted from `treasury.rs`).

**Backend consumption points:**

- Index fee-receipt events to track protocol revenue by source.
- Monitor withdrawal proposals for governance transparency.

---

### `credence_delegation`

**Responsibility:** Allows bond owners to delegate attestation or management rights to another address for a bounded time period.

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `Admin` | `Address` | Contract administrator |
| `Delegation(owner, delegate, type)` | `Delegation` | Delegation record |

**Events emitted:**

| Symbol | Data | Trigger |
|---|---|---|
| `delegation_created` | `delegation` | New delegation stored |
| `delegation_revoked` | `delegation` | Delegation revoked |

**Backend consumption points:**

- Index `delegation_created` / `delegation_revoked` to maintain the active delegation graph.
- Query `is_valid_delegate(owner, delegate, type)` before allowing delegated actions.

---

### `credence_arbitration`

**Responsibility:** Canonical dispute status machine with weighted arbitrator voting. Enforces `Open → Voting → Resolving → Resolved / Cancelled` transitions.

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `Admin` | `Address` | Contract administrator |
| `Arbitrator(addr)` | `i128` | Arbitrator voting weight |
| `Dispute(u64)` | `Dispute` | Dispute record by ID |
| `DisputeCounter` | `u64` | Auto-incrementing dispute ID |
| `DisputeVotes(u64)` | `Map<u32, i128>` | Outcome → total weight |
| `VoterCasted(u64, addr)` | `bool` | Double-vote prevention |

**Events emitted:**

| Symbol | Data | Trigger |
|---|---|---|
| `dispute_created` | `(id, creator)` | Dispute opened |
| `status_transition` | `(from, to)` | Any status change |
| `vote_cast` | `(dispute_id, voter, outcome, weight)` | Vote recorded |
| `dispute_cancelled` | `(id, caller)` | Dispute cancelled |
| `dispute_resolved` | `(id, winning_outcome)` | Dispute resolved |
| `arbitrator_registered` | `(arbitrator, weight)` | Arbitrator added |
| `arbitrator_unregistered` | `arbitrator` | Arbitrator removed |

**Backend consumption points:**

- Index `status_transition` to track dispute lifecycle.
- Index `dispute_resolved` to feed outcomes back to the reputation engine.
- Note: arbitrator weights are admin-assigned integers, not stake-backed (see [known-simplifications.md](known-simplifications.md#9-arbitration-voting-weights-are-not-stake-backed)).

---

### `dispute_resolution`

**Responsibility:** Stake-backed slash dispute system. A disputer locks tokens as stake, arbitrators vote, and the outcome determines whether the stake is returned or forfeited.

**State:**

| Key | Storage tier | Description |
|---|---|---|
| `Admin` | `instance()` | Contract administrator |
| `DisputeCounter` | `instance()` | Global dispute ID counter |
| `Dispute(u64)` | `persistent()` | Full dispute record |
| `Vote(u64, addr)` | `persistent()` | Per-(dispute, arbitrator) vote |

**Events emitted (typed `#[contractevent]`):**

| Type | Fields | Trigger |
|---|---|---|
| `DisputeCreated` | `dispute_id, disputer, slash_request_id, stake, deadline` | Dispute opened |
| `VoteCast` | `dispute_id, arbitrator, favor_disputer` | Vote recorded |
| `DisputeResolved` | `dispute_id, outcome, votes_for_disputer, votes_for_slasher` | Resolved |
| `DisputeExpired` | `dispute_id, expired_at` | Deadline passed without resolution |

**Backend consumption points:**

- Index `DisputeCreated` to track open disputes against slash requests.
- Index `DisputeResolved` to update slash status in the reputation engine.
- Index `DisputeExpired` to clean up stale dispute records.

---

### `admin`

**Responsibility:** Hierarchical role management. Defines `SuperAdmin > Admin > Operator` roles, enforces minimum admin count, and supports two-step ownership transfer.

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `AdminList` | `Vec<Address>` | All admin addresses |
| `AdminInfo(addr)` | `AdminInfo` | Role, assignment metadata, active flag |
| `RoleAdmins(role)` | `Vec<Address>` | Addresses per role |
| `Owner` | `Address` | Current contract owner |
| `PendingOwner` | `Address` | Pending owner for two-step transfer |

**Events emitted:**

| Symbol | Data | Trigger |
|---|---|---|
| `admin_initialized` | `super_admin` | Contract initialized |
| `admin_added` | `admin_info` | Admin added |
| `admin_removed` | `admin_info` | Admin removed |
| `admin_role_updated` | `(addr, old_role, new_role)` | Role changed |
| `admin_deactivated` | `admin_info` | Admin deactivated |
| `admin_reactivated` | `admin_info` | Admin reactivated |
| `ownership_transfer_initiated` | `(current_owner, pending_owner)` | Transfer proposed |
| `ownership_transfer_accepted` | `(previous_owner, new_owner)` | Transfer completed |

**Backend consumption points:**

- Index role events to maintain an up-to-date access control snapshot for off-chain authorization checks.

---

### `credence_multisig`

**Responsibility:** Generic M-of-N multi-signature proposal system. Any operation can be proposed, approved by signers, and executed once the threshold is met.

**State:** Defined in `contracts/credence_multisig/src/multisig.rs`. Includes signer list, threshold, and proposal records.

**Events emitted:** Proposal creation, approval, and execution events.

**Backend consumption points:**

- Index proposal events to surface pending governance actions in dashboards.
- Note: proposals have no expiry in the current implementation (see [known-simplifications.md](known-simplifications.md#11-multisig-proposals-have-no-expiry)).

---

### `timelock`

**Responsibility:** Queues operations with a mandatory delay before execution. Prevents immediate execution of sensitive admin actions.

**State:** Defined in `contracts/timelock/src/timelock.rs`. Includes queued operations and their earliest execution timestamps.

**Events emitted:** Operation queued, executed, and cancelled events.

**Backend consumption points:**

- Index queued operations to surface upcoming protocol changes with their execution windows.
- Note: executed operations are not permanently marked to prevent replay (see [known-simplifications.md](known-simplifications.md#10-timelock-has-no-execution-guard-against-replays)).

---

### `fixed_duration_bond`

**Responsibility:** Simplified fixed-term bond for any address. One active bond per owner. Supports optional creation fee and early-exit penalty. Rejects fee-on-transfer tokens via balance-delta verification.

**State (persistent storage per owner):**

| Key | Type | Description |
|---|---|---|
| `Bond(addr)` | `FixedBond` | Bond record per owner |

**State (instance storage):**

| Key | Type | Description |
|---|---|---|
| `Admin` | `Address` | Contract administrator |
| `Token` | `Address` | Configured token |
| `FeeConfig` | `FeeConfig` | Treasury address and fee bps |
| `PenaltyBps` | `u32` | Default early-exit penalty |
| `AccruedFees` | `i128` | Accumulated uncollected fees |

**Events emitted:**

| Symbol | Data | Trigger |
|---|---|---|
| `bond_created` | `(net_amount, expiry)` | Bond created |
| `bond_withdrawn` | `amount` | Matured withdrawal |
| `bond_early_exit` | `(net_amount, penalty)` | Early withdrawal |
| `fees_collected` | `(admin, recipient, amount)` | Fees collected |
| `fee_config_updated` | `(old_treasury, old_bps, new_treasury, new_bps)` | Fee config changed |

**Backend consumption points:**

- Index `bond_created` / `bond_withdrawn` / `bond_early_exit` to track fixed-term bond positions.
- Index `fees_collected` for revenue accounting.

---

### `credence_errors`

**Responsibility:** Shared `ContractError` enum. Imported by `credence_registry`, `dispute_resolution`, and other crates to emit consistent typed errors via `panic_with_error!`.

**No state. No events.**

---

### `credence_math`

**Responsibility:** Overflow-safe arithmetic primitives used across contracts. Provides `add_i128`, `mul_i128`, `split_bps` (basis-point split), and related helpers.

**No state. No events.**

---

## Cross-Crate Relationships

```
credence_bond
  ├── uses credence_math        (arithmetic)
  ├── uses credence_errors      (error types, indirectly via panic)
  └── logically paired with credence_registry (manual admin step)

credence_registry
  └── uses credence_errors      (ContractError variants)

dispute_resolution
  └── uses credence_errors      (ContractError variants)

fixed_duration_bond
  └── uses credence_math        (split_bps, add_i128)

All contracts
  └── share pausable module pattern (copy per crate, not a shared lib)
```

---

## Shared Patterns

### Pause mechanism

Every contract implements the same multi-sig pause pattern via a local `pausable` module. All state-changing functions call `pausable::require_not_paused(&e)` at entry. Read-only functions remain accessible when paused.

Pause can be immediate (threshold = 0, admin-only) or multi-sig (threshold > 0, requires proposal + approvals + execution).

### Storage tiers

| Tier | Used for |
|---|---|
| `instance()` | Admin config, counters, current bond state — small, always loaded with the contract |
| `persistent()` | Per-record data (attestations, disputes, votes) — independently rentable, unbounded sets |
| `temporary()` | Not currently used in production paths |

### Token safety

All token transfers use balance-delta verification to reject fee-on-transfer tokens:

```rust
let before = token.balance(&contract);
token.transfer_from(&contract, &owner, &contract, &amount);
let after = token.balance(&contract);
assert!(after - before == amount, "unsupported token");
```

This pattern is implemented in `credence_bond/src/token_integration.rs`, `dispute_resolution/src/lib.rs`, and `fixed_duration_bond/src/lib.rs`.

### Event versioning

`credence_bond` emits both `bond_created` (legacy) and `bond_created_v2` (indexed) for backward compatibility during migration. The `_v2` variants include additional indexed topics for efficient off-chain filtering. New integrations should consume `_v2` events.

---

## Backend Integration Summary

| What the backend needs | Where to get it |
|---|---|
| Identity → bond contract address | `credence_registry`: index `identity_registered`, query `get_bond_contract` |
| Current bond state (amount, tier, active) | `credence_bond`: poll `get_identity_state()` on the bond contract |
| Bond creation / withdrawal history | `credence_bond`: index `bond_created_v2`, `bond_withdrawn_v2` |
| Slash history | `credence_bond`: index `bond_slashed_v2` |
| Current tier per identity | `credence_bond`: index `tier_changed` |
| Attestation graph | `credence_bond`: index `attestation_added`, `attestation_revoked` |
| Active delegations | `credence_delegation`: index `delegation_created`, `delegation_revoked` |
| Dispute outcomes | `credence_arbitration`: index `dispute_resolved`; `dispute_resolution`: index `DisputeResolved` |
| Protocol fee revenue | `credence_treasury`: index fee-receipt events; `fixed_duration_bond`: index `fees_collected` |
| Admin / role changes | `admin`: index `admin_added`, `admin_removed`, `admin_role_updated` |
| Pending governance actions | `credence_multisig`: index proposal events; `timelock`: index queued operations |
| Supply utilization | `credence_bond`: poll `get_total_supply()`, `get_supply_cap()` |

For known limitations affecting backend integration (unbounded registry pagination, treasury token custody, etc.) see [known-simplifications.md](known-simplifications.md).
