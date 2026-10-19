# Gas Budget Breakdown

> Soroban SDK v23 · Measured with `env.budget()` in test simulation · Date: 2026-07-25

Approximate resource costs per entrypoint across all Credence contracts. Costs are categorised by operation type — adjust budgets proportionally when batching multiple calls in a single transaction.

## Cost Tiers

| Tier | CPU Range | Memory Range | Typical Operations |
| --- | --- | --- | --- |
| **Instance read** | 14k–22k | 1.4k–3k | Instance storage read (counter, config) |
| **Persistent read** | 20k–48k | 3k–6k | Persistent entry read + TTL bump |
| **Admin config write** | 40k–80k | 6k–12k | Single storage write, auth check |
| **Business logic write** | 80k–150k | 12k–25k | Multi-step logic, auth, event emission |
| **Token-transfer write** | 150k–320k | 25k–46k | Cross-contract token transfer involved |

---

## admin

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `version` | Instance read | 15,000 | 1,500 |
| `initialize` | Admin config write | 60,000 | 9,000 |
| `add_admin` | Admin config write | 55,000 | 8,000 |
| `remove_admin` | Admin config write | 50,000 | 7,500 |
| `update_admin_role` | Admin config write | 50,000 | 7,500 |
| `deactivate_admin` | Admin config write | 45,000 | 7,000 |
| `reactivate_admin` | Admin config write | 45,000 | 7,000 |
| `suspend_admin` | Admin config write | 50,000 | 7,500 |
| `transfer_ownership` | Admin config write | 55,000 | 8,000 |
| `accept_ownership` | Admin config write | 50,000 | 7,500 |
| `get_owner` | Instance read | 15,000 | 1,500 |
| `get_pending_owner` | Instance read | 15,000 | 1,500 |
| `get_admin_info` | Persistent read | 25,000 | 3,500 |
| `get_admin_role` | Persistent read | 22,000 | 3,000 |
| `is_admin` | Persistent read | 22,000 | 3,000 |
| `has_role_at_least` | Persistent read | 25,000 | 3,500 |
| `check_role_at_ledger` | Persistent read | 30,000 | 4,500 |
| `get_all_admins` | Persistent read | 35,000 | 5,000 |
| `get_admins_by_role` | Persistent read | 30,000 | 4,000 |
| `get_admin_count` | Instance read | 15,000 | 1,500 |
| `get_active_admin_count` | Persistent read | 25,000 | 3,500 |
| `get_config` | Instance read | 15,000 | 1,500 |
| `get_role` | Persistent read | 22,000 | 3,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `pause` | Business logic write | 70,000 | 10,000 |
| `unpause` | Business logic write | 70,000 | 10,000 |
| `set_pause_signer` | Admin config write | 50,000 | 7,500 |
| `set_pause_threshold` | Admin config write | 45,000 | 7,000 |
| `approve_pause_proposal` | Business logic write | 60,000 | 9,000 |
| `execute_pause_proposal` | Business logic write | 70,000 | 10,000 |

## credence_arbitration

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `initialize` | Admin config write | 55,000 | 8,000 |
| `register_arbitrator` | Admin config write | 50,000 | 7,500 |
| `unregister_arbitrator` | Admin config write | 45,000 | 7,000 |
| `create_dispute` | Token-transfer write | 310,000 | 45,000 |
| `cancel_dispute` | Business logic write | 80,000 | 12,000 |
| `vote` | Business logic write | 130,000 | 22,000 |
| `resolve_dispute` (no transfer) | Business logic write | 90,000 | 14,000 |
| `resolve_dispute` (with transfer) | Token-transfer write | 255,000 | 37,000 |
| `set_quorum` | Admin config write | 45,000 | 7,000 |
| `get_quorum` | Instance read | 15,000 | 1,500 |
| `get_dispute` | Persistent read | 48,000 | 5,800 |
| `get_tally` | Persistent read | 25,000 | 4,000 |
| `get_arbitrator_weight` | Persistent read | 22,000 | 3,500 |
| `has_voted` | Persistent read | 22,000 | 4,000 |
| `get_arbitrators_page` | Persistent read | 30,000 | 5,000 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `set_pause_signer` / `set_pause_threshold` | Admin config write | 50,000 | 7,500 |
| `approve_pause_proposal` / `execute_pause_proposal` | Business logic write | 60,000–70,000 | 9,000–10,000 |

## credence_bond

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `version` | Instance read | 15,000 | 1,500 |
| `initialize` / `initialize_with_registry` | Admin config write | 65,000 | 10,000 |
| `set_accepted_tokens` / `set_token` | Admin config write | 50,000 | 7,500 |
| `describe_config` | Instance read | 18,000 | 2,000 |
| `describe_bond` | Persistent read | 40,000 | 5,500 |
| `set_early_exit_config` | Admin config write | 50,000 | 7,500 |
| `is_borrow_frozen` | Instance read | 15,000 | 1,500 |
| `set_borrow_frozen` | Admin config write | 45,000 | 7,000 |
| `register_attester` / `unregister_attester` | Admin config write | 50,000 | 7,500 |
| `is_attester` | Persistent read | 22,000 | 3,000 |
| `create_bond` | Token-transfer write | 250,000 | 38,000 |
| `get_identity_state` | Persistent read | 40,000 | 5,500 |
| `add_attestation` | Business logic write | 110,000 | 18,000 |
| `add_attestation_batch` | Business logic write | 130,000 + 4k per item | 20,000 + 500 per item |
| `revoke_attestation` | Business logic write | 90,000 | 14,000 |
| `get_attestation` | Persistent read | 35,000 | 5,000 |
| `get_subject_attestations` | Persistent read | 35,000 | 5,000 |
| `get_subject_attestations_page` | Persistent read | 30,000 | 4,500 |
| `get_slash_history_page` | Persistent read | 30,000 | 4,500 |
| `get_subject_attestation_count` | Persistent read | 22,000 | 3,000 |
| `get_nonce` | Persistent read | 22,000 | 3,000 |
| `get_grace_window` | Instance read | 15,000 | 1,500 |
| `set_grace_window` | Admin config write | 45,000 | 7,000 |
| `set_attester_stake` | Admin config write | 50,000 | 7,500 |
| `set_weight_config` | Admin config write | 45,000 | 7,000 |
| `transfer_admin` / `transfer_upgrade_admin` | Admin config write | 55,000 | 8,000 |
| `accept_upgrade_admin` / `cancel_upgrade_admin_transfer` | Admin config write | 50,000 | 7,500 |
| `get_pending_upgrade_admin` | Instance read | 15,000 | 1,500 |
| `get_weight_config` | Instance read | 15,000 | 1,500 |
| `withdraw` | Business logic write | 100,000 | 16,000 |
| `withdraw_early` | Business logic write | 120,000 | 18,000 |
| `request_withdrawal` | Business logic write | 85,000 | 13,000 |
| `renew_if_rolling` | Business logic write | 80,000 | 12,000 |
| `get_tier` | Persistent read | 30,000 | 4,000 |
| `slash` | Business logic write | 100,000 | 15,000 |
| `top_up` | Token-transfer write | 200,000 | 30,000 |
| `extend_duration` | Business logic write | 85,000 | 13,000 |
| `deposit_fees` | Token-transfer write | 180,000 | 28,000 |
| `withdraw_bond` | Business logic write | 110,000 | 17,000 |
| `slash_bond` | Business logic write | 110,000 | 17,000 |
| `collect_fees` | Business logic write | 90,000 | 14,000 |
| `set_liquidation_treasury` / `set_slash_treasury` | Admin config write | 45,000 | 7,000 |
| `get_liquidation_treasury` / `get_slash_treasury` | Instance read | 15,000 | 1,500 |
| `is_liquidated` | Persistent read | 22,000 | 3,000 |
| `liquidate` | Business logic write | 95,000 | 15,000 |
| `set_callback` | Admin config write | 45,000 | 7,000 |
| `is_locked` / `is_paused` | Instance read | 15,000 | 1,500 |
| `expire_claims` | Business logic write | 70,000 + 5k per claim | 10,000 + 800 per claim |
| `get_pending_claims_page` | Persistent read | 30,000 | 4,500 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `schedule_emergency_drain` | Business logic write | 60,000 | 9,000 |
| `cancel_emergency_drain` | Business logic write | 55,000 | 8,500 |
| `emergency_drain_to_treasury` | Token-transfer write | 200,000 | 30,000 |
| `get_drain_eta` / `get_latest_drain_id` / `get_drain_record` | Persistent read | 22,000–35,000 | 3,000–5,000 |

## credence_delegation

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `version` | Instance read | 15,000 | 1,500 |
| `initialize` | Admin config write | 55,000 | 8,000 |
| `delegate` | Business logic write | 100,000 | 16,000 |
| `revoke_delegation` | Business logic write | 85,000 | 13,000 |
| `revoke_attestation` | Business logic write | 80,000 | 12,000 |
| `execute_delegated_delegate` | Business logic write | 110,000 | 18,000 |
| `execute_delegated_revoke` | Business logic write | 95,000 | 15,000 |
| `execute_delegated_revoke_attest` | Business logic write | 90,000 | 14,000 |
| `get_delegation_summary` | Persistent read | 30,000 | 4,500 |
| `cleanup_expired` | Business logic write | 60,000 | 9,000 |
| `get_delegation` | Persistent read | 25,000 | 3,500 |
| `is_valid_delegate` | Persistent read | 25,000 | 3,500 |
| `check_delegation_active` | Persistent read | 25,000 | 3,500 |
| `get_attestation_status` | Persistent read | 25,000 | 3,500 |
| `set_revocation_grace_period` | Admin config write | 45,000 | 7,000 |
| `get_revocation_grace_period` | Instance read | 15,000 | 1,500 |
| `get_nonce` | Persistent read | 22,000 | 3,000 |
| `invalidate_nonce_range` | Business logic write | 70,000 | 11,000 |
| `register_verifier` | Admin config write | 50,000 | 7,500 |
| `get_verifier` | Persistent read | 22,000 | 3,000 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `set_pause_signer` / `set_pause_threshold` | Admin config write | 50,000 | 7,500 |
| `approve_pause_proposal` / `execute_pause_proposal` | Business logic write | 60,000–70,000 | 9,000–10,000 |
| `get_pause_proposal_state` / `get_proposal_by_legacy_id` | Persistent read | 30,000 | 4,500 |

## credence_registry

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `version` | Instance read | 15,000 | 1,500 |
| `initialize` | Admin config write | 55,000 | 8,000 |
| `register` | Business logic write | 70,000 | 11,000 |
| `get_bond_contract` | Persistent read | 25,000 | 3,500 |
| `get_identity` | Persistent read | 25,000 | 3,500 |
| `is_registered` | Persistent read | 22,000 | 3,000 |
| `deactivate` | Business logic write | 55,000 | 8,500 |
| `remove` | Business logic write | 60,000 | 9,000 |
| `reactivate` | Business logic write | 55,000 | 8,500 |
| `get_identities_page` | Persistent read | 30,000 | 4,500 |
| `get_all_identities` | Persistent read | 40,000 | 6,000 |
| `get_admin` | Instance read | 15,000 | 1,500 |
| `transfer_admin` | Admin config write | 50,000 | 7,500 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `set_pause_signer` / `set_pause_threshold` | Admin config write | 50,000 | 7,500 |
| `approve_pause_proposal` / `execute_pause_proposal` | Business logic write | 60,000–70,000 | 9,000–10,000 |
| `set_bond_code_hash` | Admin config write | 45,000 | 7,000 |
| `get_bond_code_hash` | Instance read | 15,000 | 1,500 |
| `register_trustless` | Business logic write | 75,000 | 12,000 |

## credence_treasury

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `version` | Instance read | 15,000 | 1,500 |
| `initialize` | Admin config write | 55,000 | 8,000 |
| `receive_fee` | Token-transfer write | 180,000 | 28,000 |
| `add_depositor` / `remove_depositor` | Admin config write | 50,000 | 7,500 |
| `add_signer` / `remove_signer` | Admin config write | 50,000 | 7,500 |
| `set_threshold` | Admin config write | 45,000 | 7,000 |
| `propose_withdrawal` | Business logic write | 65,000 | 10,000 |
| `approve_withdrawal` | Business logic write | 55,000 | 8,500 |
| `execute_withdrawal` | Token-transfer write | 200,000 | 32,000 |
| `get_token` | Instance read | 15,000 | 1,500 |
| `set_token` | Admin config write | 45,000 | 7,000 |
| `set_min_liquidity` | Admin config write | 45,000 | 7,000 |
| `set_proposal_ttl` | Admin config write | 45,000 | 7,000 |
| `get_proposal_ttl` | Instance read | 15,000 | 1,500 |
| `get_min_liquidity` | Instance read | 15,000 | 1,500 |
| `get_balance` | Instance read | 18,000 | 2,000 |
| `get_balance_by_source` | Persistent read | 25,000 | 3,500 |
| `get_cumulative_received` | Instance read | 18,000 | 2,000 |
| `get_cumulative_by_source` | Persistent read | 25,000 | 3,500 |
| `get_cumulative_received_u256` | Instance read | 20,000 | 2,500 |
| `get_cumulative_by_source_u256` | Persistent read | 28,000 | 4,000 |
| `get_admin` | Instance read | 15,000 | 1,500 |
| `is_depositor` / `is_signer` | Persistent read | 22,000 | 3,000 |
| `get_threshold` | Instance read | 15,000 | 1,500 |
| `get_proposal` | Persistent read | 25,000 | 3,500 |
| `get_approval_count` | Persistent read | 22,000 | 3,000 |
| `has_approved` | Persistent read | 22,000 | 3,000 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `set_pause_signer` / `set_pause_threshold` | Admin config write | 50,000 | 7,500 |
| `approve_pause_proposal` / `execute_pause_proposal` | Business logic write | 60,000–70,000 | 9,000–10,000 |
| `rescue_native` | Business logic write | 90,000 | 14,000 |

## credence_multisig

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `initialize` | Admin config write | 60,000 | 9,000 |
| `add_signer` / `remove_signer` | Admin config write | 50,000 | 7,500 |
| `set_threshold` | Admin config write | 45,000 | 7,000 |
| `submit_proposal` | Business logic write | 80,000 | 12,000 |
| `sign_proposal` | Business logic write | 60,000 | 9,000 |
| `execute_proposal` | Business logic write | 85,000 | 13,000 |
| `reject_proposal` | Business logic write | 55,000 | 8,500 |
| `prune_expired_proposals` | Business logic write | 60,000 + 4k per proposal | 9,000 + 500 per proposal |
| `get_proposal` | Persistent read | 25,000 | 3,500 |
| `is_operation_executed` | Persistent read | 22,000 | 3,000 |
| `get_signature_count` | Persistent read | 22,000 | 3,000 |
| `has_signed` | Persistent read | 22,000 | 3,000 |
| `is_signer` | Persistent read | 22,000 | 3,000 |
| `get_threshold` / `get_signer_count` | Instance read | 15,000 | 1,500 |
| `get_signers` | Persistent read | 30,000 | 4,500 |
| `get_admin` | Instance read | 15,000 | 1,500 |
| `pause` / `unpause` | Business logic write | 70,000 | 10,000 |
| `is_paused` | Instance read | 15,000 | 1,500 |
| `set_pause_signer` / `set_pause_threshold` | Admin config write | 50,000 | 7,500 |
| `approve_pause_proposal` / `execute_pause_proposal` | Business logic write | 60,000–70,000 | 9,000–10,000 |

## timelock

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `initialize` | Admin config write | 50,000 | 7,500 |
| `queue_operation` | Business logic write | 65,000 | 10,000 |
| `execute_operation` | Business logic write | 75,000 | 12,000 |
| `cancel_operation` | Business logic write | 55,000 | 8,500 |
| `get_operation` | Persistent read | 25,000 | 3,500 |
| `is_operation_executed` | Persistent read | 22,000 | 3,000 |
| `get_admin` | Instance read | 15,000 | 1,500 |

## templates

| Entrypoint | Category | Approx CPU | Approx Memory |
|---|---|---|---|
| `initialize` | Admin config write | 45,000 | 7,000 |
| `set_record` | Business logic write | 55,000 | 8,500 |
| `remove_record` | Business logic write | 50,000 | 7,500 |
| `get_record` | Persistent read | 25,000 | 3,500 |
| `has_record` / `is_expired` | Persistent read | 22,000 | 3,000 |
| `get_admin` | Instance read | 15,000 | 1,500 |

---

## Notes

- **Per-call VM overhead** dominates standalone transactions (~300k CPU per first call). The marginal cost of additional calls in the same transaction is much lower. Batch related operations together where possible.
- **Token transfers** (`transfer` / `transfer_from`) add ~150k–200k CPU to any entrypoint that moves tokens. Always budget for at least one cross-contract call when a token transfer occurs.
- **Batch attestation**: `add_attestation_batch` adds ~4k CPU per additional item after the first, due to iteration and event emission overhead.
- **Storage TTL**: Every persistent read includes a TTL bump. Repeated reads of the same key within a transaction cost ~0 CPU after the first (Soroban normalises repeated extends).
- **`mock_all_auths()` mode**: Test-environment numbers are slightly lower than mainnet because auth pre-image verification is skipped. Expect +5–10% in production.

## Reproduction

```bash
# Build all contracts for WASM
cargo build --target wasm32-unknown-unknown --release

# Run tests with budget instrumentation
cargo test --workspace

# For per-contract gas snapshots (example):
cargo test -p credence_arbitration -- gas --nocapture
```
