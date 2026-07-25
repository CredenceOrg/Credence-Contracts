# Threat Model — Credence Bond & Delegation

**Status:** Living artifact (canonical threat register)  
**Last Updated:** 2026-07-25
**Scope:** Bond operations (`credence_bond`), delegation (`credence_delegation`), treasury guardrails

This document enumerates security threats to the Credence bond and delegation system, mitigations applied, and test fixtures that validate each mitigation. Each threat row is referenced by test comments via `/// THREAT: T-XXX` to establish bidirectional traceability.

---

## Threat Registry

| ID | Asset | Attacker | Attack Vector | Impact | Mitigation | Test Fixture(s) | Status |
|:---|:------|:---------|:--------------|:-------|:-----------|:-----------------|:-------|
| **T-001** | Bond principal | Unauthorized user | Direct slash via unsigned call | Loss of bonded funds | Role-based access control; `require_auth` on admin slash | `contracts/credence_bond/src/test_slashing.rs::test_slash_unauthorized_rejection` | ✅ Covered |
| **T-002** | Bond principal | Unauthorized user | Modify bond parameters (duration, tier, etc.) without authorization | Unfair bond terms | Admin-only config setters with role checks | `contracts/credence_bond/src/test_access_control.rs::test_exhaustive_non_admin_rejected` | ✅ Covered |
| **T-003** | Bonded funds | Unauthorized user | Create bonds on behalf of other identities | Fraudulent lockup | Identity authorization is required for bond creation | `contracts/credence_bond/src/test_auth.rs::create_bond_succeeds_when_identity_authorizes` | ✅ Covered |
| **T-004** | Bond arithmetic state | Attacker (overflow) | Craft maximum bond + top-ups to cause i128 overflow | Funds stolen via underflow bypass | Checked arithmetic (`checked_add`, `checked_sub`); panic on overflow | `contracts/credence_bond/src/security/test_arithmetic.rs::test_i128_overflow_on_top_up` | ✅ Covered |
| **T-005** | Bond arithmetic state | Attacker (underflow) | Craft slashes + withdrawals to cause i128 underflow | Negative balances; theft | Checked arithmetic; underflow panics | `contracts/credence_bond/src/security/test_arithmetic.rs::test_withdrawal_exceeds_available_balance` | ✅ Covered |
| **T-006** | Bond arithmetic state | Attacker | Timestamp duration overflow (u64::MAX + duration) | Bond expiry bypass | Checked addition on `start + duration`; panic if overflows | `contracts/credence_bond/src/security/test_arithmetic.rs::test_u64_overflow_on_duration_extension` | ✅ Covered |
| **T-007** | Bond invariant (I2) | Attacker | Slash bond for amount > principal | State corruption: slashed > bonded | Slash amounts above the bonded amount are rejected | `contracts/credence_bond/src/security/test_arithmetic.rs::test_slashing_exceeds_bonded_amount` | ✅ Covered |
| **T-008** | Bond invariant (I4) | Attacker | Withdrawal > bonded - slashed | Negative bonded; stolen funds | Checked subtraction: available = bonded - slashed | `contracts/credence_bond/src/security/test_arithmetic.rs::test_withdrawal_exceeds_available_balance` | ✅ Covered |
| **T-009** | Fee collection | Reentrancy attacker | Reenter withdraw_bond during fee callback to drain treasury | Protocol fee theft | Reentrancy guard + checks-effects-interactions; state updated before callback | `contracts/credence_bond/src/test_reentrancy.rs::test_fee_collection_reentrancy_blocked` | ✅ Covered |
| **T-010** | Bond state | Reentrancy attacker | Reenter slash_bond during transfer callback | Double slash same bond; state corruption | Reentrancy guard; atomic slash then transfer | `contracts/credence_bond/src/test_reentrancy.rs::test_slash_reentrancy_blocked` | ✅ Covered |
| **T-011** | Nonce sequencing | Replay attacker | Replay signed attestation (nonce + deadline + data) | Duplicate attestation; false evidence | Nonce incremented atomically; second call fails "invalid nonce" | `contracts/credence_bond/src/test_replay_prevention.rs::replay_add_attestation_rejected` | ✅ Covered |
| **T-012** | Nonce sequencing | Replay attacker | Reorder attestations to consume nonce out-of-order | Early attestations rejected; trust broken | Strict nonce matching; out-of-order calls rejected | `contracts/credence_bond/src/test_replay_prevention.rs::wrong_nonce_rejected` | ✅ Covered |
| **T-013** | Contract domain | Cross-chain attacker | Replay nonce/deadline pair across different deployed contracts | Fund theft from sibling deployment | Contract ID validated against current address | `contracts/credence_bond/src/test_replay_prevention.rs::wrong_contract_address_rejected_on_add_attestation` | ✅ Covered |
| **T-014** | Attestation integrity | Unauthorized attester | Add attestations without admin registration | False claims recorded | Only registered attesters pass authorization checks | `contracts/credence_bond/src/test_attestation.rs::test_unregistered_attester_cannot_attest` | ✅ Covered |
| **T-015** | Attestation integrity | Attester | Duplicate attestations for same subject + data | Inflated weight; unfair tier | Duplicate check: `(verifier, subject, data)` unique key | `contracts/credence_bond/src/test_attestation.rs::test_duplicate_attestation_rejected` | ✅ Covered |
| **T-016** | Attestation weight | Attacker | Craft a weight below the protocol default | Underweighted attestations; broken tier system | Computed weight is bounded by the configured default | `contracts/credence_bond/src/test_weighted_attestation.rs::weight_always_at_least_default` | ✅ Covered |
| **T-017** | Tier assignment | Attacker | Threshold not validated; arbitrary tier assignment | Over-leveraged bonds; default | Tier computed from configured amount thresholds | `contracts/credence_bond/src/test_tiered_bond.rs::test_tier_thresholds` | ✅ Covered |
| **T-018** | Rolling bond invariant (I3) | Attacker | Request withdrawal on fixed-duration bond | Invalid state; withdrawal becomes claimable when it shouldn't | Fixed bonds reject `request_withdrawal` with error | `contracts/credence_bond/src/test_rolling_bond.rs::test_request_withdrawal_non_rolling` | ✅ Covered |
| **T-019** | Rolling bond invariant (I6) | Attacker | Create a rolling bond with an invalid notice period | Notice never clears; funds locked indefinitely | Rolling bonds require a valid non-zero notice period | `contracts/credence_bond/src/test_batch.rs::test_rolling_bond_without_notice_period_fails` | ✅ Covered |
| **T-020** | Early exit penalty | Attacker | Exit early without penalty | Penalty evasion; unfair exit | Penalty deducted from available balance before withdrawal | `contracts/credence_bond/src/test_withdraw_bond.rs::test_withdraw_early_before_lockup_applies_penalty` | ✅ Covered |
| **T-021** | Supply cap | Attacker | Create bonds totaling > supply cap | Excessive leverage | Supply cap enforced on bond creation; rejected if exceeded | `contracts/credence_bond/src/test_create_bond.rs::test_supply_cap_enforcement_above_cap` | ✅ Covered |
| **T-022** | Cooldown timing | Attacker | Withdraw within cooldown period | Rapid re-entry; unfair advantage | Cooldown recorded; withdrawal rejected if still in cooldown | `contracts/credence_bond/src/test_withdraw_bond.rs::test_withdraw_bond_rolling_before_cooldown_panics` | ✅ Covered |
| **T-023** | Grace window (nonce) | Attacker | Submit attestation after grace window expiration | Old attestations reused; stale evidence | Deadline checked before nonce consumed; deadline < now panics | `contracts/credence_bond/src/test_grace_period.rs::strict_mode_rejects_one_second_past_deadline` | ✅ Covered |
| **T-024** | Same-ledger invariant | Sandwich attacker | Slash immediately after bond creation (same ledger) | Unfair liquidation; slashed without chance to adjust | Same-ledger guard: slash rejected if `last_collateral_increase` is current | `contracts/credence_bond/src/test_same_ledger_liquidation_guard.rs::test_slash_same_ledger_after_create_bond_rejected` | ✅ Covered |
| **T-025** | Bond lifecycle | Attacker | Withdraw a fixed-duration bond before maturity | Premature liquidation | Fixed bonds enforce their lockup boundary | `contracts/credence_bond/src/test_withdraw_bond.rs::test_withdraw_bond_before_lockup_panics` | ✅ Covered |
| **T-026** | Emergency mode | Malicious admin | Activate emergency mode to freeze all bonds indefinitely | Denial of service; funds locked | Emergency transitions are recorded and mode can be disabled | `contracts/credence_bond/src/test_emergency.rs::test_set_emergency_mode_records_transition` | ✅ Covered |
| **T-027** | Emergency withdrawal | Unauthorized user | Call emergency_withdraw during non-emergency | Unauthorized fund extraction | Emergency withdrawals are rejected while disabled | `contracts/credence_bond/src/test_emergency.rs::test_emergency_withdraw_rejected_when_disabled` | ✅ Covered |
| **T-028** | Fee collection | Admin | Reuse an idempotency key to collect twice | Double-fee theft | Duplicate idempotency keys are rejected | `contracts/credence_bond/src/idempotency.rs::test_check_and_record_prevents_duplicate` | ✅ Covered |
| **T-029** | Pause mechanism | Attacker | Bypass pause to execute state-changing calls | Paused contract compromise | All state-changing calls check `!paused()` guard | `contracts/credence_bond/src/test_pausable.rs::test_pause_rejects_state_mutating_entrypoints` | ✅ Covered |
| **T-030** | Pause signer | Attacker | Pause contract without valid multi-sig threshold | Unauthorized freeze | Pause requires threshold approval from authorized signers | `contracts/credence_delegation/src/test_pause_snapshots.rs::pause_proposal_remains_pending_when_approvals_are_below_threshold` | ✅ Covered |
| **T-031** | Verifier management | Unauthorized user | Register an attester without admin authorization | Broken trust chain | Attester registration requires admin authorization | `contracts/credence_bond/src/test_auth.rs::register_attester_requires_admin_auth` | ✅ Covered |
| **T-032** | Governance vote | Unauthorized user | Vote on a slash proposal without governor authority | Frivolous or manipulated governance outcomes | Only governors or valid delegates may vote | `contracts/credence_bond/src/test_governance_approval.rs::test_non_governor_cannot_vote` | ✅ Covered |
| **T-033** | Governance execution | Attacker | Execute a rejected governance slash proposal | Unapproved slash; unfair loss | Rejected proposals cannot be executed | `contracts/credence_bond/src/test_governance_approval.rs::test_vote_reject_then_execute_fails` | ✅ Covered |
| **T-034** | Batch atomicity | Attacker | Create batch with one failing sub-operation; partial state update | Inconsistent state | Batch operations atomic: all-or-nothing via revert | `contracts/credence_bond/src/test_batch.rs::test_atomic_failure_on_second_bond` | ✅ Covered |
| **T-035** | Batch pagination | Attacker (pagination) | Query claims with a cursor past the final item | Incorrect or duplicated claim extraction | Out-of-range cursors return an empty page | `contracts/credence_bond/src/test_pending_claims_page.rs::start_after_past_the_end_returns_empty` | ✅ Covered |
| **T-036** | Basis point math | Attacker | Craft fees/penalties with rounding inconsistencies | Fee bypass; accounting drift | Fee and net components are checked to sum to the original amount | `contracts/credence_bond/src/test_bps_denominator.rs::split_bps_net_plus_fee_equals_amount` | ✅ Covered |
| **T-037** | Zero address | Attacker | Set bond token or treasury to address(0) | Transfers to void; state corruption | Zero address guards on parameter setters | `contracts/credence_bond/src/test_zero_address.rs::test_set_token_rejects_zero_address` | ✅ Covered |
| **T-038** | Delegation TTL | Attacker | Replay a delegation after it expires | Unauthorized delegation; re-entry | Delegation expiry checked before execution | `contracts/credence_delegation/src/test.rs::test_is_valid_delegate_after_expiry` | ✅ Covered |
| **T-039** | Domain separation (delegation) | Cross-contract attacker | Replay delegation across different contract domains | Sibling contract compromise | Contract domain is validated on execution | `contracts/credence_delegation/src/test_domain_separation.rs::cross_contract_replay_rejected` | ✅ Covered |
| **T-040** | Token custody | External token | Receive fee-on-transfer token; fee charged twice | Protocol fee reduced; state mismatch | Balance delta is verified after token transfer | `contracts/credence_bond/src/token_integration_test.rs::test_fee_on_transfer_detection_prevents_silent_success` | ✅ Covered |
| **T-041** | Attestation revocation | Unauthorized user | Revoke another attester's attestation | Trust chain broken; false negation | Revocation requires the original attester | `contracts/credence_bond/src/test_attestation.rs::test_revoke_wrong_attester` | ✅ Covered |
| **T-042** | Evidence preservation | Attacker | Submit duplicate evidence to overwrite an existing record | Audit trail corruption | Duplicate evidence hashes are rejected | `contracts/credence_bond/src/test_evidence.rs::test_submit_duplicate_hash` | ✅ Covered |
| **T-043** | Bond lockup gate | Attacker | Claim locked bond before lockup expires | Premature withdrawal; theft | Lockup expiry validated; withdrawal rejected if locked | `contracts/credence_bond/src/test_withdraw_bond.rs::test_withdraw_bond_before_lockup_panics` | ✅ Covered |
| **T-044** | Admin ownership | Attacker | Hijack administration during ownership transfer | Unauthorized access | Admin transfer requires both current and new admin authorization | `contracts/credence_bond/src/test_auth.rs::transfer_admin_succeeds_when_both_parties_authorize` | ✅ Covered |
| **T-045** | Treasury slippage | Attacker | Craft liquidation order to extract more than queued | Arbitrage theft | Withdrawal guardrails; slippage cap enforced | `contracts/credence_treasury/src/test_withdrawal_guardrails.rs::test_slippage_guard_rejects_higher_minimum` | ✅ Covered |
| **T-046** | Flash loan attack | Attacker | Borrow from treasury and fail to satisfy repayment rules | Protocol fee loss; treasury drained | Flash-loan success path enforces repayment and fees | `contracts/credence_treasury/src/test_flash_loan.rs::test_flash_loan_success` | ✅ Covered |
| **T-047** | Decimal normalization | Attacker | Craft tokens with unusual decimals to bypass amount checks | Rounding errors; theft | Decimal normalization preserves accounting invariants | `contracts/credence_bond/src/test_decimal_normalization.rs::test_invariant_preservation_across_decimals` | ✅ Covered |
| **T-048** | Duration validation | Attacker | Set zero bond duration | Maturity bypass; lockup shortcut | Duration > 0 validated at creation | `contracts/credence_bond/src/test_duration_validation.rs::test_create_bond_zero_duration_rejected` | ✅ Covered |
| **T-049** | Immutable config | Attacker | Reinitialize contract configuration after deployment | Governance violated; parameters changed unfairly | Initialization can only occur once | `contracts/credence_bond/src/test_immutable_config.rs::test_admin_cannot_be_reinitialized` | ✅ Covered |
| **T-050** | Long-horizon bonds | Attacker | Create bonds with extreme durations | Maturity computation overflow; lock forever | Extreme durations are rejected by validation | `contracts/credence_bond/src/test_duration_validation.rs::test_validate_duration_u64_max` | ✅ Covered |

---

## Threat Coverage by Module

### credence_bond

| Module | Threats | Test Commands |
|:-------|:--------|:--------------|
| Access Control | T-001, T-002, T-003, T-014, T-031 | `cargo test -p credence_bond test_access_control` |
| Arithmetic Safety | T-004, T-005, T-006 | `cargo test -p credence_bond security::test_arithmetic` |
| Slashing & Invariants | T-007, T-008, T-016, T-017 | `cargo test -p credence_bond test_slashing test_weighted_attestation` |
| Reentrancy | T-009, T-010 | `cargo test -p credence_bond test_reentrancy` |
| Replay Prevention | T-011, T-012, T-013 | `cargo test -p credence_bond test_replay_prevention` |
| Attestation | T-014, T-015, T-016, T-041 | `cargo test -p credence_bond test_attester test_weighted_attestation` |
| Bond Lifecycle | T-018, T-019, T-020, T-022, T-025 | `cargo test -p credence_bond test_rolling_bond test_claim test_early_exit_penalty` |
| Same-Ledger Guard | T-024 | `cargo test -p credence_bond test_same_ledger_liquidation_guard` |
| Emergency & Pause | T-026, T-027, T-029 | `cargo test -p credence_bond test_emergency test_pausable` |
| Governance | T-032, T-033 | `cargo test -p credence_bond integration::test_governance` |
| Batch Operations | T-034, T-035 | `cargo test -p credence_bond test_batch test_claim_pagination` |
| Configuration & Params | T-006, T-023, T-037, T-048, T-050 | `cargo test -p credence_bond test_parameters test_duration_validation` |
| Fees & Penalties | T-028, T-036, T-040 | `cargo test -p credence_bond test_fees test_bps_denominator test_token_custody` |
| Evidence & Audit | T-042 | `cargo test -p credence_bond test_evidence` |
| Advanced | T-043, T-044, T-047, T-049 | `cargo test -p credence_bond test_ownership_transfer test_decimal_normalization test_immutable_config` |

### credence_delegation

| Module | Threats | Test Commands |
|:-------|:--------|:--------------|
| Pause Mechanism | T-030 | `cargo test -p credence_delegation test_pause_signer_invariant` |
| Delegation TTL | T-038 | `cargo test -p credence_delegation test_delegation_ttl` |
| Domain Separation | T-039 | `cargo test -p credence_delegation test_domain_separation` |

### credence_treasury

| Module | Threats | Test Commands |
|:-------|:--------|:--------------|
| Slippage & Guardrails | T-045 | `cargo test -p credence_treasury test_withdrawal_guardrails` |
| Flash Loans | T-046 | `cargo test -p credence_treasury test_flash_loan` |

---

## Linking Tests to Threats

Each test function **must** begin with a comment block listing the threats it covers:

```rust
/// THREAT: T-001, T-002
/// Validates that role-based access control prevents unauthorized admin operations.
#[test]
fn test_only_admin_can_slash() {
    // test body
}
```

### Guidelines

- **One threat per line:** If a test covers multiple threats, list each on its own line after `THREAT:`.
- **Top of test function:** Threat annotation must appear immediately before `#[test]` or at the very top of the function.
- **Format:** `/// THREAT: T-NNN` (comment style matches module convention).
- **Bidirectional traceability:** The threat registry row points to the test; the test comments point back via threat ID.

---

## Living Maintenance

### Adding a New Threat

1. Assign next ID (e.g., T-051).
2. Fill in row: asset, attacker profile, vector, impact, mitigation, test fixture.
3. Create or update test with threat annotation.
4. Run `cargo test threats_link` to verify the test exists.
5. Commit with message: `docs(contracts): add threat T-NNN — [description]`.

### Updating a Mitigation

1. Locate threat row by ID.
2. Update mitigation column.
3. Verify test still passes.
4. If test name changes, update test fixture column.
5. Commit with message: `docs(contracts): revise T-NNN mitigation — [reason]`.

### Retiring a Threat

1. Mark row as `⚠ Archived` in Status column.
2. Add a note: "**Archived:** [reason], replaced by T-XXX on [date]."
3. Keep row in table for historical audit trail.
4. Do not remove tests (they provide regression protection).

---

## Validation

Bidirectional consistency is enforced via the `tests/threats_link.rs` test:

- ✅ Parses THREATS.md table.
- ✅ Verifies each referenced test exists.
- ✅ Verifies each test begins with a `/// THREAT: T-NNN` comment.
- ✅ Ensures threat IDs and test mappings match.
- ✅ Fails if a threat row references a non-existent test.
- ✅ Fails if a test is missing threat annotation.

**Usage:**

```bash
cargo test -p credence_bond threats_link -- --nocapture
```

---

## Related Documents

- [Bond Invariants](docs/bond-invariants.md) — Formal invariants guarding bond state.
- [Security](docs/security.md) — Overflow-safe arithmetic, replay prevention, reentrancy guards.
- [SECURITY.md](SECURITY.md) — Access control matrix and role hierarchy.
- [SECURITY_ANALYSIS.md](SECURITY_ANALYSIS.md) — Arithmetic security deep dive.
