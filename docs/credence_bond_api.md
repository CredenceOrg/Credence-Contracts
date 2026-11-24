# CredenceBond Smart Contract API

The **CredenceBond** contract is a Soroban-based identity staking protocol. It allows users to "bond" (stake) tokens to establish an on-chain identity tier, which authorized verifiers can then vouch for via attestations. It includes governance-managed slashing, rolling renewals, and reentrancy protection.

## Table of Contents

- [Data Structures](https://www.google.com/search?q=%23data-structures)
- [Initialization & Admin](https://www.google.com/search?q=%23initialization--admin)
- [Bond Management](https://www.google.com/search?q=%23bond-management)
- [Attestation System](https://www.google.com/search?q=%23attestation-system)
- [Governance & Slashing](https://www.google.com/search?q=%23governance--slashing)
- [Read-Only View Functions](https://www.google.com/search?q=%23read-only-view-functions)

---

## Data Structures

### `IdentityBond`

| Field                     | Type      | Description                                           |
| ------------------------- | --------- | ----------------------------------------------------- |
| `identity`                | `Address` | The owner of the bond.                                |
| `bonded_amount`           | `i128`    | The current net amount staked.                        |
| `bond_start`              | `u64`     | Timestamp when the bond started.                      |
| `bond_duration`           | `u64`     | The lock-up period in seconds.                        |
| `slashed_amount`          | `i128`    | Total amount lost to slashing.                        |
| `active`                  | `bool`    | Whether the bond is currently active.                 |
| `is_rolling`              | `bool`    | If true, the bond auto-renews at the end of duration. |
| `withdrawal_requested_at` | `u64`     | Timestamp of withdrawal request (for rolling bonds).  |
| `notice_period_duration`  | `u64`     | Required lead time for rolling bond withdrawal.       |

### `BondTier`

An enum representing the user's reputation level:

- `Bronze`: Entry level.
- `Silver`: Medium stake.
- `Gold`: High stake.
- `Platinum`: Elite stake.

---

## Initialization & Admin

### `initialize(e: Env, admin: Address, token: Address)`

Sets the primary administrator and the custody token for the contract. This can only be called once.

### `get_token(e: Env) -> Address`

Returns the configured custody token address.

### `set_token(e: Env, admin: Address, token: Address)`

Configures the token (e.g., USDC) used for bonding.

- **Auth**: Admin signature required.

### `register_attester(e: Env, attester: Address)`

Whitelists an address to allow it to submit attestations for other identities.

- **Auth**: Admin signature required.

---

## Bond Management

### `create_bond(...)`

Creates a standard or rolling bond. Pulls approved custody tokens from the identity into the contract.

- **Params**: `identity: Address`, `amount: i128`, `duration: u64`, `is_rolling: bool`, `notice_period_duration: u64`.
- **Auth**: `identity.require_auth()`
- **Invariant**: on success, the contract custody increases by `amount`.
- **Panics**: **CRITICAL** This function will panic if an active bond already exists for the specified `identity`. To increase stakes on an active identity bond, clients must call `top_up` instead.

#### Input Constraints

All parameters are validated before the bond is created. Every violation returns a typed
`ContractError` — no panics are used.

| Parameter                     | Constraint                                      | Error (code)                |
| ----------------------------- | ----------------------------------------------- | --------------------------- |
| `amount: i128`                | Must be strictly positive (`> 0`)               | `InvalidBondAmount` (214)   |
| `duration: u64`               | Must be strictly positive (`> 0`)               | `InvalidBondDuration` (215) |
| `notice_period_duration: u64` | When `is_rolling = true`: must be `> 0`         | `InvalidNoticePeriod` (216) |
| `notice_period_duration: u64` | When `is_rolling = true`: must be `<= duration` | `InvalidNoticePeriod` (216) |
| `bond_start + duration`       | Must not overflow `u64`                         | `Overflow` (700)            |

Checks are applied in the order listed above, so the first violated constraint is the one
reported. For non-rolling bonds, `notice_period_duration` is stored but not validated.

> See [`contracts/credence_bond/docs/bond-input-constraints.md`](../contracts/credence_bond/docs/bond-input-constraints.md)
> for the full constraint reference including boundary examples and security notes.

### `top_up(e: Env, identity: Address, amount: i128)`

Increases the stake of an existing bond to reach a higher `BondTier`.
Pulls additional approved custody tokens from the bonded identity into the contract.

- **Auth**: `identity.require_auth()`

### `request_withdrawal(e: Env, identity: Address)`

**Required for Rolling Bonds.** Initiates the notice period for a given identity. You cannot withdraw a rolling bond without calling this first and waiting for the `notice_period_duration`.

- **Auth**: `identity.require_auth()`

### `withdraw(e: Env, identity: Address, amount: i128)` and `withdraw_bond(e: Env, identity: Address, amount: i128)`

Decrease the bonded amount by `amount` and, when a USDC token is configured via `set_token`, transfer `amount` USDC from the bond contract back to the bonded identity. Both entrypoints perform the same canonical withdrawal flow; `withdraw_bond` is the lifecycle-friendly alias name used by some clients.

- **Auth**: `identity.require_auth()`
- **Custody**: real on-chain `TokenClient::transfer` push, gated by `token_integration::has_token()`. State writes happen *before* the external transfer (Checks–Effects–Interactions) so a hostile token contract cannot re-enter and double-spend against a post-withdrawal snapshot.
- **Note**: If called before the end of the lock-up on a standard bond, the call panics. Use `withdraw_early` instead for an exit that routes a penalty to the early-exit treasury.

### `withdraw_early(e: Env, identity: Address, amount: i128)`

Withdraws funds for an identity before the duration is over.

- **Auth**: `identity.require_auth()`
- **Custody**: real on-chain split — the penalty is pushed to the early-exit treasury via `token_integration::transfer_from_contract_with_source(FundSource::ProtocolFee)` and `net_amount = amount - penalty` is pushed back to the identity. Both transfers use the balance-delta guard that rejects fee-on-transfer tokens.
- **Penalty**: Applies a penalty defined in the `early_exit_penalty` module; arithmetic is exact (`penalty + net_amount == amount`) and the invariant check (`ContractError::InvariantViolation`) reverts the entire transaction on drift.

---

## Attestation System

### `add_attestation(...)`

Allows a registered attester to vouch for a subject.

- **Params**: `attester`, `subject`, `attestation_data`, `nonce`.
- **Features**: Uses a `dedup_key` to prevent the same attester from submitting the same data twice for the same subject.

### `revoke_attestation(e: Env, attester: Address, attestation_id: u64, nonce: u64)`

Allows the original verifier to cancel an attestation they previously issued.

---

## Governance & Slashing

The contract uses a delegated governance model to ensure slashing is fair.

### `initialize_governance(...)`

Sets up the council of governors and the quorum requirements for slashing proposals.

### `propose_slash(e: Env, proposer: Address, identity: Address, amount: i128)`

Creates a proposal to slash a specific identity's bond. Must be called by the Admin or a Governor.

### `governance_vote(e: Env, voter: Address, proposal_id: u64, approve: bool)`

Governors cast their vote on a pending slash proposal.

### `execute_slash_with_governance(e: Env, proposer: Address, proposal_id: u64)`

If the quorum is reached (e.g., 51% approval), the proposer executes this function to finalize the slash and move funds.

---

## Read-Only View Functions

| Function                                        | Returns        | Description                                                                 |
| ----------------------------------------------- | -------------- | --------------------------------------------------------------------------- |
| `get_identity_state(e: Env, identity: Address)` | `IdentityBond` | Returns all data for the current bond matching the given identity.          |
| `get_tier(e: Env, identity: Address)`           | `BondTier`     | Calculates the tier based on the identity's `bonded_amount`.                |
| `renew_if_rolling(e: Env, identity: Address)`   | `bool`         | Processes an automated renewal epoch for rolling bonds if conditions match. |
| `is_attester`                                   | `bool`         | Checks if an address is an authorized verifier.                             |
| `get_subject_attestations`                      | `Vec<u64>`     | Lists all attestation IDs for a specific user.                              |
| `get_nonce`                                     | `u64`          | Gets the next expected nonce for replay protection.                         |
| `is_locked`                                     | `bool`         | Checks if the reentrancy guard is currently active.                         |

---

### 🛡 Security Features

- **Reentrancy Guard**: Functions involving external callbacks use `with_reentrancy_guard` to prevent recursive attacks.
- **CEI Pattern**: All state updates (Checks-Effects) happen before external token Interactions.
- **Replay Prevention**: Nonces are consumed for every sensitive attestation action.
- **Custody**: Real token escrow for `create_bond`, `top_up`, `withdraw`, and `withdraw_early` is documented in [bond-token-custody.md](bond-token-custody.md).
