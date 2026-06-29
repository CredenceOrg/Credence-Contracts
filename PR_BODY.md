# test(delegation): invariant test for PauseSigner set vs PauseSignerCount

Adds an invariant test ensuring `PauseSignerCount` equals the number of `PauseSigner(Address)` entries set to true.

What changed
- tests: `contracts/credence_delegation/src/test_pause_signer_invariant.rs`
- docs: `docs/pause-signer-invariant.md`
- doc comment: `contracts/credence_delegation/src/pausable.rs` (describes the invariant)
- registered test module in `contracts/credence_delegation/src/lib.rs`

Why
- Prevents drift between per-address `PauseSigner(...)` booleans and the `PauseSignerCount` counter which could undermine pause threshold checks and recovery.

How to validate locally
```bash
# run the new tests
cd contracts/credence_delegation
cargo test -p credence_delegation pause_signer_invariant
```

Notes
- I removed temporary BUG_PATCHES artifacts; they are not included in this branch.

Request
- Please assign reviewers or let me know who should review and I will add suggested reviewers to the PR body.
