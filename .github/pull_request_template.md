## Summary

<!-- What changed and why? Link the issue. -->

## Type of change

- [ ] Bug fix
- [ ] Feature
- [ ] Security hardening
- [ ] Refactor
- [ ] Documentation/templates
- [ ] CI/tooling

## Verification

<!-- Paste the exact commands run and summarize the output. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo build --all-targets`
- [ ] `cargo test --all-targets` or `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] Coverage checked or not applicable
- [ ] Fuzz/property test run or not applicable
- [ ] Security scan impact reviewed or not applicable

## Contract safety checklist

- [ ] Storage layout changes are documented or not applicable
- [ ] Event/indexer changes are documented or not applicable
- [ ] `ContractError` numeric discriminants remain wire-stable or `docs/error-codes-wire.md` updates explain the new code
- [ ] Funds-flow, authorization, and replay assumptions were reviewed
- [ ] No secrets, private keys, seed phrases, or live credentials are included

## Documentation checklist

- [ ] README or docs updated for public behavior/API changes
- [ ] Tests or examples updated where useful
- [ ] Changelog/release note added or explicitly not needed

## Reviewer notes

<!-- Anything reviewers should focus on, known unrelated CI blockers, or follow-up work. -->
