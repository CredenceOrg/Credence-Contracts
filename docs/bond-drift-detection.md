# Bond Drift Detection Runbook

Issue **#436** adds on-chain **post-write self-checks** so bond accounting drift is detected immediately after every bond-module storage mutation, rather than only in off-chain tests.

## What is checked

| Check | Condition | Error | Event kind |
|-------|-----------|-------|------------|
| Slashed vs bonded | `slashed_amount <= bonded_amount` and both non-negative | `ContractError::InvariantViolation` (218) | `SlashedExceedsBonded` |
| Attestation counter | If `SubjectAttestationCount(s)` exists, it equals `len(SubjectAttestations(s))` | Same | `AttestationCountMismatch` |

Implementation: [`contracts/credence_bond/src/invariants.rs`](../contracts/credence_bond/src/invariants.rs).

## When it runs

`invariants::assert_self_consistent(&env)` is invoked at the end of bond write entrypoints, including:

- `CredenceBond::create_bond`, `top_up`, `extend_duration`, `withdraw`, `withdraw_early`, `request_withdrawal`, `renew_if_rolling`, `withdraw_bond`, `slash_bond`
- `slashing::slash_bond`, `slashing::unslash_bond`
- `batch::create_batch_bonds`
- Attestation paths use `assert_self_consistent_for_subject` after `add_attestation` / `revoke_attestation`

Reads and governance-only writes (e.g. `get_identity_state`, fee collection that does not touch `DataKey::Bond`) do **not** run the check.

## Failure behaviour

1. Contract emits **`bond_drift_detected`** (structured topics + data; see [`events.rs`](../contracts/credence_bond/src/events.rs)).
2. Contract panics with **`ContractError::InvariantViolation`** (`218`, bond category 200–299).
3. Transaction fails; storage changes in that invocation roll back.

Indexers should alert on `bond_drift_detected` even though the transaction aborts—the event is published before the panic.

### Event shape

| Field | Location | Type |
|-------|----------|------|
| Event name | Topic[0] | `Symbol` `"bond_drift_detected"` |
| Subject | Topic[1] | `Address` |
| Kind | Data[0] | `BondDriftKind` |
| Bonded | Data[1] | `i128` |
| Slashed | Data[2] | `i128` |
| Count | Data[3] | `u32` |
| List length | Data[4] | `u32` |

## Performance / cost notes

Documented on `assert_self_consistent` in code:

- **Bond-only writes**: ~2–4 extra instance storage reads.
- **Attestation writes**: ~3–5 reads plus O(n) list length for the subject (n = attestation IDs stored).
- Intentional trade-off: small per-tx overhead vs catching counter/balance drift before withdrawals or slashes proceed on corrupt state.

Do **not** call `assert_self_consistent` on read-only entrypoints.

## Operations playbook

### Alert: `bond_drift_detected` in logs

1. **Capture** transaction hash, contract ID, and full event payload (`kind`, bonded, slashed, count, list_len).
2. **Classify**
   - `SlashedExceedsBonded` → bond principal vs slash accumulator diverged (often bug or malicious storage; not a normal user error).
   - `AttestationCountMismatch` → counter vs list desync (check recent attestation add/revoke deployments).
3. **Halt** treat as **severity-1**: pause new deposits/top-ups via governance pause if available; do not assume bond balances are trustworthy until root-caused.
4. **Compare** on-chain `get_identity_state` vs raw storage keys `Bond`, `SubjectAttestationCount`, `SubjectAttestations` for the subject in the event.
5. **Root cause** recent upgrade, partial migration, or custom indexer/script writing storage (only contract writes should mutate these keys).

### Expected user-facing error

Wallets and SDKs decoding errors should map code **218** to a message such as: *internal bond invariant failed; contact support*. This is not an end-user recoverable error.

### Regression testing

```bash
cargo test -p credence_bond drift
```

Covers:

- Hand-injected drift via direct storage manipulation inside `env.as_contract`
- Panic with `HostError` wrapping `InvariantViolation`
- `bond_drift_detected` emission for slash drift and attestation count drift

## Relation to test-only invariants

[`test_invariants.rs`](../contracts/credence_bond/src/test_invariants.rs) (I1–I7) remains the **test harness** catalogue. Drift detection implements the critical subset (I2 + I7) **on-chain** after writes. Tests should continue calling `assert_all_invariants` off-chain; production uses `assert_self_consistent`.

## Wire stability

- `ContractError::InvariantViolation = 218` — do not renumber after deployment.
- Event name `bond_drift_detected` — treat as stable for indexers.

## Change checklist (developers)

- [ ] Any new bond or attestation **write** path calls `assert_self_consistent` or `assert_self_consistent_for_subject`.
- [ ] Keep `SubjectAttestationCount` in sync when mutating `SubjectAttestations`.
- [ ] Extend `cargo test -p credence_bond drift` if new drift classes are added.
- [ ] Update this runbook when event schema or error code changes.
