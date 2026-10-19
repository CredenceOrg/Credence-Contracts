# Admin operation atomicity

Administrative transactions execute in Soroban's atomic invocation boundary:
if an entrypoint returns an error, its storage writes and contract events are
rolled back together. The admin contract validates all ownership-transfer
preconditions before its first ownership write.

## Ownership invariant

`transfer_ownership` records only a pending proposal. `accept_ownership`
rechecks that the pending candidate is currently an active, unsuspended
`SuperAdmin` after the timelock. A candidate removed, demoted, deactivated, or
suspended during that window cannot receive ownership. The failed acceptance
leaves the owner, pending owner, proposal timestamp, and emitted event set
unchanged, so the current owner can recover by cancelling or replacing the
proposal through the existing transfer flow.

This is compatible with the public interface and storage layout: no migration
is required. It intentionally adds one failure condition at acceptance for a
proposal whose candidate is no longer currently eligible. The security model
assumes Soroban preserves atomicity for a failed invocation and that the
ledger timestamp is the authoritative timelock and suspension clock.
