# Doctest Authoring Guide

This guide explains how to write and maintain `/// # Example` doctests on public
entrypoints in `contracts/credence_bond/src/lib.rs` (and future contracts).

## Why doctests?

Doctests are compiled and executed by `cargo test --doc -p credence_bond`.
They fail CI if an example no longer compiles, which prevents stale docs from
misleading integrators. Renaming an entrypoint without updating its doctest
immediately breaks the build.

## Running doctests

```bash
cargo test --doc -p credence_bond
```

To run all workspace tests (including doctests):

```bash
cargo test --workspace
```

## Two categories of doctests

### 1. Pure Rust helpers — fully runnable

Functions that do not touch `Env` (e.g. `is_valid_bond`, `create_bond`) can
have fully runnable doctests with no annotation:

```rust
/// # Example
///
/// ```
/// use credence_bond::create_bond;
///
/// let bond = create_bond(1000, 0, 3600, false, 0).unwrap();
/// assert_eq!(bond.amount, 1000);
/// ```
pub fn create_bond(...) -> Result<Bond, ContractError> { ... }
```

These run in the standard Rust doctest harness without any special setup.

### 2. Contract methods — `no_run`

Methods on `CredenceBond` require a Soroban `Env` and the generated
`CredenceBondClient`. Use `no_run` so the example is compiled (catching API
drift) but not executed (avoiding harness limitations):

```rust
/// # Example
///
/// ```no_run
/// use credence_bond::{CredenceBond, CredenceBondClient};
/// use soroban_sdk::{Env, Address};
/// use soroban_sdk::testutils::Address as _;
///
/// let e = Env::default();
/// e.mock_all_auths();
/// let contract_id = e.register(CredenceBond, ());
/// let client = CredenceBondClient::new(&e, &contract_id);
/// let admin = Address::generate(&e);
/// client.initialize(&admin);
/// ```
pub fn initialize(e: Env, admin: Address) { ... }
```

`no_run` guarantees the snippet compiles against the real API. If you rename
`initialize` to `init`, the doctest fails to compile and CI catches it.

## Standard boilerplate for contract method doctests

Every contract method doctest should start with this setup block:

```rust
/// ```no_run
/// use credence_bond::{CredenceBond, CredenceBondClient};
/// use soroban_sdk::{Env, Address};
/// use soroban_sdk::testutils::Address as _;
///
/// let e = Env::default();
/// e.mock_all_auths();
/// let contract_id = e.register(CredenceBond, ());
/// let client = CredenceBondClient::new(&e, &contract_id);
/// let admin = Address::generate(&e);
/// client.initialize(&admin);
/// ```
```

Add `use soroban_sdk::testutils::Ledger;` when you need to advance the ledger
timestamp (e.g. to expire a lockup):

```rust
/// let mut info = e.ledger().get();
/// info.timestamp = info.timestamp + 3601;
/// e.ledger().set(info);
```

## Error path examples

Use `should_panic` for examples that demonstrate a panic path:

```rust
/// # Example — panics when bond not found
///
/// ```should_panic
/// use credence_bond::{CredenceBond, CredenceBondClient};
/// use soroban_sdk::{Env, Address};
/// use soroban_sdk::testutils::Address as _;
///
/// let e = Env::default();
/// e.mock_all_auths();
/// let contract_id = e.register(CredenceBond, ());
/// let client = CredenceBondClient::new(&e, &contract_id);
/// let admin = Address::generate(&e);
/// client.initialize(&admin);
/// // No bond created — panics with BondNotFound
/// client.get_identity_state();
/// ```
```

For pure Rust helpers that return `Result`, prefer `assert_eq!(result, Err(...))`:

```rust
/// ```
/// use credence_bond::create_bond;
/// use credence_errors::ContractError;
///
/// assert_eq!(create_bond(0, 0, 3600, false, 0), Err(ContractError::InvalidBondAmount));
/// ```
```

## Cross-referencing markdown docs

Add a `See also:` line pointing to the relevant markdown file so readers can
find the full narrative documentation:

```rust
/// See also: [`docs/early-exit.md`](../../../docs/early-exit.md)
```

Use relative paths from the crate root (`contracts/credence_bond/`).

## Checklist before opening a PR

- [ ] Every `pub fn` on `CredenceBond` has at least one `/// # Example` block.
- [ ] Every pure Rust `pub fn` has a fully runnable (no annotation) doctest.
- [ ] `cargo test --doc -p credence_bond` passes locally.
- [ ] Error paths are covered with `should_panic` or `Err(...)` assertions.
- [ ] `See also:` links point to the correct markdown file.
