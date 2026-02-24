# Weighted attestations

Attestation value depends on the attester's credibility (stake). Weight is derived from attester stake with a configurable multiplier and cap.

## Config

- **set_weight_config(admin, multiplier_bps, max_weight)** — Admin only. `multiplier_bps` is in basis points (e.g. 100 = 1%); weight = stake * multiplier_bps / 10_000, capped at `max_weight` and at protocol MAX_ATTESTATION_WEIGHT.
- **get_weight_config()** — Returns (multiplier_bps, max_weight).

## Attester stake

- **set_attester_stake(admin, attester, amount)** — Admin only. Sets the stake used to compute attestation weight for that attester. Can reflect bond amount or delegated credibility.
- If no stake is set, attestations use default weight 1.

## Weight computation

- When adding an attestation, weight = min(stake * multiplier_bps / 10_000, max_weight, MAX_ATTESTATION_WEIGHT), with a minimum of 1.
- Existing attestations keep their stored weight; when attester stake or config changes, only new attestations use the new weight.

## Security

- Weight is capped to prevent a single high-stake attester from dominating.
- Negative stake is rejected in set_attester_stake.
