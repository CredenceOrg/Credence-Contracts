# Delegation Failure Modes

## Revoke replay security semantics

The delegated revoke entry points enforce an explicit validation order:

1. Domain-separated payload verification
2. Nonce consumption
3. State transition (`mark_delegation_revoked`)

This order is intentional.

### Why ordering matters

If the contract were to check the delegation state before consuming the nonce, a replayed revoke payload could fail with `AlreadyRevoked` instead of `InvalidNonce` once the delegation had already been revoked.

For off-chain monitoring and relayer logic, this is important because:

- `InvalidNonce` indicates the payload is a replay or stale request.
- `AlreadyRevoked` indicates the payload was fresh but the delegation state was already revoked.

Keeping nonce checks before state changes preserves this semantic separation and prevents the protocol from leaking revoke-state signals through error ordering.

## Entry points

- `execute_delegated_revoke` — revokes a delegation through a signed relayed payload.
- `execute_delegated_revoke_attest` — revokes an attestation through a signed relayed payload.

Both functions consume the payload nonce before updating the delegation or attestation state.

## Expected semantics

- A replay of the same revoke payload must fail with `InvalidNonce`.
- A second distinct revoke payload with a fresh nonce must fail with `AlreadyRevoked` if the delegation or attestation is already revoked.
- In all cases, payload domain verification occurs before nonce consumption, so payloads signed for the wrong domain are rejected as `InvalidNonce`.

## Error codes

- `InvalidNonce` — nonce has been consumed, is stale, or is out of order.
- `AlreadyRevoked` — the target delegation or attestation has already been revoked.
