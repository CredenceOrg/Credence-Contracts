# Weighted attestations

Attestation value depends on the attester's credibility (stake). Weight is derived from attester stake with a configurable multiplier and cap.

## Config

- **set_weight_config(admin, multiplier_bps, max_weight)** — Admin only.
  - `multiplier_bps` is in basis points (e.g. 100 = 1%).
  - `multiplier_bps` is bounded to 10_000 (100%) to prevent runaway multiplier amplification.
  - `max_weight` is bounded to `MAX_ATTESTATION_WEIGHT`.
  - Emits `weight_config_set(old_multiplier_bps, old_max_weight, multiplier_bps, max_weight)`.
- **get_weight_config()** — Returns (multiplier_bps, max_weight).

## Attester stake

- **set_attester_stake(admin, attester, amount)** — Admin only. Sets the stake used to compute attestation weight for that attester. Can reflect bond amount or delegated credibility.
- **register_verifier(verifier, stake_deposit)** — When using stake-based verifier registration, the verifier's staked amount is mirrored into attester stake so weights reflect real locked stake.
- If no stake is set, attestations use default weight 1.

## Weight computation

- When adding an attestation, weight is computed with integer floor division:
  `floor(stake * min(multiplier_bps, 10_000) / 10_000)`.
- The calculation is split into quotient/remainder terms before multiplication, so max-range stake inputs clamp instead of overflowing before the cap is applied.
- Computed weight is clamped to the stored `max_weight` and `MAX_ATTESTATION_WEIGHT`.
- Stored attestations must have positive weight, so any zero raw result (including zero stake, zero multiplier, or `max_weight == 0`) is lifted to the default weight 1.
- Existing attestations keep their stored weight; when attester stake or config changes, only new attestations use the new weight.

## Security

- Weight is capped to prevent a single high-stake attester from dominating.
- Negative stake is rejected in set_attester_stake.
