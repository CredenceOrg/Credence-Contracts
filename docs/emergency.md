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
