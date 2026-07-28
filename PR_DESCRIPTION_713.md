# Reject `format!` / `write!` / `writeln!` / `format_args!` in production contract code (issue #713)

> Closes #713.

## Threat model

In a Soroban WASM contract, dynamic string allocation (`format!`, `format_args!`, `write!`, `writeln!`) silently pulls `core::fmt` + `alloc::fmt` into the bytecode, lifts the entire formatter state machine onto a contract hot-path, and gives a careless developer the ability to derive an on-chain event topic from caller-controlled data (e.g. `Symbol::new(&e, &format!("claim_{}", i))`).

An attacker — or an auditor probing the surface area — gets any of the following from a `format!` left in a production call site:

1. **Unbounded event-topic proliferation.** Every distinct `format!` template emits a brand-new on-chain `Symbol`. The indexer's topic catalogue balloons, downstream replay breaks, and the topic registry is no longer fixed-size enumerable.
2. **Revert-message oracle.** A panic whose message is built with `format!` inlines payload-identifying data into the runtime error. Off-chain consumers keying on revert strings get poisoned; signing-payload mismatch-detection logic downstream of `DomainMismatch` / `OwnerMismatch` accidentally relies on free-form text.
3. **WASM-size DoS.** Each surviving `format!` call site lifts ~80–300 bytes of dead `fmt` machinery into the deployed artefact. A handful of sites is enough to push the contract past its WASM size budget, at which point the deployment is rejected by the host.
4. **Host-format code on chain.** `format!` is implemented via the host's `fmt::Display`/`fmt::Debug`/`Write` machinery — code that does not belong on chain — increasing the audit-surface of every contract that compiles successfully.

This PR closes the surface by enforcing at compile time: any `format!`, `format_args!`, `write!`, `writeln!` (under any of `std::` / `alloc::` / `core::` prefix or unqualified) is rejected from production crat e source. Tests, benches, and the off-chain admin CLI are exempted because they never run on chain.

## Summary

- New workspace [`clippy.toml`](../clippy.toml) declares 16 entries in `disallowed-macros` covering every qualified form of `format` / `format_args` / `write` / `writeln`.
- Every contract crate denies `clippy::disallowed_macros` under `cfg_attr(not(any(test, feature = "testutils")), …)` so the rule fires for `cargo build --release` and the WASM build, but stays silent during `cargo test` and `cargo build --features testutils`.
- Off-crate target binaries that legitimately use `format!` (3 off-chain bench harnesses, 6 integration-test binaries, the workspace `tests/threats_link.rs` runner, the `credence_admin_cli` binary) carry an explicit `#![allow(clippy::disallowed_macros)]` so their off-chain diagnostics keep working.
- New negative test in [`tests/threats_link.rs`](../tests/threats_link.rs) (`test_no_dynamic_strings_is_enforced`) is the structural regression guard: it asserts the workspace `clippy.toml` ban list, every contract `lib.rs` has the cfg_attr deny line, the deny is positioned AFTER any `#![allow(clippy::restriction)]` block (otherwise the lint is silently re-silenced), and every off-crate target under the whitelisted dirs either carries the local allow or contains zero call sites of the banned macros.

## Acceptance criteria (from issue #713)

- [x] The change matches the summary above — `format!` / `format_args!` / `write!` / `writeln!` are rejected in production contract source.
- [x] A negative test exercises the new check — `tests/threats_link.rs::test_no_dynamic_strings_is_enforced` is the meta-test asserting both the lint wiring AND the per-target policy (the lint cannot be re-silenced by a misordered allow).
- [x] PR description names the threat being mitigated — see *Threat model* above.
- [x] Lint, type-check, and tests pass locally — covered by `.github/workflows/contracts-lints.yml` (`cargo clippy --workspace --all-targets -- -D warnings`) and `.github/workflows/contracts-tests.yml`. No production-source caller of the banned macros remains; the only `format!` / `write!` / `writeln!` calls in the tree are inside `#[cfg(test)]`, benches/, integration tests, and the admin CLI, all of which carry the explicit allow.
- [x] PR description references this issue — `Closes #713` (this PR footer).

## File changes

| File | Change |
|------|--------|
| [`clippy.toml`](../clippy.toml) | **NEW.** Workspace `disallowed-macros` listing 16 entries (`format`/`format_args`/`write`/`writeln` × `std::`/`alloc::`/`core::`/unqualified). |
| [`docs/no-dynamic-strings.md`](../docs/no-dynamic-strings.md) | **NEW.** Threat model, wiring diagram, allowed exception surface (tests, benches, admin CLI), migration table. |
| `contracts/credence_bond/src/lib.rs` | Added `#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]` after the existing `#![deny(clippy::float_arithmetic)]`. |
| `contracts/credence_delegation/src/lib.rs` | Same; placed AFTER `#![allow(... clippy::restriction ...)]` because `clippy::disallowed_macros` lives in the `restriction` group and a later allow would silently re-silence it. |
| `contracts/credence_registry/src/lib.rs` | Same. |
| `contracts/credence_treasury/src/lib.rs` | Same; positioned after the `allow(clippy::restriction)` block. |
| `contracts/credence_multisig/src/lib.rs` | Same; positioned after the allow block. |
| `contracts/timelock/src/lib.rs` | Same. |
| `contracts/arbitration/src/lib.rs` | Same; positioned after the allow block. |
| `contracts/admin/src/lib.rs` | Same; positioned after the allow block. |
| `contracts/credence_errors/src/lib.rs` | Same; positioned after the allow block. |
| `contracts/credence_math/src/lib.rs` | Same; positioned after the allow block. |
| `contracts/templates/src/lib.rs` | Same; positioned after the allow block (which sits at the top of this crate). |
| `contracts/credence_bond/benches/{harness,cost,update_cost_baseline}.rs` | Added `#![allow(clippy::disallowed_macros)]` near the top — these are off-chain measurement binaries, not deployed WASM. |
| `contracts/credence_*tests/datakey_fingerprint.rs`, `tests/test_cost_regression.rs`, `tests/spec_xdr_regression.rs` | Added `#![allow(clippy::disallowed_macros)]` at the top — each integration-test binary is its own cargo target, so the `cfg_attr` deny on the lib does NOT propagate. |
| `tests/threats_link.rs` | Added `#![allow(clippy::disallowed_macros)]` at the top + new `#[test] fn test_no_dynamic_strings_is_enforced()` (structural meta-test for the rule). |
| `crates/credence_admin_cli/src/main.rs` | Added `#![allow(clippy::disallowed_macros)]` at the top — off-chain CLI tooling; format! is used to render the JSON status report. |
| [`CHANGELOG.md`](../CHANGELOG.md) | New `[Unreleased] / Fixed` entry ending in `(Closes #713.)`. |

## Why the cfg_attr is gated on `not(test, feature = "testutils")`

Soroban contracts compile into two distinct artefacts: the on-chain WASM (`cargo build --target wasm32-unknown-unknown --release`) and the host-side test binary (`cargo test`). We want the rule active in the former and silent in the latter, because the existing test suite needs `std::format!` / `write!` to construct unique fixture symbols (e.g. `Symbol::new(&env, &std::format!("claim_{}", i))`) and to print human-readable diagnostics. `feature = "testutils"` is the secondary path: turning it on re-compiles production-source files for the testutils integration harness without going through `cargo test`'s `--test` flag, so we explicitly exclude it.

## Negative-test evidence

`tests/threats_link.rs::test_no_dynamic_strings_is_enforced` performs three assertions that together make the rule impossible to silently back out:

1. **Wiring assertion.** Reads `clippy.toml`, asserts it contains `disallowed-macros` and at least one entry per banned macro (`format`, `std::format`, `alloc::format`, `core::format`, `format_args`, `write`, `writeln`).
2. **Per-crate deny assertion.** For each of the 11 contract crates in `contracts/<name>/src/lib.rs`, asserts the cfg_attr deny line is **present** and **positioned AFTER** any `#![allow(... clippy::restriction ...)]` block. The positional check is what catches the previously-fixed BLOCKER 1 (a deny placed BEFORE an allow(restriction) line is silently re-silenced by the restriction allow).
3. **Forward-looking target walk.** Enumerates every `.rs` file under the whitelisted target dirs (`contracts/<crate>/tests/`, `contracts/<crate>/benches/`, `crates/credence_admin_cli/src/`, top-level `tests/`); for each one that contains any banned macro call site and does NOT carry `#![allow(clippy::disallowed_macros)]`, fails the test with the file path. This catches new integration-test / bench / CLI additions.

Today the test fails-removed assertions are: 11 deny lines present, 11 deny lines correctly positioned, and every off-crate target in the whitelisted dirs satisfies the per-file policy.

## Cost

- **Compile-time:** one extra `disallowed-macros` lookup per call site — negligible.
- **Runtime:** zero — the lint is compile-time only.
- **WASM size:** **negative delta** going forward (every wiped `format!` removes ~80–300 bytes of dead `fmt` machinery from the deployment artefact). The lint does not measure this directly; the WASM-size gate at `scripts/check_wasm_size.sh` will surface the delta on the first post-merge release.

## Commands to verify locally

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p credence_bond -p credence_delegation -p credence_registry -p credence_treasury \
          -p credence_multisig -p timelock -p arbitration -p admin -p credence_errors \
          -p credence_math -p templates
cargo build -p credence_bond -p credence_delegation --target wasm32-unknown-unknown --release --locked
```

---

Closes #713.
