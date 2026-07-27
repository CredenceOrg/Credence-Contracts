# Storage TTL policy

This document describes the storage TTL strategy used by the Credence Bond contract.

- The contract keeps bond state, attestations, subject-attestation lists, attester stake, and replay-prevention nonce entries alive by bumping their instance-storage TTL on the read/write paths that matter most.
- The configured TTL window is aligned with the maximum supported bond duration of 365 days so a bond locked for the longest legal period does not silently fall out of storage before the owner can unlock it.

## Policy highlights

- Bond state (`DataKey::Bond`) is bumped whenever it is created, fetched, modified, or used by withdrawal/suspension flows.
- Attestation records (`DataKey::Attestation(id)`) and subject-attestation lists (`DataKey::SubjectAttestations`) are bumped whenever they are created or updated.
- Attester stake and weight configuration are bumped when they change so attestation weighting remains available.
- Nonce entries are bumped whenever they are consumed or incremented to keep replay-protection state durable.

## Recovery guidance

If an entry is ever archived unexpectedly, the recovery path is to restore the relevant state from off-chain backups and write it back into contract storage with a fresh TTL. The implementation here focuses on preventing archival during normal lifecycle operations.
