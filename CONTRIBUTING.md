# Contributing

## Changelog Discipline

We maintain a strict changelog to help consumers, indexers, and integrators track behavior and wire-format changes.

If your Pull Request modifies any smart contracts (`contracts/**`), you **must** update the `CHANGELOG.md` file. Add a brief, actionable entry describing the change under the `## [Unreleased]` section.
Categorize your change appropriately (e.g., `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).

A CI check is in place to remind you if your PR touches `contracts/**` but misses the `CHANGELOG.md` update.

## Storage Key Naming

When adding or modifying storage keys in contract code, follow the canonical naming convention documented in [docs/STORAGE_KEYS.md](docs/STORAGE_KEYS.md). This ensures consistency across the codebase and helps reviewers verify behavior against documented intent.

## Storage TTL Policy

Every `persistent().set()` call **must** be immediately followed by `persistent().extend_ttl(...)` in the same call frame. Every contract's public entrypoints must call `bump_instance_ttl(&e)` at entry.

See [docs/storage-ttl.md](docs/storage-ttl.md) for the full policy, constants, and the expiry-aware TTL pattern for time-bounded records.
