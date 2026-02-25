# Fuzz Testing (Bond Operations)

This repository includes **fuzz-style, property/invariant tests** for the `credence_bond` contract to help uncover edge cases and potential vulnerabilities in core bond flows:

- `create_bond`
- withdrawals (`withdraw_bond`, `withdraw_early`)
- slashing (`slash`, `slash_bond`)

Unlike `cargo-fuzz`/libFuzzer targets, these fuzz tests run under `cargo test` and are **deterministic** (seeded), so they can be executed reliably in CI and reproduced locally.

## Where the tests live

- `contracts/credence_bond/src/fuzz/test_bond_fuzz.rs`

## Running

Smoke/default run:

```bash
cargo test -p credence_bond fuzz::test_bond_fuzz
```

Extended session (recommended locally):

```bash
BOND_FUZZ_EXTENDED=1 BOND_FUZZ_SILENCE_PANICS=1 cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture
```

Reproduce a specific run by pinning the seed and iteration count:

```bash
BOND_FUZZ_SEED=0x00c0dece BOND_FUZZ_ITERS=5000 BOND_FUZZ_SILENCE_PANICS=1 cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture
```

## Configuration

The fuzz tests are configured via environment variables:

- `BOND_FUZZ_SEED` (u64): deterministic RNG seed (supports `0x` hex).
- `BOND_FUZZ_ITERS` (usize): number of iterations.
- `BOND_FUZZ_ACTIONS` (usize): number of post-creation operations per successful `create_bond`.
- `BOND_FUZZ_EXTENDED`: if set and `BOND_FUZZ_ITERS` is not set, uses a higher default iteration count.
- `BOND_FUZZ_SILENCE_PANICS`: if set, installs a no-op panic hook during the fuzz run (useful with `--nocapture`).

## Invariants checked

On successful operations, the fuzz harness asserts:

- `bonded_amount >= 0`
- `slashed_amount >= 0`
- `slashed_amount <= bonded_amount`
- `bond_start + bond_duration` does not overflow
- Token balance conservation for:
  - `create_bond` (identity → bond contract)
  - `withdraw_bond` (bond contract → identity)
  - `withdraw_early` (bond contract → identity + treasury split equals requested amount)

## Findings / issues discovered

During implementation of this fuzz harness, the following issues were identified and **hardened**:

- **Missing owner authorization on bond operations**: added `require_auth()` checks so only the bond owner can create, top up, request withdrawal, extend duration, or withdraw.
- **Negative amount handling**: added explicit validation to reject negative amounts for withdrawals, top-ups, and slashing entrypoints.
- **Slashing overflow safety**: ensured the callback-oriented `slash_bond` entrypoint uses checked arithmetic for `slashed_amount + slash_amount`.

If you run extended sessions and discover additional issues, document them here with:

- seed / iteration / operation sequence (minimal reproduction)
- observed panic message or invariant violation
- expected behavior
