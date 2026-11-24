# Error Code Wire Stability

Credence smart contract error codes are explicitly wire-stable. External systems, indexers, and off-chain clients depend on the numeric discriminants of `ContractError` variants, so those values must never change silently.

## Policy

- Each `ContractError` variant has a fixed numeric code.
- Variants must not be renumbered after deployment.
- New variants may only be appended at the end of their existing category block.
- Changing the numeric value of an existing variant is a breaking wire-format change.

## Upgrade authorization additions

The upgrade authorization paths use these wire-stable authorization codes. Existing
codes are reused wherever their semantics already match.

| Code | Variant | Condition |
|------|---------|-----------|
| 128 | `UpgradeAuthorizationAlreadyExists` | An authorization record already exists for the address |
| 129 | `UpgradeAuthorizationNotFound` | No authorization record exists for the address |
| 130 | `UpgradeSelfRoleNotAllowed` | An upgrade admin attempts to grant themselves an equal or higher role |
| 131 | `LastUpgradeAdmin` | Revocation would remove the last upgrade admin |
| 132 | `NotPendingUpgradeAdmin` | The caller is not the proposed upgrade admin |

## Bump procedure

1. Add the new variant in `contracts/credence_errors/src/lib.rs` in the correct category section.
2. Assign it the next unused code within that category range.
3. Add a doc comment describing the variant and reaffirming that the code is wire-stable.
4. Update the `ErrorExt` category, description, and recoverability classifications.
5. Add or update tests in `contracts/credence_errors/tests/error_codes_wire.rs`.
6. Review the new variant in `docs/errors.md` to keep the canonical reference in sync.
7. Update the exhaustive category, description, and recoverability mappings in `ErrorExt`.
7. Run `cargo test -p credence_errors error_codes_wire` to verify the discriminant values.

## Testing

The package includes a dedicated wire-stability assertion test in `contracts/credence_errors/tests/error_codes_wire.rs`.

> If the numeric value of an existing `ContractError` variant changes, these assertions must fail.
