# CROSS_CONTRACT.md

## Summary
When to call `try_*` vs the panicking variant; how to surface partial failure.

## Background
We rely on this information internally but it is currently tribal knowledge. Writing it down lets reviewers verify behaviour against the documented intent, lets new contributors get productive without reading every commit, and lets the support team answer common questions without paging an engineer.

## Acceptance Criteria
- The change matches the summary above.
- The new document is linked from at least one existing top-level doc (README or docs/README).
- Examples in the document compile / run if applicable.
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation Hints
**Audience:** Contributors

Prefer concrete examples over abstract definitions. Show a real entrypoint, request, or output rather than `foo()` placeholders.

Cross-link from the README and from any related doc; orphaned docs rot fastest.

Where the project ships generated docs (rustdoc, JSDoc, OpenAPI), keep this new content discoverable from the same starting point.

**Repo-specific notes:**
- This is a Soroban contract crate. Run `cargo build --target wasm32-unknown-unknown --release` to verify the change still builds for WASM.
- Run `cargo test -p` for the affected crate, and `cargo clippy --workspace --all-targets -- -D warnings` before pushing.
- Keep `#![no_std]` discipline: do not introduce `std::` calls; use `soroban_sdk` primitives.

## Examples
```rust
// Using `try_*`
let result = my_function().try_unwrap()?;

// Using panicking variant
let result = my_function().unwrap_or_else(|e| panic!("Error: {}", e));
```

## Cross-links
- [README](README.md#cross-contract-guide)
- [Related Doc](related_doc.md)