## Description

<!-- Briefly describe the changes and the motivation behind them. -->

Closes #(issue-number)

## Type of Change

- [ ] feat — new functionality
- [ ] fix — bug fix
- [ ] docs — documentation only
- [ ] refactor — code restructuring with no behaviour change
- [ ] test — test additions or improvements
- [ ] ci — CI configuration changes
- [ ] chore — maintenance, dependencies, tooling

## How Has This Been Tested?

- [ ] `cargo test --workspace` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] Coverage ≥ 95% for affected crates (`cargo llvm-cov --package <crate> --fail-under-lines 95`)
- [ ] Fuzz harness passes (`cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture`)
- [ ] Error code wire-stability test passes (`cargo test -p credence_errors error_codes_wire`)
- [ ] Release build passes (`cargo build --release`)

## Checklist

- [ ] Tests added/updated for new or changed functionality
- [ ] Docs updated (if public API, storage layout, error codes, or architecture changed)
- [ ] `CHANGELOG.md` updated (if `contracts/**` touched)
- [ ] Branch follows `<type>/<short-description>` naming convention
- [ ] Commit messages follow [conventional commits](https://www.conventionalcommits.org/)

## Additional Context

<!-- Any additional information, screenshots, or relevant links. -->