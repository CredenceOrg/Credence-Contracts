# Bond Issuance

Who can create a `credence_bond` identity bond, and under what conditions.

**Audience:** contributors and reviewers working on `credence_bond`. If you
are integrating against a deployed contract from a backend or wallet, see
[architecture.md](architecture.md) and
[bond-input-constraints.md](../contracts/credence_bond/docs/bond-input-constraints.md)
for the wire-level contract; this document explains the *access-control*
side — who is allowed to call the entry points and what must already be true
on-chain for the call to succeed.

---

## Who can issue a bond

There is no admin-gated "issuer" role. Bond issuance is **self-service**:
any Stellar `Address` can create a bond for itself by calling `create_bond`
and authenticating as the `identity` it is creating the bond for.

```rust
// contracts/credence_bond/src/lib.rs
pub fn create_bond(
    e: Env,
    identity: Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
    notice_period_duration: u64,
) -> Result<Bond, ContractError> {
    Self::require_not_paused(&e);
    identity.require_auth();
    // ...
}
```

`identity.require_auth()` is the only authorization check — the caller must
hold the signing key for `identity`. There is no separate `admin` or
`verifier` role involved in creating a bond; those roles govern *other*
lifecycle actions (slashing, attester registration, upgrades — see
[access-control.md](access-control.md)), not issuance itself.

The one contract-level gate that sits in front of issuance is
`set_accepted_tokens`, which **is** admin-only:

```rust
pub fn set_accepted_tokens(e: Env, admin: Address, accepted_tokens: Vec<Address>) {
    Self::require_not_paused(&e);
    admin.require_auth();
    if Some(admin) != storage::get_admin(&e) {
        panic_with_error!(e, ContractError::NotAdmin);
    }
    crate::validation::require_non_empty_vec(&e, &accepted_tokens);
    storage::set_accepted_tokens(&e, &accepted_tokens);
}
```

An admin configures which token addresses the deployment will accept before
any bond can be funded in that token. This is a one-time (or infrequent)
deployment-configuration step, not part of the per-bond issuance flow.

---

## Conditions that must hold to issue a bond

`create_bond` returns a typed `ContractError` — never a bare panic — for
every rejected input or state. In the order they are checked:

| # | Condition | Failure | Error |
|---|-----------|---------|-------|
| 1 | Contract is not paused | Admin/multisig has paused the contract | `ContractError::ContractPaused` |
| 2 | Caller signs as `identity` | Caller cannot produce `identity`'s auth | Soroban auth failure (not a `ContractError`) |
| 3 | `identity` does not already have a bond on this contract instance | A bond already exists | `ContractError::BondAlreadyExists` |
| 4 | `amount > 0` | `amount <= 0` | `ContractError::InvalidBondAmount` |
| 5 | `duration > 0` | `duration == 0` | `ContractError::InvalidBondDuration` |
| 6 | If `is_rolling`: `0 < notice_period_duration <= duration` | Notice period is `0` or exceeds `duration` | `ContractError::InvalidNoticePeriod` |
| 7 | `bond_start + duration` does not overflow `u64` | Overflow | `ContractError::Overflow` |

If all checks pass, the contract:

1. Pulls `amount` of the configured token from `identity` into the contract
   via `safe_token::transfer_in` (balance-delta verified — see
   [CROSS_CONTRACT_TRUST.md](CROSS_CONTRACT_TRUST.md) for why fee-on-transfer
   tokens are rejected here).
2. Persists the new `Bond` record keyed by `identity`.
3. Emits `bond_created_v2` (see [EVENTS.md](EVENTS.md)) with the amount,
   duration, and rolling flag, for off-chain indexing.

One bond contract instance holds **one bond per identity**. There is no
"batch issuance" path in the deployed contract — the `batch` module that
creates several bonds atomically is `testutils`-only and is not compiled
into the release WASM (see [BATCH_ATOMICITY.md](BATCH_ATOMICITY.md)).

---

## Topping up an existing bond

Once a bond exists, the same `identity` can increase it with `top_up`,
which follows the same self-authorization pattern:

```rust
pub fn top_up(e: Env, identity: Address, amount: i128) -> Result<(), ContractError> {
    Self::require_not_paused(&e);
    identity.require_auth();
    if !is_valid_bond(amount) {
        return Err(ContractError::InvalidBondAmount);
    }
    // ... loads the existing bond, adds `amount`, checked for overflow
}
```

`top_up` requires a bond to already exist (`ContractError::BondNotFound` if
not) and rejects the same non-positive `amount` as `create_bond`
(`ContractError::InvalidBondAmount`), via the shared `is_valid_bond` helper —
the single source of truth for the "amount must be positive" rule.

---

## What "amount" validation actually enforces today

`is_valid_bond(amount)` (`contracts/credence_bond/src/lib.rs`) only checks
`amount > 0`. `validation.rs` also defines `validate_bond_amount` and
`validate_bond_duration` helpers with `MIN_BOND_AMOUNT`/`MAX_BOND_AMOUNT` and
day-based duration bounds, but neither is currently called from
`create_bond` or `top_up` — they are not part of the enforced issuance path.
Do not rely on a minimum/maximum bond size or a minimum bond duration until
those helpers are actually wired in; only the conditions in the table above
are enforced today.

---

## See also

- [access-control.md](access-control.md) — roles for slashing, attester
  registration, and other post-issuance actions.
- [bond-input-constraints.md](../contracts/credence_bond/docs/bond-input-constraints.md) —
  full parameter-validation reference.
- [bond-state-transitions.md](bond-state-transitions.md) — lifecycle after
  issuance.
- [EVENTS.md](EVENTS.md) — event schema for `bond_created_v2`.
- [CROSS_CONTRACT_TRUST.md](CROSS_CONTRACT_TRUST.md) — token custody and
  trust assumptions during the transfer-in step.
