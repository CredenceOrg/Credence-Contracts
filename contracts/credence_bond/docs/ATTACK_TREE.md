# Attack Tree — `credence_bond`

**Audience**: Contributors and security auditors who want to verify that the
bond contract's implementation matches its documented security intent.

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
- [docs/security.md](../../../docs/security.md) — overflow, replay, and reentrancy mechanisms
- [docs/access-control.md](../../../docs/access-control.md) — entrypoint authority matrix
- [docs/THREAT_MODEL.md](../../../docs/THREAT_MODEL.md) — workspace-level STRIDE overview
- [docs/auth-tree-threats.md](../../../docs/auth-tree-threats.md) — Soroban auth-tree specifics
- [docs/reentrancy.md](../../../docs/reentrancy.md) — reentrancy guard design
- [docs/slashing.md](../../../docs/slashing.md) — slash mechanics and treasury routing
- [docs/emergency.md](../../../docs/emergency.md) — emergency mode and drain procedure

---

## 1. Initialisation

### Attack: double-initialise to replace admin

**STRIDE**: E  
**Entrypoint**: `initialize(admin, registry)`

```
GOAL: replace admin with attacker-controlled address
  ├── CALL initialize() a second time with a different admin
  │   └── BLOCKED: credence_errors::require_contract_uninitialized panics
  │         when DataKey::Admin already exists in storage (AlreadyInitialized)
  └── RACE the first call — submit own initialize before legitimate deployer
      └── BLOCKED: Soroban deploy-and-init is atomic in a single transaction;
            no ledger gap exists between contract creation and first initialize
```

**Mitigation code**: `credence_errors::require_contract_uninitialized(&e, e.storage().instance().has(&DataKey::Admin))` in `initialize`.

---

## 2. Bond creation and top-up

### Attack: create a bond on behalf of another identity

**STRIDE**: S  
**Entrypoint**: `create_bond(identity, amount, duration, is_rolling, notice_period_duration)`

```
GOAL: stake tokens on behalf of victim to lock their funds or influence their tier
  └── CALL create_bond(victim_address, ...)
      └── BLOCKED: identity.require_auth() — Soroban host rejects the call
            unless victim's signature is present in the transaction auth tree
```

**Mitigation code**: `identity.require_auth()` is the first guard in `create_bond`.

---

### Attack: create a bond with amount = 0 or negative amount

**STRIDE**: T  
**Entrypoint**: `create_bond`

```
GOAL: create a zero-value bond to "occupy" a slot or trigger arithmetic issues
  ├── Pass amount = 0
  │   └── BLOCKED: validation::validate_bond_amount panics (InvalidBondAmount)
  └── Pass amount = i128::MIN (negative)
      └── BLOCKED: same check (require_positive_amount! macro)
```

---

### Attack: overflow bonded_amount via successive top_ups

**STRIDE**: T  
**Entrypoint**: `top_up(identity, amount)`

```
GOAL: overflow bonded_amount to wrap around to a small value
  └── Repeatedly call top_up with large amounts
      └── BLOCKED: bond.bonded_amount.checked_add(amount) panics with
            ContractError::Overflow before any storage write
```

**Mitigation code**: `checked_add` in `top_up`; invariants checked by `invariants::assert_self_consistent`.

---

### Attack: bypass the same-ledger slash guard by topping up and slashing in one ledger

**STRIDE**: T  
**Entrypoints**: `top_up`, `slash_bond`

```
GOAL: sandwich — increase collateral then slash it in the same ledger for unfair sequencing
  └── In ledger N: call top_up; call slash_bond in same ledger
      └── BLOCKED: same_ledger_liquidation_guard records the ledger sequence
            on every collateral-increasing action; slash_bond rejects if
            LastCollateralIncreaseLedger == current ledger sequence
```

See also: [docs/security.md](../../../docs/security.md) § Same-ledger sequencing guardrails.

---

## 3. Withdrawal paths

### Attack: withdraw another identity's bond

**STRIDE**: S  
**Entrypoints**: `withdraw`, `withdraw_early`, `withdraw_bond`

```
GOAL: drain victim's bond
  └── Call withdraw(victim, amount)
      └── BLOCKED: identity.require_auth() + bond.identity != identity check
            ensures only the bond owner can withdraw
```

---

### Attack: withdraw before the lock-up ends (non-rolling bond)

**STRIDE**: T  
**Entrypoint**: `withdraw`

```
GOAL: reclaim locked funds before the agreed duration
  └── Call withdraw(identity, amount) before bond_start + bond_duration
      └── BLOCKED: now < end panics with LockupNotExpired
```

---

### Attack: skip the notice period on a rolling bond

**STRIDE**: T  
**Entrypoint**: `withdraw` / `withdraw_bond`

```
GOAL: withdraw immediately without waiting for notice period
  ├── Call withdraw without first calling request_withdrawal
  │   └── BLOCKED: withdrawal_requested_at == 0 check panics ("withdrawal not requested")
  └── Call withdraw immediately after request_withdrawal
      └── BLOCKED: now < withdrawal_requested_at + notice_period_duration panics
            ("notice period not elapsed")
```

---

### Attack: withdraw more than available (bonded − slashed)

**STRIDE**: T  
**Entrypoint**: `withdraw`, `withdraw_bond`

```
GOAL: extract more tokens than the bond actually holds
  └── Pass amount > (bonded_amount - slashed_amount)
      └── BLOCKED: available = bonded_amount.checked_sub(slashed_amount);
            amount > available panics with InsufficientBalance
```

---

### Attack: re-enter withdraw_bond via a hostile callback contract

**STRIDE**: T / E  
**Entrypoint**: `withdraw_bond`

```
GOAL: drain contract by triggering recursive withdrawal before state is zeroed
  └── Register a callback contract at DataKey::callback that calls withdraw_bond again
      └── BLOCKED: SettlingFlag reentrancy lock acquired before any external call;
            second entry panics with ContractError::ReentrancyDetected
```

See also: [docs/reentrancy.md](../../../docs/reentrancy.md).

---

## 4. Early exit

### Attack: bypass early exit penalty

**STRIDE**: T  
**Entrypoint**: `withdraw_early`

```
GOAL: withdraw before lock-up without paying the penalty
  ├── Call withdraw_early when no EarlyExitConfig is set
  │   └── BLOCKED: get_config() returns Err → panics with EarlyExitConfigNotSet
  └── Manipulate remaining time to reduce penalty to zero
      └── BLOCKED: remaining = end.saturating_sub(now); penalty = calculate_penalty(...)
            capped by invariant check: penalty + net == amount
```

---

### Attack: underflow penalty calculation to receive more than deposited

**STRIDE**: T  
**Entrypoint**: `withdraw_early`

```
GOAL: arithmetic error returns net_amount > bonded_amount
  └── Craft inputs so penalty calculation underflows
      └── BLOCKED: net_amount = amount.checked_sub(penalty) with Underflow error;
            split_total = net_amount.checked_add(penalty) must equal amount;
            InvariantViolation panic if not exactly equal
```

---

## 5. Slashing

### Attack: unauthorized slash

**STRIDE**: S / E  
**Entrypoints**: `slash`, `slash_bond`

```
GOAL: slash a victim's bond as a non-admin
  └── Call slash_bond(attacker_address, ...)
      └── BLOCKED: admin.require_auth() + stored_admin != admin check
            panics with ContractError::NotAdmin
```

---

### Attack: slash more than the bonded amount

**STRIDE**: T  
**Entrypoint**: `slash_bond`

```
GOAL: over-slash a bond to create a negative available balance
  └── Pass slash_amount > bonded_amount - existing_slashed
      └── BLOCKED: new_slashed = bond.slashed_amount + slash_amount;
            if new_slashed > bond.bonded_amount → SlashExceedsBond
```

---

### Attack: replay a slash operation

**STRIDE**: R  
**Entrypoint**: `slash_bond`

```
GOAL: re-submit the same slash transaction to deduct the same amount twice
  ├── Replay the exact same transaction at the network layer
  │   └── BLOCKED: Stellar/Soroban network-level transaction replay prevention
  └── Submit slash_bond with the same idempotency_salt a second time
      └── BLOCKED: idempotency::check_and_record detects the pre-stored key
            and panics with DuplicateIdempotencyKey
```

---

### Attack: route slashed funds to an attacker-controlled treasury

**STRIDE**: T  
**Entrypoint**: `set_slash_treasury`

```
GOAL: redirect slash proceeds to attacker's address
  └── Call set_slash_treasury(attacker_addr, ...)
      └── BLOCKED: admin.require_auth() + stored_admin != admin check
```

---

## 6. Attestations

### Attack: add an attestation as an unregistered attester

**STRIDE**: S  
**Entrypoint**: `add_attestation`

```
GOAL: inject a fraudulent attestation for a subject
  └── Call add_attestation(unregistered_attester, subject, data, nonce)
      └── BLOCKED: DataKey::Attester(attester) lookup returns false →
            panics with ContractError::UnauthorizedAttester
```

---

### Attack: replay a previously used attestation nonce

**STRIDE**: R  
**Entrypoint**: `add_attestation`, `revoke_attestation`

```
GOAL: reuse an already-consumed nonce to duplicate or ghost an attestation
  └── Re-submit with nonce N after it was already consumed
      └── BLOCKED: nonce::consume_nonce compares stored nonce to caller-supplied
            value; stored nonce was incremented → panics with InvalidNonce
```

---

### Attack: add a duplicate attestation to saturate storage

**STRIDE**: D  
**Entrypoint**: `add_attestation`

```
GOAL: bloat storage by re-submitting the same (attester, subject, data) triple
  └── Call add_attestation with identical parameters twice
      └── BLOCKED: AttestationDedupKey presence check panics with DuplicateAttestation
```

---

### Attack: revoke another attester's attestation

**STRIDE**: S  
**Entrypoint**: `revoke_attestation`

```
GOAL: silence another attester's valid attestation
  └── Call revoke_attestation(attacker, victim_attestation_id, nonce)
      └── BLOCKED: attestation.verifier != attester check panics with NotOriginalAttester
```

---

### Attack: exceed attestation weight limits in a batch

**STRIDE**: T  
**Entrypoint**: `add_attestation_batch`

```
GOAL: artificially inflate a subject's aggregate weight beyond protocol limits
  └── Construct a batch where sum of per-attester weights > max_weight
      └── BLOCKED: total_weight > max_weight as u64 panics with
            ContractError::AttestationWeightExceedsMax
```

---

## 7. Admin and upgrade authority

### Attack: steal the admin role

**STRIDE**: E  
**Entrypoint**: `transfer_admin`

```
GOAL: become the new admin without current admin co-signing
  └── Call transfer_admin(attacker, attacker)
      └── BLOCKED: current_admin.require_auth() AND new_admin.require_auth()
            — both signatures required; attacker cannot sign for the current admin
```

---

### Attack: abandon the admin role (zero-address hand-off)

**STRIDE**: D  
**Entrypoint**: `transfer_admin`

```
GOAL: leave the contract in a permanently admin-less state
  └── Call transfer_admin(current, ZERO_ADDRESS)
      └── BLOCKED: zero-address string comparison panics with InvalidAdminAddress
```

---

### Attack: unauthorised contract upgrade

**STRIDE**: E  
**Entrypoint**: `transfer_upgrade_admin`, `accept_upgrade_admin`

```
GOAL: swap in a malicious WASM blob
  └── Call transfer_upgrade_admin(attacker, attacker)
      └── BLOCKED: upgrade_auth module requires current upgrade admin's
            require_auth() — two-step hand-off documented in docs/UPGRADE.md
```

See also: [docs/UPGRADE.md](../../../docs/UPGRADE.md).

---

## 8. Liquidation

### Attack: liquidate a healthy bond

**STRIDE**: T  
**Entrypoint**: `liquidate`

```
GOAL: forcibly close a bond that still has unslashed collateral and active lock-up
  └── Call liquidate(admin, healthy_identity)
      └── BLOCKED: eligibility check panics with "bond is not eligible for
            liquidation" if slashed_amount < bonded_amount AND now < bond_start + bond_duration
```

---

### Attack: liquidate the same bond twice

**STRIDE**: D  
**Entrypoint**: `liquidate`

```
GOAL: trigger double-sweep to drain the treasury
  └── Call liquidate twice on the same identity
      └── BLOCKED: first call flips bond.active = false and sets
            DataKey::Liquidated(identity) = true; second call panics with BondNotActive
```

---

## 9. Pause / circuit-breaker

### Attack: pause the contract without a threshold of signers

**STRIDE**: D  
**Entrypoint**: `pause`, `execute_pause_proposal`

```
GOAL: single rogue pause-signer halts the contract
  └── Single signer calls pause directly
      └── BLOCKED: pausable module requires PauseThreshold approvals
            across registered PauseSigners before execution
```

---

### Attack: pause the contract to block legitimate withdrawals

**STRIDE**: D  
**Entrypoint**: Any state-changing entrypoint

```
GOAL: freeze withdrawals by abusing the pause mechanism
  └── Gather enough pause-signer approvals to reach threshold
      └── MITIGATED: pause signers are admin-managed; threshold is configurable;
            emergency mode provides a separate drain path
            See docs/emergency.md for the break-glass procedure
```

---

## 10. Fee collection

### Attack: drain fee balance as a non-admin

**STRIDE**: E  
**Entrypoint**: `collect_fees`

```
GOAL: steal protocol fee balance
  └── Call collect_fees(attacker, salt)
      └── BLOCKED: admin.require_auth() + stored_admin != admin check
```

---

### Attack: replay a fee collection with the same salt

**STRIDE**: R  
**Entrypoint**: `collect_fees`

```
GOAL: collect the same fee epoch twice
  └── Re-submit collect_fees with the same idempotency_salt
      └── BLOCKED: idempotency::check_and_record deduplicates by salt
```

---

## 11. Storage and information disclosure

### Attack: enumerate all bonds by brute-forcing storage keys

**STRIDE**: I

```
GOAL: discover all bond holders' amounts and states
  └── Read DataKey::Bond from every deployed instance
      NOTE: Soroban storage is public on Stellar — all instance storage
      is visible via Horizon. Bond state is NOT treated as secret;
      see docs/bond-introspection.md for the intentional read-only views.
      Operators who need privacy should use off-chain identity mapping.
```

---

### Attack: overflow the attestation counter to wrap to 0

**STRIDE**: T  
**Entrypoint**: `add_attestation`, `add_attestation_batch`

```
GOAL: force counter to wrap, overwriting older attestation records
  └── Add 2^64 attestations to overflow the u64 counter
      └── BLOCKED: next_id.checked_add(1) panics with Overflow;
            gas cost of 2^64 transactions is astronomically infeasible
```

---

## Summary — mitigations per STRIDE category

| Category | Key controls |
|----------|-------------|
| **S** Spoofing | `require_auth()` at every state-changing entrypoint; bond owner identity check |
| **T** Tampering | Checked arithmetic throughout; invariant assertions; same-ledger slash guard; weight caps |
| **R** Repudiation | Per-identity nonces with `consume_nonce`; idempotency keys on admin operations; event emission on every state change |
| **I** Disclosure | Storage is inherently public on Stellar; no secret state is stored; `describe_config` / `describe_bond` expose the same data via structured views |
| **D** DoS | Pause multi-sig threshold; batch size capped at `MAX_BATCH_ATTESTATION_SIZE = 64`; query results capped at `MAX_QUERY_LIMIT = 200`; reentrancy lock prevents recursive exhaustion |
| **E** Privilege | Two-signature admin transfer; upgrade two-step hand-off; attester allow-list; slash restricted to admin |
