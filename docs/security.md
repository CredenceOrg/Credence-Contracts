# Security

Security mechanisms for the Credence bond and attestation system.

## Replay attack prevention

- **Nonces** — Each identity has a nonce (starts at 0). State-changing attestation calls require the current nonce and increment it on success.
- **get_nonce(identity)** — Returns the current nonce; the caller must pass this value in the next add_attestation or revoke_attestation call.
- Replayed or out-of-order transactions are rejected with "invalid nonce" because the stored nonce no longer matches.
- Nonce overflow is handled by checked arithmetic (panic if increment would overflow).

## Attestation security

- Only registered attesters can add attestations; attester must pass require_auth.
- Duplicate attestations (same verifier, identity, attestation_data) are rejected.
- Revocation is restricted to the original verifier; nonce is required for revoke.

## Bond and reentrancy

- Reentrancy guard is used in withdraw_bond, slash_bond, and collect_fees; state is updated before any external call (checks-effects-interactions).
- See contract code for lock acquire/release around callbacks.
