# Add a require_no_ongoing_dispute_lease(lease) guard

## Summary

Add a typed guard that rejects new arbitration disputes while a creator already has an unresolved dispute in progress. This closes a defense-in-depth re-entry gap in the dispute lifecycle and surfaces a specific contract error instead of allowing duplicate dispute state to accumulate.

## Background

The arbitration contract previously allowed a creator to open another dispute while an earlier dispute for the same creator was still unresolved. That gap creates an avoidable state machine weakness: an attacker or buggy actor can repeatedly queue dispute activity for the same address, increasing the amount of unresolved dispute state that must be reasoned about and making downstream dispute handling less predictable.

## Threat model

### Attack scenario: dispute-state amplification

If this check is missing, a user can keep opening fresh disputes while an older dispute for the same creator remains unresolved. The impact is not a direct token drain, but it does increase the surface area for later abuse and makes the dispute lifecycle easier to exploit for griefing, state amplification, and confusion in downstream tooling.

### Impact

- Additional unresolved dispute state accumulates for a single creator.
- Off-chain indexers and consumers must reason about more ambiguous or overlapping dispute activity.
- The protocol is left more exposed to griefing patterns that rely on repeated dispute creation while existing disputes remain unresolved.

### Mitigation

The new guard checks for an existing active dispute keyed by the creator and returns `ArbitrationError::OngoingDispute` immediately. This preserves a clearly typed failure mode and prevents duplicate unresolved dispute state from being created.

## Changes

- Added a typed guard in the arbitration contract that rejects `create_dispute` when an active dispute already exists for the creator.
- Added a regression test covering the new negative path.
- Documented the security fix in the changelog.

## Tests

- Added regression coverage for the new rejection path in the arbitration lifecycle tests.

## Verification

```bash
cargo test -p arbitration
cargo build --target wasm32-unknown-unknown --release
cargo clippy --workspace --all-targets -- -D warnings
```

## Notes

- No new storage layout was introduced; the change reuses the existing active-dispute tracking state.
- The added check is constant-time in the storage lookup path and does not add meaningful runtime cost.

Closes #850
