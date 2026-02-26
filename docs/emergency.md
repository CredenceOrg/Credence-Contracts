# Emergency Pause Mechanism

## Overview

All Credence contracts include a comprehensive emergency pause mechanism that allows authorized parties to temporarily halt all state-changing operations while preserving read access. This mechanism provides a critical safety layer for emergency situations.

## Architecture

### Core Components

1. **Pause State**: A boolean flag stored in contract storage indicating whether the contract is paused
2. **Pause Signers**: Authorized addresses that can propose pause/unpause actions
3. **Pause Threshold**: Minimum number of approvals required to execute pause/unpause proposals
4. **Proposal System**: Multi-signature workflow for pause/unpause decisions

### Contracts with Pause Mechanism

- `credence_registry` - Identity registration management
- `credence_arbitration` - Dispute resolution system  
- `credence_delegation` - Attestation delegation management
- `credence_treasury` - Fee collection and withdrawal management
- `credence_bond` - Identity bond creation and management
- `admin` - Admin role management system

## Pause Mechanism API

### Core Functions

#### `is_paused() -> bool`
Returns the current pause state of the contract.

#### `pause(caller: Address) -> Option<u64>`
Proposes to pause the contract. Returns proposal ID if multi-sig is required, None if threshold is 0.

#### `unpause(caller: Address) -> Option<u64>`
Proposes to unpause the contract. Returns proposal ID if multi-sig is required, None if threshold is 0.

### Multi-signature Management

#### `set_pause_signer(admin: Address, signer: Address, enabled: bool)`
Add or remove pause signers. Only contract admins can manage signers.

#### `set_pause_threshold(admin: Address, threshold: u32)`
Set the minimum number of approvals required. Threshold cannot exceed signer count.

#### `approve_pause_proposal(signer: Address, proposal_id: u64)`
Approve a pause/unpause proposal. Only authorized pause signers can approve.

#### `execute_pause_proposal(proposal_id: u64)`
Execute a pause/unpause proposal once sufficient approvals are collected.

## Operational Modes

### Mode 1: Admin-only (Threshold = 0)
- Single admin can pause/unpause immediately
- No proposal system required
- Fastest response time for emergencies

### Mode 2: Multi-signature (Threshold > 0)
- Requires multiple approvals for pause/unpause
- Proposal-based workflow
- Higher security through distributed control

## Security Considerations

### Pause State Behavior
- **When Paused**: All state-changing functions are blocked with `contract is paused` error
- **When Paused**: Read-only functions continue to work normally
- **When Paused**: Pause management functions remain available for recovery

### Authorization Model
- **Admin Functions**: Require SuperAdmin role (in admin contract) or Admin address (in other contracts)
- **Pause Signers**: Can be any addresses set by contract admins
- **Self-protection**: Pause mechanism cannot be used to block itself

### Event Emission
All pause operations emit events for audit trails:
- `paused(proposal_id)` - Contract paused
- `unpaused(proposal_id)` - Contract unpaused  
- `pause_proposed(proposal_id, action)` - New proposal created
- `pause_approved(proposal_id, signer)` - Proposal approved
- `pause_signer_set(signer, enabled)` - Signer status changed
- `pause_threshold_set(threshold)` - Threshold updated

## Emergency Response Procedure

### Immediate Response (Admin-only Mode)
```rust
// Admin immediately pauses the contract
contract.pause(admin_address);
```

### Coordinated Response (Multi-sig Mode)
```rust
// Multiple signers approve pause proposal
let proposal_id = contract.pause(signer1_address);
contract.approve_pause_proposal(signer2_address, proposal_id);
contract.approve_pause_proposal(signer3_address, proposal_id);
contract.execute_pause_proposal(proposal_id);
```

### Recovery Process
```rust
// Once emergency is resolved
let proposal_id = contract.unpause(signer1_address);
contract.approve_pause_proposal(signer2_address, proposal_id);
contract.execute_pause_proposal(proposal_id);
```

## Configuration Recommendations

### Production Environment
- Set threshold to majority of signers (e.g., 3 of 5)
- Use geographically distributed signers
- Regularly test pause/unpause procedures
- Monitor pause events in real-time

### Development Environment
- Set threshold to 0 for rapid testing
- Use single admin account
- Test both paused and unpaused states

## Testing Coverage

All pause mechanism implementations include comprehensive tests covering:
- Basic pause/unpause functionality
- Multi-signature proposal workflow
- Threshold enforcement
- Read-only operation preservation
- State-changing operation blocking
- Error conditions and edge cases

## Integration Notes

### Contract Integration Pattern
Each contract follows the same integration pattern:
1. Add pause-related `DataKey` entries to storage enum
2. Initialize pause state in contract `initialize()` function
3. Add `pausable::require_not_paused(&e)` to all state-changing functions
4. Expose pause management entrypoints
5. Include comprehensive test coverage

### Upgrade Compatibility
The pause mechanism is designed to be:
- Backward compatible with existing contracts
- Non-disruptive to current functionality
- Easily enabled/disabled through configuration
- Upgrade-safe through storage versioning

## Monitoring and Alerting

### Key Metrics to Monitor
- Pause state changes
- Proposal creation and approval rates
- Threshold configuration changes
- Signer status modifications

### Alert Conditions
- Contract enters paused state
- High rate of pause proposals
- Failed pause execution attempts
- Unauthorized pause access attempts

## Troubleshooting

### Common Issues

**Contract won't pause**
- Verify caller is authorized admin or pause signer
- Check if threshold is met for multi-sig mode
- Ensure contract is not already paused

**Contract won't unpause**  
- Verify sufficient approvals for proposal
- Check proposal ID is valid
- Ensure proposal has not already been executed

**Read operations failing**
- Read operations should always work when paused
- Check if function incorrectly includes pause check
- Verify error is not from other validation logic

### Emergency Recovery
If pause mechanism becomes inaccessible:
1. Contract upgrade can reset pause state
2. Admin transfer can restore access
3. Multi-sig threshold can be reduced to 0
4. Last resort: contract migration to new instance

## Future Enhancements

Planned improvements to the pause mechanism:
- Time-based automatic unpause
- Granular pause levels (partial functionality)
- Emergency override keys
- Cross-contract coordinated pausing
- Integration with external monitoring systems
# Emergency Withdrawal System

Emergency withdrawal is a crisis-only escape hatch that lets governance execute withdrawals with elevated approval while preserving a complete on-chain audit trail.

## Goals

- Allow emergency withdrawals during extreme scenarios.
- Require elevated governance approval (admin + governance).
- Apply configurable emergency fee.
- Emit explicit emergency events.
- Persist immutable audit records for every emergency execution.

## Configuration

Set once (and update when needed) via:

- `set_emergency_config(admin, governance, treasury, emergency_fee_bps, enabled)`

Rules:

- `admin` must be the initialized contract admin.
- `emergency_fee_bps` must be `<= 10000`.
- `governance` becomes the required second approver.
- `enabled` controls whether emergency withdrawals are currently allowed.

Emergency mode can be toggled with elevated approval:

- `set_emergency_mode(admin, governance, enabled)`

## Execution Flow

Emergency withdrawal entrypoint:

- `emergency_withdraw(admin, governance, amount, reason)`

Validation order:

1. Verify `admin` is the stored admin.
2. Verify `governance` matches configured governance address.
3. Verify emergency mode is enabled.
4. Verify `amount > 0`.
5. Verify available balance (`bonded_amount - slashed_amount`) covers `amount`.

Fee and accounting:

- `fee_amount = amount * emergency_fee_bps / 10000`
- `net_amount = amount - fee_amount`
- Bond principal is reduced by `amount`.

## Audit Trail

Each emergency execution writes an immutable record with incrementing id:

- `id`
- `identity`
- `gross_amount`
- `fee_amount`
- `net_amount`
- `treasury`
- `approved_admin`
- `approved_governance`
- `reason`
- `timestamp`

Accessors:

- `get_latest_emergency_record_id()`
- `get_emergency_record(id)`

## Events

- `emergency_mode(enabled, admin, governance, timestamp)`
- `emergency_withdrawal(record_id, identity, gross_amount, fee_amount, net_amount, reason, timestamp)`

## Security Notes

- Elevated approval is enforced by requiring both admin and governance addresses.
- Emergency path is hard-gated by `enabled` mode to avoid accidental use.
- Arithmetic uses checked operations for overflow/underflow-sensitive paths.
- Withdrawal respects slashed-balance invariant (`slashed_amount <= bonded_amount`).
- Immutable records + events provide forensic traceability for incident response.

### Validated Assumptions

- **Assumption: only authorized operators can trigger emergency controls.**
	- Validated by tests: `test_set_emergency_config_rejects_non_admin`, `test_set_emergency_mode_rejects_wrong_governance`, `test_emergency_withdraw_requires_governance_approver`.
- **Assumption: emergency path cannot be used unless explicitly enabled.**
	- Validated by test: `test_emergency_withdraw_rejected_when_disabled`.
- **Assumption: withdrawals cannot exceed safe available balance after slashing.**
	- Validated by test: `test_emergency_withdraw_respects_slashed_available_balance`.
- **Assumption: fee configuration and withdrawal inputs are bounded/sane.**
	- Validated by tests: `test_set_emergency_config_rejects_invalid_fee_bps`, `test_emergency_withdraw_rejects_non_positive_amount`.

## Test Coverage (Emergency)

Emergency tests validate:

- Successful emergency withdrawal and exact fee math.
- Incrementing audit record ids and record integrity.
- Elevated approval checks (`not admin`, `not governance`).
- Emergency mode gating (`emergency mode disabled`).
- Balance safety under slashing constraints.
- Invalid amount and invalid fee configuration rejection.

## Verification Snapshot (2026-02-25)

- `cargo test -p credence_bond`: **305 passed, 0 failed**.
- `cargo test --all-targets`: **passed** (workspace test targets).
- `cargo llvm-cov -p credence_bond --summary-only`:
	- **TOTAL**: 95.82% region coverage, 94.14% line coverage.
	- **emergency.rs**: 94.92% region coverage, 95.31% line coverage.
- CI-equivalent core checks from `.github/workflows/ci.yml`:
	- `cargo fmt --all -- --check`: **passed**
	- `cargo build --all-targets`: **passed**
	- `cargo test --all-targets`: **passed**
	- `cargo build --release`: **passed**
- Security checks from `.github/workflows/security.yml`:
	- `cargo audit`: **passed** (2 non-critical unmaintained dependency warnings, no critical vulnerabilities)
	- `cargo clippy ... -D warnings` with security lints: **fails on pre-existing repository-wide lints** (not emergency-specific)
	- `cargo geiger`: **reports unsafe usage in dependency tree and exits with warnings**
