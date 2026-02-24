# Attestations

Verifiers add credibility attestations to identity bonds. Only authorized attesters can add or revoke attestations; each attestation has a weight, timestamp, and is deduplicated and replay-protected.

## Data structure

- **Attestation** — `id`, `verifier` (attester address), `identity` (subject address), `timestamp`, `weight`, `attestation_data`, `revoked`. Stored by ID; dedup key is (verifier, identity, attestation_data).
- **Subject attestation count** — O(1) count per identity, updated on add/revoke.

## Authorization

- **register_attester(attester)** — Admin only. Registers an authorized verifier.
- **unregister_attester(attester)** — Admin only.
- **is_attester(attester)** — Returns whether the address is an authorized attester.

## Adding attestations

- **add_attestation(attester, subject, attestation_data, nonce)**  
  - Caller must be the attester (require_auth).  
  - Attester must be registered.  
  - Nonce must match current attester nonce (replay prevention); nonce is incremented on success.  
  - Duplicate (same verifier, identity, attestation_data) is rejected.  
  - Weight is computed from attester stake (see weighted attestations).  
  - Emits `attestation_added` with (subject, id, attester, attestation_data, weight).

## Revoking attestations

- **revoke_attestation(attester, attestation_id, nonce)**  
  - Only the original verifier can revoke. Nonce consumed and incremented.  
  - Subject attestation count is decremented; dedup key is removed so the same triple can be attested again.  
  - Emits `attestation_revoked`.

## Queries

- **get_attestation(attestation_id)** — Returns the attestation or panics if not found.
- **get_subject_attestations(subject)** — Returns list of attestation IDs for the identity.
- **get_subject_attestation_count(subject)** — Returns the active attestation count for the identity.

## Security

- Verifier must be authorized and pass require_auth.
- Duplicate attestations (same verifier, identity, data) are prevented.
- Replay is prevented via per-identity nonces; see security.md.
