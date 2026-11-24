# credence_errors — Error Reference

This crate contains the canonical `ContractError` enum shared by all Credence
smart contracts.

For the full error taxonomy, wire-code layout, recoverability table, and
integration guide see:

- **[`docs/errors.md`](../../../../docs/errors.md)** — workspace-level reference
  (all 110 variants, categories, recoverability, entry-point matrix)
- **[`docs/error-codes-wire.md`](../../../../docs/error-codes-wire.md)** — wire
  stability contract and bump procedure

## Quick variant count

| Category       | Codes    | Count |
|----------------|----------|-------|
| Initialization | 1–99     | 2     |
| Authorization  | 100–199  | 26    |
| Bond           | 200–299  | 36    |
| Attestation    | 300–399  | 5     |
| Registry       | 400–499  | 9     |
| Delegation     | 500–599  | 16    |
| Treasury       | 600–699  | 12    |
| Arithmetic     | 700–799  | 4     |
| **Total**      |          | **110** |

## Single source of truth

`variant_table.rs` — one row per variant, included by both
`src/test_errors.rs` and `tests/discriminant_uniqueness.rs`. When adding a
new variant:

1. Add the variant to `src/lib.rs` with a wire-stable `#[repr(u32)]` code.
2. Add one row to `variant_table.rs`.
3. Add an arm to `is_recoverable()` in `src/lib.rs`.
4. Add an arm to `expected_is_recoverable()` in `src/test_errors.rs`.
5. Add the variant to the `cases` list in `test_is_recoverable_exhaustive`.
6. Run `cargo test -p credence_errors` to confirm all tests pass.
