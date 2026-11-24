# Bond Upgrade Authorization Checklist

Pre-upgrade authorization and safety checklist for the `credence_bond` contract. All authorization checks, governance requirements, and proxy compatibility safeguards must pass before executing a contract upgrade.

## 1. Upgrader Identity & Role Verification

- [ ] **Caller Authentication**: Caller must sign the transaction (`executor.require_auth()`).
- [ ] **Active Upgrader Role**: Caller must have an active authorization entry in storage with `UpgradeRole::Upgrader`.
- [ ] **Expiration Check**: `auth.expires_at == 0` or current ledger timestamp `timestamp <= auth.expires_at`.
- [ ] **Role Level Check**: Role value is 2 (`Upgrader`). Users with `UpgradeRole::Proposer` (role 1) cannot call `execute_upgrade`.

## 2. Upgrade Admin Two-Step Handoff Verification (if rotating admin)

- [ ] **Admin Authentication**: Only the current `UpgradeAdmin` stored under `DataKey::Upgrade(UpgradeKey::Admin)` can propose transfer (`transfer_upgrade_admin`).
- [ ] **Non-Self Assignment**: Proposed `new_admin` is distinct from the current admin.
- [ ] **Zero-Address Guard**: Proposed `new_admin` is not a zero-address (`AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`).
- [ ] **Timelock Delay**: 24 hours (86,400 seconds) have elapsed since proposal (`now >= proposed_at + 86_400`).
- [ ] **Proposal Expiry**: Handoff executed within 7 days (604,800 seconds) of proposal (`now <= proposed_at + 604_800`).
- [ ] **Pending Acceptor Authentication**: `accept_upgrade_admin` signed by `pending.new_admin`.

## 3. Governance Proposal & Multi-Sig Approvals (if governance-gated)

- [ ] **Proposal Creation**: Proposed by authorized `Proposer` or `Upgrader` via `propose_upgrade`.
- [ ] **Pending Status**: Proposal is in `UpgradeStatus::Pending` state prior to execution.
- [ ] **Approval Threshold**: Proposal has accumulated required number of unique upgrader approvals (`approvals.len() >= required_approvals`).
- [ ] **Implementation Matching**: `new_implementation` matches proposal `new_implementation` exactly.
- [ ] **Execution State Transition**: Proposal marked as `UpgradeStatus::Executed` post-execution to prevent double execution.

## 4. Timelock Guard Verification (for production upgrades)

- [ ] **Timelock Queueing**: Operation queued via timelock with deterministic `op_hash` derived from implementation WASM hash.
- [ ] **Minimum Delay Window**: Timelock delay of 24 hours (86,400 s) satisfied (`now >= eta`).
- [ ] **Grace Period Expiry**: Execution attempted before grace period expires (`now <= eta + 86_400`).
- [ ] **Timelock Execution**: Timelock `execute_operation` succeeded and marked `op_hash` as executed.

## 5. WASM & Implementation Integrity Safeguards

- [ ] **Distinct Implementation**: `new_implementation` is distinct from `current_implementation` ("same implementation" guard).
- [ ] **Replay Guard Rejection**: Implementation hash has not been executed previously (`DataKey::ExecutedOp(op_hash)` is `false`).
- [ ] **WASM Size Budget**: Compiled binary passes `scripts/check_wasm_size.sh` budget verification.
- [ ] **Reproducible Build Check**: WASM hash verified against reproducible build pipeline output (see [wasm-reproducibility.md](wasm-reproducibility.md)).

## 6. Storage Migration & Data Integrity Safeguards

- [ ] **Lazy Migration Compatibility**: Schema migration logic registered in `src/migration.rs` if introducing new storage fields.
- [ ] **Key Namespace Preservation**: Instance and persistent storage key schema compatibility verified against [STORAGE_KEYS.md](STORAGE_KEYS.md).
- [ ] **State Preservation**: Critical invariant variables (total supply, balance trackers, admin keys) preserved post-upgrade.

## 7. Automated Test Coverage & Verification Matrix

Run full contract test suite before approving or executing upgrades:

```sh
cargo test -p credence_bond
```

Key test coverage for upgrade authorization checks:

| Test | Location | Validates |
|------|----------|-----------|
| `test_upgrade_authorization_initialization` | `src/test_upgrade_auth.rs` | Deployer initialised with `Upgrader` role & admin |
| `test_grant_and_revoke_upgrade_authorization` | `src/test_upgrade_auth.rs` | Role granting, revocation, and permission checks |
| `test_cannot_revoke_last_upgrade_admin` | `src/test_upgrade_auth.rs` | Rejection when trying to revoke sole upgrade admin |
| `test_upgrade_authorization_expiry` | `src/test_upgrade_auth.rs` | Expiry timestamp enforcement on upgrader role |
| `test_unauthorized_upgrade_attempts` | `src/test_upgrade_auth.rs` | Rejection of unauthorized & proposer-only upgrade calls |
| `test_upgrade_proposal_and_approval` | `src/test_upgrade_auth.rs` | Proposal creation, approval counting, and status transitions |
| `test_upgrade_execution_with_proposal` | `src/test_upgrade_auth.rs` | Execution of approved proposal & history tracking |
| `test_upgrade_replay_prevention_surfaces_typed_error` | `src/test_upgrade_auth.rs` | SHA-256 replay guard rejection on duplicate execution |
| `test_upgrade_admin_transfer_full_flow` | `src/test_admin_transfer.rs` | 2-step admin transfer flow |
| `test_upgrade_admin_transfer_timelock_enforced` | `src/test_admin_transfer.rs` | 24-hour timelock delay enforcement on admin transfer |
| `test_upgrade_admin_transfer_expiry_enforced` | `src/test_admin_transfer.rs` | 7-day proposal expiry enforcement |
| `test_upgrade_admin_transfer_wrong_acceptor` | `src/test_admin_transfer.rs` | Rejection when unauthorized caller attempts acceptance |
