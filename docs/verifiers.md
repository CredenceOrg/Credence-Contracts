# Verifiers

Verifiers are authorized attestation providers. This contract supports **stake-based verifier registration**, **reputation tracking**, and **deactivation**.

## Delegation Signature-Scheme Dispatch

`credence_delegation` ships a multi-scheme signature-verifier registry that allows
Secp256r1 (NIST P-256) and MLDSA44 (post-quantum ML-DSA) delegated signatures to
be validated by operator-registered verifier contracts.

### Scheme Tags (wire-stable)

| Tag | Value | Description |
|-----|-------|-------------|
| Ed25519  | 0 | Default; verified by Soroban's built-in auth engine |
| Secp256r1 | 1 | ECDSA over NIST P-256 |
| MLDSA44  | 2 | ML-DSA post-quantum scheme |

Values are **wire-stable** and must never be renumbered after deployment.

### Registration → Storage → Dispatch → Error mapping

```
Admin calls register_verifier(scheme, verifier_address)
  └─► validates scheme is known (0–2)
  └─► stores DataKey::Verifier(scheme) → verifier_address  (instance storage)
  └─► emits verifier_registered event

execute_delegated_delegate / execute_delegated_revoke / execute_delegated_revoke_attest
  └─► owner.require_auth()  ← Soroban auth validates Ed25519 payload
  └─► domain::verify_payload(...)  ← domain-separation check
  └─► scheme = decode_scheme_safe(&payload)  ← defaults to Ed25519 for legacy payloads
  └─► verifier::verify_delegated_signature(e, owner, message, sig, scheme)
        Ed25519 (0) ─► no-op (Soroban auth already covered it)
        Secp256r1/MLDSA44:
          1. look up DataKey::Verifier(scheme) in instance storage
             ✗ missing ─► panic VerifierNotRegistered (506)
          2. invoke_contract(verifier_addr, "verify", [owner, message, signature])
             returns false ─► panic VerificationFailed (507)
             returns true  ─► continue
  └─► nonce::consume_nonce(...)  ← replay prevention
```

### Error codes

| Error | Code | Cause |
|-------|------|-------|
| `UnknownScheme` | 504 | scheme field > 2 |
| `VerifierNotRegistered` | 506 | scheme known but no entry in `DataKey::Verifier(scheme)` |
| `VerificationFailed` | 507 | verifier contract returned `false` |

### Verifier contract interface

A registered verifier must expose a single entry point:

```rust
fn verify(owner: Address, message: Bytes, signature: Bytes) -> bool
```

Return `true` for a valid signature, `false` for an invalid one.
Panicking inside the verifier is also treated as `VerificationFailed`.

### Re-registration

Calling `register_verifier` for a scheme that already has a registered verifier
**overwrites** the mapping. This enables key rotation and scheme upgrades without
a contract upgrade. The old verifier is not notified.

---

## Bond-contract verifier registry (stake-based)

## Overview

- A verifier becomes active by staking the configured token (typically USDC).
- Active verifiers can add credibility attestations via `add_attestation`.
- Verifiers can be deactivated (self or admin) which immediately disables attestation rights.
- Staked funds can be withdrawn only after deactivation.

## Configuration

Before stake-based registration, the bond contract must have a token set:

- `set_token(admin, token)` — Admin-only. Sets the token used by bonds and verifier stake.

Stake-based registration is controlled by a minimum stake:

- `set_verifier_stake_requirement(admin, min_stake)` — Admin-only. Sets the minimum stake required to activate as a verifier.
- `get_verifier_stake_requirement()` — Returns the configured minimum stake (defaults to 0).

## Registration

To register as a verifier, an address must:

1. Hold the configured token.
2. Approve the bond contract to spend tokens on its behalf (`approve` on the token contract).
3. Call:
   - `register_verifier(verifier, stake_deposit)`

Notes:

- New registration and reactivation both enforce `total_stake >= min_stake`.
- Calling `register_verifier` while already active requires a **positive** `stake_deposit` and is treated as a stake top-up.
- The staked amount is locked in the bond contract address until withdrawn after deactivation.

## Deactivation

Deactivation disables a verifier immediately:

- `deactivate_verifier(verifier)` — Self-deactivation.
- `deactivate_verifier_by_admin(admin, verifier)` — Admin-only deactivation.

After deactivation:

- `require_verifier` checks fail, preventing new attestations.
- Existing attestations remain in storage; deactivation does not retroactively revoke them.

## Stake withdrawal

After deactivation, a verifier may withdraw stake:

- `withdraw_verifier_stake(verifier, amount)`

Constraints:

- Verifier must be inactive.
- `amount` must be positive and `<= stake`.

## Reputation

Reputation is tracked per verifier in `VerifierInfo`:

- `get_verifier_info(verifier)` returns:
  - `stake`
  - `reputation`
  - `active`
  - `registered_at` / `deactivated_at`
  - `attestations_issued` / `attestations_revoked`

This implementation updates reputation automatically:

- On `add_attestation`, reputation increases by the attestation `weight`.
- On `revoke_attestation`, reputation decreases by the attestation `weight`.

Admin override (optional):

- `set_verifier_reputation(admin, verifier, new_reputation)` — Admin-only.

## Events

Verifier-related events are emitted for off-chain indexing:

- `verifier_config_updated(min_stake)`
- `verifier_registered(verifier)` — data `(kind, stake_deposited, total_stake, min_stake)`
- `verifier_reactivated(verifier)` — data `(kind, stake_deposited, total_stake, min_stake)`
- `verifier_stake_deposited(verifier)` — data `(kind, stake_deposited, total_stake, min_stake)`
- `verifier_deactivated(verifier)` — data `(reason, timestamp, stake)`
- `verifier_stake_withdrawn(verifier)` — data `(amount, remaining_stake)`
- `verifier_reputation_updated(verifier)` — data `(delta, new_reputation, issued, revoked, reason)`

## Security considerations

- **Authorization**: Only active verifiers can add attestations; deactivation clears the verifier role used by `require_verifier`.
- **Stake custody**: Stake is held at the bond contract address. Withdrawal is only allowed when inactive.
- **Checks-effects-interactions**: Stake deposit/withdraw follow CEI and contract entrypoints are wrapped with a reentrancy guard.
- **Token approvals**: Stake deposit uses `transfer_from` where the spender is the bond contract; verifiers must explicitly approve allowances.

Limitations / non-goals (current implementation):

- No stake slashing mechanism is included (only lock + withdraw).
- Reputation is activity-weighted; it does not encode "truthfulness" beyond on-chain actions.

