## Description

Documents the four snapshot generations used across the workspace — what each pins, when its generation bumps, and how to refresh. Moves tribal knowledge into a discoverable doc for reviewers, new contributors, and the support team.

## Type of Change

- [x] docs — documentation only

## How Has This Been Tested?

- [x] `cargo test --workspace` passes (pre-existing failures unrelated)
- [x] `cargo fmt --all -- --check` passes (pre-existing drift unrelated)
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes (pre-existing failures unrelated)

No code changed — documentation only.

## Checklist

- [ ] Tests added/updated for new or changed functionality
- [x] Docs updated
- [ ] `CHANGELOG.md` updated
- [x] Branch follows `<type>/<short-description>` naming convention
- [ ] Commit messages follow conventional commits

## Additional Context

- **New file:** `docs/SNAPSHOT_GENERATIONS.md`
- **Linked from:** `docs/README.md` and `README.md`
