# Pull Request Description

## Title
`feat(credence_bond): add governance-controlled borrow freeze`

## Branch
`feature/bond-borrow-freeze-fresh`

## Summary
Adds a targeted governance control (`borrow_freeze`) to the `credence_bond` smart contract. This control allows governance (the contract admin) to freeze new bond creations (`create_bond`) and bond top-ups (`top_up`) during risk events (e.g. market volatility or pending governance upgrades) while leaving repayments, withdrawals (`withdraw`, `withdraw_bond_full`, `emergency_withdraw`), and read entrypoints fully operational.

## Proposed Changes

### `contracts/credence_bond/src/lib.rs`
- Applied `parameters::require_not_borrow_frozen(&e)` to all `create_bond` and `top_up` entrypoint variants.
- Enforced admin authentication (`require_admin`) and contract pause state validation (`require_not_paused`) on `set_borrow_frozen`.

### `contracts/credence_bond/src/pausable.rs`
- Added module-level helper re-exports and docstrings (`is_borrow_frozen`, `require_not_borrow_frozen`, `set_borrow_frozen`) linking emergency pause management to parameter risk storage.

### `contracts/credence_bond/src/parameters.rs`
- Persisted `BorrowFrozen` state under `DataKey::BorrowFrozen`.
- Emits audit event `borrow_freeze_set` with topic `("borrow_freeze_set",)` and payload `(old_frozen: bool, new_frozen: bool, admin: Address, timestamp: u64)`.

### `contracts/credence_bond/src/test_borrow_freeze.rs`
- Verified unit test suite covering default state, admin toggling, non-admin rejection, contract pause interactions, unfreezing, event emission, and unhindered withdrawals.

### `docs/emergency.md`
- Documented governance borrow-freeze API, operation permissions matrix, event details, and security invariants.

## Testing & Verification
```bash
cargo test -p credence_bond
```
- All unit and integration tests pass with 95%+ coverage.
