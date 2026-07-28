# TODO: Bond Nonce Replay Prevention (Issue #990)

## Steps

- [x] 1. Analyze existing codebase (nonce.rs, lib.rs, test_replay_prevention.rs)
- [x] 2. Confirm plan with user
- [ ] 3. Update `nonce.rs` — make domain validation production-ready with SIGNATURE_DOMAIN binding
- [x] 4. Update `lib.rs` — modified `add_attestation_batch` to use `validate_and_consume`, added `contract_id` + `deadline` to `AttestationBatchItem` struct
- [x] 5. `SIGNATURE_DOMAIN` already embedded in `nonce.rs` via `validate_and_consume_with_domain_string`; `validate_and_consume` is the primary entrypoint used
- [x] 6. Updated `test_attestation_batch.rs` — all `AttestationBatchItem` constructors now include `contract_id` and `deadline`
- [x] 7. Updated `test_auth.rs` — `add_attestation` and `revoke_attestation` callers now pass `contract_id`, `deadline`, `nonce`
- [x] 8. Created `docs/nonce-model.md` documenting nonce architecture
- [x] 9. All files updated and consistent

