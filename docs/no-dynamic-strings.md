# No dynamic strings in production contract code

> Closes the threat surfaced in GitHub issue **#713**: a Soroban contract
> must never silently allocate or format a dynamic string in its hot path.
> This document is the canonical reference for the rule; the lint is
> wired in `clippy.toml` and re-enabled per crate via
> `#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]`.

## 1. Threat model

A Soroban WASM contract runs inside the host VM with a fixed resource
budget per contract execution:

* **CPU instructions** and **memory bytes** are metered.
* **WASM size** is metered at deployment time and grows linearly with
  the size of the compiled bytecode.
* **Storage footprint** is pay-as-you-go via Soroban rent.

`format!`, `format_args!`, `write!`, `writeln!` (collectively the
"format macros") attack all three axes simultaneously:

| Axis                | Effect of admitting a `format!`                                   |
|---------------------|-------------------------------------------------------------------|
| WASM size           | Pulls the `core::fmt` + `alloc::fmt` machinery into the binary — measurable kilobyte-level growth per call site. |
| Runtime cost        | Allocates a `String` (heap) and runs the full `fmt::Display`/`fmt::Debug` infrastructure per invocation. |
| Surface area        | Event topics and revert strings become *derived from caller-controlled data* — e.g. `Symbol::new(&e, &format!("claim_{}", i))` — which is exactly the dynamic-symbol anti-pattern auditors flag (cf. `docs/EVENTS.md`). |

The original issue frames it as **defence-in-depth**: the public API
restrictions on `Symbol` already prevent runtime injection, but the gap
"a developer writes `format!` in a contract" remains and should be
closed at compile time, not by convention.

### What an attacker gets if the check is missing

1. **Unbounded event-topic proliferation.** A malicious or careless
   developer writes `Symbol::new(&e, &format!("slash:{}", reason))`
   inside the bond slash path. Each slash emits a *new* on-chain symbol,
   which the indexer must catalogue or mis-classifies; over time this
   bloats the topic registry and breaks downstream replay.
2. **Revert-message oracle.** A revert string built with `format!`
   inlines payload identifiers into the panic message. Off-chain
   indexers that key on revert strings get poisoned.
3. **WASM bloat DoS.** Every call site that introduces `format!` lifts
   the `alloc::fmt` state machine into the contract binary. A handful
   of call sites can push the contract past its WASM size budget, at
   which point the deployment is rejected by the host.

## 2. The rule

> **No call to `format!`, `format_args!`, `write!` or `writeln!` from any
> production contract source file or module.**

Production = anything compiled by `cargo build --release` or
`cargo build --target wasm32-unknown-unknown --release`. The lint is
**deliberately not** enabled during `cargo test` (test modules need
the macros for diagnostic messages) or when building with the
`testutils` feature (the testutils integration path uses them for
stdout diagnostics).

### Surface area

The banned macros cover every path a Soroban crate might resolve them
through, including all prefixed forms (`std::format`, `alloc::format`,
`core::format` / `std::format_args`, etc.) and the unqualified form
`format!`. This guards against a developer writing `format!` after
importing via `use std::format;` or `use alloc::format;` and
incorrectly thinking the lint does not apply.

### Out of scope (deliberately allowed)

* Tests inside `#[cfg(test)]` modules of contract crates.
* Files under `contracts/<crate>/tests/` (integration tests).
* Files under `contracts/<crate>/benches/` (off-chain benchmarks).
* The `crates/credence_admin_cli` CLI binary (off-chain tooling).
* The workspace tests under `tests/` (`THREATS.md` link checks and
  the issue-#713 negative test itself).

These are exempted through the `cfg_attr(not(any(test, feature =
"testutils")), deny(...))` gate at each crate's lib.rs.

## 3. Wiring

### `clippy.toml` (workspace root)

Defines `disallowed-macros` for `format`, `format_args`, `write`,
`writeln` under every relevant path prefix. This file is read by
clippy for the entire workspace. See [`clippy.toml`](../clippy.toml).

### Per-crate `lib.rs`

Every contract crate places, just below its `#![no_std]` line:

```rust
#![cfg_attr(
    not(any(test, feature = "testutils")),
    deny(clippy::disallowed_macros)
)]
```

`clippy::disallowed_macros` is part of the `clippy::restriction`
group; the surrounding `#![allow(clippy::restriction, ...)]` in each
crate is overridden for this specific lint by `deny`, so all other
restriction lints remain silenced and only this one is enforced.

The `not(any(test, feature = "testutils"))` predicate means the
cursor is **on** for:

* `cargo build --release`
* `cargo build --target wasm32-unknown-unknown --release`

and **off** for:

* `cargo test` (test compilation)
* `cargo build --features testutils` (testutils compile paths)

### CI

`.github/workflows/contracts-lints.yml` runs
`cargo clippy --workspace --all-targets -- -D warnings` so a regression
on the rule breaks CI loudly.

## 4. Migrating an accidental dynamic string

If the lint fires on a contract crate's production code, the fix is
deterministic:

| Pattern                                                          | Replace with                                                                                |
|------------------------------------------------------------------|---------------------------------------------------------------------------------------------|
| Event topic with a static literal                                | `Symbol::new(&e, "fixed_name")` — unchanged                                                |
| Event topic whose value varies but with a finite closed set     | Match → `Symbol::new(&e, "fixed_<variant>")` for each variant                              |
| Debug/log message inside a contract entrypoint                  | Replace with `panic_with_error!(e, ContractError::X)` — typed errors are a wire-stable signal |
| Diagnostic string in a unit test (the test helper itself)       | Move the diagnostic out of the contract path (it already is, in `#[cfg(test)]` modules)    |
| A test that legitimately needs many symbol flavours             | Already exempted via `cfg(test)`; nothing to do                                            |

## 5. Cost analysis

The lint runs at compile time, not at runtime. There is **zero hot-path
cost** from the lint itself. Removing a `format!` from a
production call site can save tens of CPU instructions and one heap
allocation per call, but the more durable win is the **WASMs-sized
budget**: every wiped `format!` is ~80–300 bytes of dead `fmt`
machinery that was previously emitted into the deployment artefact.

## 6. Negative test

`tests/threats_link.rs::test_no_dynamic_strings_is_enforced` is the
structural regression test: it asserts the workspace `clippy.toml`
contains the ban and that every contract crate's `lib.rs` contains
the `cfg_attr` deny. If either is removed, the test fails immediately,
making this change impossible to silently back out.
