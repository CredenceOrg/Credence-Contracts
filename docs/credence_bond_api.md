

# CredenceBond Smart Contract API

The **CredenceBond** contract is a Soroban-based identity staking protocol. It allows users to "bond" (stake) tokens to establish an on-chain identity tier, which authorized verifiers can then vouch for via attestations. It includes governance-managed slashing, rolling renewals, and reentrancy protection.

## Table of Contents

* [Data Structures](https://www.google.com/search?q=%23data-structures)
* [Initialization & Admin](https://www.google.com/search?q=%23initialization--admin)
* [Bond Management](https://www.google.com/search?q=%23bond-management)
* [Attestation System](https://www.google.com/search?q=%23attestation-system)
* [Governance & Slashing](https://www.google.com/search?q=%23governance--slashing)
* [Read-Only View Functions](https://www.google.com/search?q=%23read-only-view-functions)

---

## Data Structures

### `IdentityBond`

| Field | Type | Description |
| --- | --- | --- |
| `identity` | `Address` | The owner of the bond. |
| `bonded_amount` | `i128` | The current net amount staked. |
| `bond_start` | `u64` | Timestamp when the bond started. |
| `bond_duration` | `u64` | The lock-up period in seconds. |
| `slashed_amount` | `i128` | Total amount lost to slashing. |
| `active` | `bool` | Whether the bond is currently active. |
| `is_rolling` | `bool` | If true, the bond auto-renews at the end of duration. |
| `withdrawal_requested_at` | `u64` | Timestamp of withdrawal request (for rolling bonds). |
| `notice_period_duration` | `u64` | Required lead time for rolling bond withdrawal. |

### `BondTier`

An enum representing the user's reputation level:

* `Bronze`: Entry level.
* `Silver`: Medium stake.
* `Gold`: High stake.
* `Platinum`: Elite stake.

---

## Initialization & Admin

### `initialize(e: Env, admin: Address)`

Sets the primary administrator for the contract. This can only be called once.

### `set_token(e: Env, admin: Address, token: Address)`

Configures the token (e.g., USDC) used for bonding.

* **Auth**: Admin signature required.

### `register_attester(e: Env, attester: Address)`

Whitelists an address to allow it to submit attestations for other identities.

* **Auth**: Admin signature required.

---

## Bond Management

### `create_bond(...)`

Creates a standard or rolling bond. Transfers tokens from the identity to the contract.

* **Params**: `identity`, `amount`, `duration`, `is_rolling`, `notice_period_duration`.

### `top_up(e: Env, amount: i128)`

Increases the stake of an existing bond to reach a higher `BondTier`.

### `request_withdrawal(e: Env)`

**Required for Rolling Bonds.** Initiates the notice period. You cannot withdraw a rolling bond without calling this first and waiting for the `notice_period_duration`.

### `withdraw_bond(e: Env, amount: i128)`

Withdraws funds after the lock-up or notice period has elapsed.

* **Note**: If called before the end of the lock-up on a standard bond, it will panic. Use `withdraw_early` instead.

### `withdraw_early(e: Env, amount: i128)`

Withdraws funds before the duration is over.

* **Penalty**: Applies a penalty defined in the `early_exit_penalty` module, which is sent to the treasury.

---

## Attestation System

### `add_attestation(...)`

Allows a registered attester to vouch for a subject.

* **Params**: `attester`, `subject`, `attestation_data`, `nonce`.
* **Features**: Uses a `dedup_key` to prevent the same attester from submitting the same data twice for the same subject.

### `revoke_attestation(e: Env, attester: Address, attestation_id: u64, nonce: u64)`

Allows the original verifier to cancel an attestation they previously issued.

---

## Governance & Slashing

The contract uses a delegated governance model to ensure slashing is fair.

### `initialize_governance(...)`

Sets up the council of governors and the quorum requirements for slashing proposals.

### `propose_slash(e: Env, proposer: Address, amount: i128)`

Creates a proposal to slash a bond. Must be called by the Admin or a Governor.

### `governance_vote(e: Env, voter: Address, proposal_id: u64, approve: bool)`

Governors cast their vote on a pending slash proposal.

### `execute_slash_with_governance(e: Env, proposer: Address, proposal_id: u64)`

If the quorum is reached (e.g., 51% approval), the proposer executes this function to finalize the slash and move funds.

---

## Read-Only View Functions

| Function | Returns | Description |
| --- | --- | --- |
| `get_identity_state` | `IdentityBond` | Returns all data for the current bond. |
| `get_tier` | `BondTier` | Calculates the tier based on `bonded_amount`. |
| `is_attester` | `bool` | Checks if an address is an authorized verifier. |
| `get_subject_attestations` | `Vec<u64>` | Lists all attestation IDs for a specific user. |
| `get_nonce` | `u64` | Gets the next expected nonce for replay protection. |
| `is_locked` | `bool` | Checks if the reentrancy guard is currently active. |

---

### 🛡 Security Features

* **Reentrancy Guard**: Functions involving external callbacks use `with_reentrancy_guard` to prevent recursive attacks.
* **CEI Pattern**: All state updates (Checks-Effects) happen before external token Interactions.
* **Replay Prevention**: Nonces are consumed for every sensitive attestation action.
