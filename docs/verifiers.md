# Verifiers

Verifiers are authorized attestation providers. This contract supports **stake-based verifier registration**, **reputation tracking**, and **deactivation**.

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

