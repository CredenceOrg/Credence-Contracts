# CI

## Checks

`.github/workflows/ci.yml` runs the contract lints and tests, plus the
bare-`panic!` ratchet:

```sh
python3 scripts/check_no_panic.py
```

### `scripts/check_no_panic.py`

Fails the build if a bare `panic!(...)` macro call appears in contract
release code (compiled, non-`#[cfg(test)]` source). Soroban contracts
should use `panic_with_error!(env, Error::Variant)` instead — see
`contracts/credence_errors/src/lib.rs` for the error catalogue.

Pre-existing debt is grandfathered in `scripts/panic_baseline.txt`, whose
entries are `<path>::<panic!(...) snippet>TAB<occurrence count>`. The count
is the number of times that exact message may appear in that file, so two
identical `panic!("not admin")` calls in one file are allowed but a third
fails the check. Counts may shrink freely (fixing panics is always
welcome); they may never grow.

- Regenerate after an intentional, reviewed change:
  `python3 scripts/check_no_panic.py --update-baseline`
- Unit tests: `python3 -m unittest discover -s scripts/tests -v`
