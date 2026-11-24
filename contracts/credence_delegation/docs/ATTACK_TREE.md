# Attack Tree — `credence_delegation`

**Audience**: Contributors and security auditors who want to verify that the
delegation contract's implementation matches its documented security intent.

**STRIDE categories used below**:

| Letter | Category | One-line definition |
|--------|----------|---------------------|
| S | Spoofing | Acting as an identity you don't control |
| T | Tampering | Modifying data without authorisation |
| R | Repudiation | Denying an action you performed |
| I | Information disclosure | Exposing data that should be opaque |
| D | Denial of service | Rendering the contract unavailable or degraded |
| E | Elevation of privilege | Gaining capabilities you were not granted |

Cross-references:
- [docs/delegation.md](../../../docs/delegation.md) — delegation types, expiry, and revocation design
- [docs/DELEGATION_HANDBOOK.md](../../../docs/DELEGATION_HANDBOOK.md) — operational handbook
- [docs/auth-tree-threats.md](../../../docs/auth-tree-threats.md) — Soroban auth-tree specifics
- [docs/THREAT_MODEL.md](../../../docs/THREAT_MODEL.md) — workspace-level STRIDE overview
- [docs/security.md](../../../docs/security.md) — overflow, replay, and reentrancy mechanisms
- [docs/delegation-failure-modes.md](../../../docs/delegation-failure-modes.md) — error taxonomy
- [contracts/credence_delegation/docs/nonce-replay-proof.md](nonce-replay-proof.md) — formal replay-prevention argument

---

## 1. Initialisation

### Attack: double-initialise to replace admin

**STRIDE**: E  
**Entrypoint**: `initialize(admin)`

```
GOAL: replace the deployed admin with an attacker-controlled address
  ├── Call initialize() a second time
  │   └── BLOCKED: credence_errors::require_contract_uninitialized panics
  │         when DataKey::Admin already exists in instance storage
  └── Race the first call (deploy-gap attack)
      └── BLOCKED: Stellar transactions are atomic; no ledger gap between
            contract creation and initialize in a single transaction
```

---

## 2. Direct delegation path (`delegate`)

### Attack: delegate on behalf of another owner

**STRIDE**: S  
**Entrypoint**: `delegate(owner, delegate, delegation_type, expires_at, nonce)`

```
GOAL: create a delegation that owner never authorised
  └── Call delegate(victim_address, attacker, ...)
      └── BLOCKED: owner.require_auth() — Soroban host rejects the call
            unless the victim's signature is present in the transaction auth tree
```

---

### Attack: create an already-expired or indefinite delegation

**STRIDE**: T  
**Entrypoint**: `delegate`

```
GOAL: create a delegation that is already expired (DoS / confusion) OR
      one that never expires (persistent backdoor)
  ├── Pass expires_at <= now
  │   └── BLOCKED: validate_delegation_expiry panics with ExpiryInPast
  ├── Pass expires_at = u64::MAX
  │   └── BLOCKED: expires_at > now + MAX_DELEGATION_DURATION (≈365 days)
  │         panics with DelegationExpiryTooLong
  └── Pass expires_at = now (exact equality)
      └── BLOCKED: expires_at <= now check is strict (≤, not <)
```

**`MAX_DELEGATION_DURATION`** = 365 × 24 × 60 × 60 = 31,536,000 seconds.

---

### Attack: replay the `delegate` call with the same nonce

**STRIDE**: R  
**Entrypoint**: `delegate`

```
GOAL: replay a past signed delegate transaction
  └── Re-submit with nonce N after it was already consumed
      └── BLOCKED: nonce::consume_nonce increments stored nonce on success;
            re-submission uses stale nonce → panics with InvalidNonce
```

---

## 3. Relayer path (`execute_delegated_*`)

### Attack: replay a delegated action in a different function

**STRIDE**: R  
**Entrypoints**: `execute_delegated_delegate`, `execute_delegated_revoke`,
`execute_delegated_revoke_attest`

```
GOAL: take a payload signed for one action and replay it against another
  ├── Use a "delegate" payload against execute_delegated_revoke
  │   └── BLOCKED: domain::verify_payload checks DomainTag; tag mismatch panics
  └── Use a "revoke_delegation" payload against execute_delegated_revoke_attest
      └── BLOCKED: same domain-tag mismatch check
```

Domain tags: `DomainTag::Delegate` / `DomainTag::RevokeDelegation` / `DomainTag::RevokeAttestation`.

---

### Attack: replay a payload across different contract deployments

**STRIDE**: R  
**Entrypoints**: `execute_delegated_*`

```
GOAL: take a payload signed for contract deployment A and replay it on deployment B
  └── Submit the same payload to a freshly deployed CredenceDelegation
      └── BLOCKED: domain::verify_payload checks payload.contract_id against
            e.current_contract_address() — mismatch panics
```

---

### Attack: submit a stale / expired signed payload

**STRIDE**: R  
**Entrypoints**: `execute_delegated_*`

```
GOAL: use a payload that was signed far in the past to make a delayed change
  └── Submit a payload with signed_at ledger far behind current ledger
      └── BLOCKED: domain::check_payload_age rejects payloads older than
            MAX_PAYLOAD_AGE_LEDGERS
            NOTE: checked AFTER verify_payload but BEFORE nonce consumption
            so a stale payload does not burn a nonce slot
```

---

### Attack: forge a delegated payload without owner's private key

**STRIDE**: S  
**Entrypoints**: `execute_delegated_*`

```
GOAL: construct a valid DelegatedActionPayload without controlling owner's key
  ├── Craft a payload and call execute_delegated_delegate(owner, ...)
  │   └── BLOCKED: owner.require_auth() — Soroban host verifies owner signed
  └── Use a different signature scheme to bypass Ed25519 check
      └── BLOCKED: scheme dispatch calls verifier::verify_delegated_signature
            which routes to the registered verifier for Secp256r1/MLDSA44;
            an unregistered scheme panics with UnknownScheme
```

---

### Attack: escalate privilege by substituting `target` in a relayed payload

**STRIDE**: T  
**Entrypoint**: `execute_delegated_delegate`

```
GOAL: create a delegation for a different (stronger) delegate than was signed
  └── Call execute_delegated_delegate(owner, evil_delegate, ..., payload_for_other_delegate)
      └── BLOCKED: domain::verify_payload binds the payload to the owner+target pair;
            substituting delegate causes hash mismatch → panics
```

---

## 4. Revocation

### Attack: revoke another owner's delegation

**STRIDE**: S  
**Entrypoints**: `revoke_delegation`, `execute_delegated_revoke`

```
GOAL: silently revoke a delegation that an attacker does not own
  └── Call revoke_delegation(victim, victim_delegate, ...)
      └── BLOCKED: owner.require_auth() — victim's signature is required
```

---

### Attack: revoke an already-expired delegation to prevent clean-up

**STRIDE**: D  
**Entrypoint**: `revoke_delegation`

```
GOAL: consume victim's nonce by revoking an already-expired delegation
  └── Owner tries to revoke after expires_at + revocation_grace_period
      └── PARTIALLY MITIGATED: revocation after the grace window is rejected;
            cleanup_expired is the correct path once outside the grace window
```

---

### Attack: skip nonce consumption by revoking via cleanup_expired

**STRIDE**: T  
**Entrypoint**: `cleanup_expired`

```
GOAL: remove a delegation without consuming a nonce (allowing nonce reuse)
  └── Call cleanup_expired(owner, delegate, delegation_type) on an expired delegation
      NOTE: cleanup_expired is INTENTIONALLY permissionless and does NOT consume
      a nonce — it only removes expired entries from storage. Because the
      delegation is already expired (expires_at < now) it has no authority;
      no auth-bypass occurs. The nonce sequence is independent of cleanup.
```

---

### Attack: selectively block cleanup to inflate on-chain storage costs

**STRIDE**: D  
**Entrypoint**: `cleanup_expired`

```
GOAL: prevent cleanup of expired entries to waste storage rent budget
  └── Attacker does nothing — cleanup_expired is permissionless, so anyone
      can trigger it on any expired entry.
      MITIGATED: any third party can call cleanup_expired to reclaim rent;
      storage cost is borne by the owner's storage TTL anyway
```

---

## 5. Nonce management

### Attack: invalidate_nonce_range to block future delegations

**STRIDE**: D  
**Entrypoint**: `invalidate_nonce_range(identity, new_nonce)`

```
GOAL: advance victim's nonce far enough that their pre-signed payloads are unusable
  └── Call invalidate_nonce_range(victim, high_nonce) as an attacker
      └── BLOCKED: invalidate_nonce_range requires identity.require_auth();
            victim must sign the call
```

---

### Attack: advance own nonce by more than MAX_NONCE_INVALIDATION_SPAN at once

**STRIDE**: D  
**Entrypoint**: `invalidate_nonce_range`

```
GOAL: DoS own delegation history by jumping nonce by u64::MAX in one call
  └── Call invalidate_nonce_range(self, current_nonce + 100_001)
      └── BLOCKED: new_nonce - stored_nonce > MAX_NONCE_INVALIDATION_SPAN (10 000)
            panics; must be done in multiple bounded calls
```

**`MAX_NONCE_INVALIDATION_SPAN`** = 10,000 — caps a single jump.

---

### Attack: overflow the nonce counter

**STRIDE**: T  
**Entrypoint**: Any nonce-consuming entrypoint

```
GOAL: wrap the nonce counter from u64::MAX back to 0 to replay old payloads
  └── Submit 2^64 nonce-consuming calls
      └── BLOCKED: nonce::consume_nonce uses checked_add; Overflow panic before
            increment; gas cost of 2^64 transactions is infeasible
```

---

## 6. Verifier registration

### Attack: register a malicious verifier contract

**STRIDE**: E  
**Entrypoint**: `register_verifier(scheme, verifier_id)`

```
GOAL: inject a verifier that always returns "valid" for any signature
  └── Call register_verifier(scheme, malicious_contract)
      └── BLOCKED: admin.require_auth() + stored admin check required;
            only the contract admin can register verifiers
```

---

### Attack: register a verifier for an unsupported scheme tag

**STRIDE**: T  
**Entrypoint**: `register_verifier`

```
GOAL: flood the verifier registry with unknown scheme entries to confuse dispatch
  └── Call register_verifier(9999, ...)
      └── BLOCKED: verifier::validate_scheme_registered panics for unknown tags;
            only Ed25519=0, Secp256r1=1, MLDSA44=2 are accepted
```

---

## 7. Grace period manipulation

### Attack: set revocation_grace_period = 0 to invert InGrace semantics

**STRIDE**: T  
**Entrypoint**: `set_revocation_grace_period(admin, period)`

```
GOAL: change post-expiry revocation window to enable or disable late revocations
  └── Call set_revocation_grace_period(attacker, 0)
      └── BLOCKED: admin.require_auth() required
```

---

### Attack: exploit InGrace status as a delegate authority window

**STRIDE**: E  
**Entrypoint**: Any operation checking delegation validity

```
GOAL: act as delegate while delegation status is InGrace
  └── Call is_valid_delegate / check_delegation_active after expires_at
      └── BLOCKED: is_valid = !d.revoked && d.expires_at > now
            Authority check is a HARD CLIFF at expires_at.
            InGrace is informational only — it does NOT re-grant authority.
```

See also: [docs/delegation.md](../../../docs/delegation.md) § Lifecycle status.

---

## 8. Pause / circuit-breaker

### Attack: pause contract without meeting threshold

**STRIDE**: D  
**Entrypoints**: `pause`, `execute_pause_proposal`

```
GOAL: halt all delegation operations with a single rogue pause-signer
  └── Call pause(rogue_signer)
      └── BLOCKED: pausable module enforces PauseThreshold approvals from
            registered PauseSigners before execution
```

---

### Attack: keep contract paused indefinitely

**STRIDE**: D  
**Entrypoints**: `pause`, `unpause`

```
GOAL: freeze delegations by blocking the unpause quorum
  └── Refuse to sign unpause proposals (key rotation / off-chain coordination)
      └── MITIGATED: admin can update the pause-signer set via set_pause_signer;
            rotate compromised or unresponsive signers out to restore the quorum
```

---

## 9. Cross-contract (Soroban auth-tree) threats

### Attack: strip a leaf from the auth tree

**STRIDE**: T

```
GOAL: submit a transaction that omits one required auth entry so an
      otherwise-guarded sub-call proceeds without owner consent
  └── Remove the CredenceDelegation leaf from the auth tree
      └── BLOCKED: Soroban host enforces every require_auth leaf is present;
            missing entries cause transaction-level rejection, not partial execution
```

---

### Attack: wrap a legitimate delegated call inside a malicious contract

**STRIDE**: S

```
GOAL: owner authorises ContractA; attacker reroutes through malicious ContractC
  └── ContractC calls execute_delegated_delegate on behalf of ContractA
      └── BLOCKED: owner.require_auth() ensures owner's signature covers the
            exact call shape — contract address, function name, and arguments
            are all bound in the Soroban auth hash
```

See also: [docs/auth-tree-threats.md](../../../docs/auth-tree-threats.md) § Root Hijacking.

---

## 10. Information disclosure

### Attack: enumerate all delegations by brute-forcing storage keys

**STRIDE**: I

```
GOAL: discover all active delegations between addresses
  └── Scan DataKey::Delegation(owner, delegate, type) entries via Horizon
      NOTE: Soroban persistent storage is public on Stellar — delegation records
      are intentionally transparent to allow verifiers and indexers to read them.
      The design assumes addresses are pseudonymous, not private. There is no
      secret state stored in this contract.
```

---

## Summary — mitigations per STRIDE category

| Category | Key controls |
|----------|-------------|
| **S** Spoofing | `owner.require_auth()` on every mutation; payload `contract_id` binding; payload `target` binding |
| **T** Tampering | Domain-tag verification per action; expiry boundary enforcement (`ExpiryInPast`, `DelegationExpiryTooLong`); `InGrace` is informational only — hard cliff at `expires_at` |
| **R** Repudiation | Per-identity monotone nonces (`consume_nonce`); payload staleness guard (`check_payload_age`); delegation creation events emitted on every state change |
| **I** Disclosure | No secret state; delegation records are public by design; pseudonymity through address abstraction |
| **D** DoS | Pause threshold quorum; `MAX_NONCE_INVALIDATION_SPAN = 10_000` caps single nonce jumps; delegation TTL limits storage bloat; `cleanup_expired` is permissionless |
| **E** Privilege | Admin-only verifier registration; admin-only pause-signer management; authority is a hard cliff — `InGrace` never grants delegate rights |
