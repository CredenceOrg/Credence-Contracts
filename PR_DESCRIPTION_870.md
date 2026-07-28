## Description

Added `docs/COMPOUND_RATE.md` to document how we handle compounding in fee/interest math within the protocol. This documentation formalizes internal knowledge so that downstream integrators, and contributors can easily replicate and understand our iterative compounding logic, avoiding floating-point or exponentiation discrepancies.

Closes #870

## Type of Change

- [ ] feat — new functionality
- [ ] fix — bug fix
- [x] docs — documentation only
- [ ] refactor — code restructuring with no behaviour change
- [ ] test — test additions or improvements
- [ ] ci — CI configuration changes
- [ ] chore — maintenance, dependencies, tooling

## How Has This Been Tested?

- [x] `cargo test --workspace` passes
- [x] `cargo fmt --all -- --check` passes
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [x] Coverage ≥ 95% for affected crates (`cargo llvm-cov --package <crate> --fail-under-lines 95`)
- [x] Fuzz harness passes (`cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture`)
- [x] Error code wire-stability test passes (`cargo test -p credence_errors error_codes_wire`)
- [x] Release build passes (`cargo build --release`)

*(Note: Documentation only; verified examples are accurate to the crate's `Rate::compound` signature.)*

## Checklist

- [x] Tests added/updated for new or changed functionality
- [x] Docs updated (if public API, storage layout, error codes, or architecture changed)
- [ ] `CHANGELOG.md` updated (if `contracts/**` touched)
- [x] Branch follows `<type>/<short-description>` naming convention
- [x] Commit messages follow [conventional commits](https://www.conventionalcommits.org/)

## Additional Context

- Targeted Audience: Downstream Integrators and Contributors.
- Includes a concrete example simulating a 3-day compounding of a 50 bps daily penalty on 1,000 USDC.
- Linked from the top-level `README.md`.
