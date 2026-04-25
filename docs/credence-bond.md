# Crate: Credence Bond

**Path:** `contracts/credence-bond`

## Overview

The Credence Bond contract is the foundational security layer for the protocol. It manages the lifecycle of USDC-collateralized bonds, tiered identity statuses, and the enforcement of slashing via governance.

## Architecture & Flow

1. **Bonding**: User locks USDC -> Receives `IdentityBond` status.
2. **Attestation**: Authorized Verifiers sign claims against the bond.
3. **Slashing**: Multi-sig governance can deduct funds for protocol violations.

## 1. Entrypoint Reference

### Bond Management

| Function                   | Roles    | Description                                                                         |
| :------------------------- | :------- | :---------------------------------------------------------------------------------- |
| `create_bond`              | Identity | Locks USDC for a fixed duration. Validates against `SupplyCap` and `MinBondAmount`. |
| `create_bond_with_rolling` | Identity | Creates a bond that auto-renews at the end of the term unless a notice is filed.    |
| `start_withdrawal_notice`  | Identity | Begins the mandatory cooldown period before a bond can be unlocked.                 |
| `withdraw_expired_bond`    | Identity | Reclaims USDC after the notice period and bond duration have both lapsed.           |

### Attestations (Identity Logic)

| Function             | Roles    | Description                                                                     |
| :------------------- | :------- | :------------------------------------------------------------------------------ |
| `add_attestation`    | Verifier | Issues a signed claim for a subject. Requires a valid nonce to prevent replays. |
| `revoke_attestation` | Verifier | Invalidates an existing claim. Only the original issuer can revoke.             |

### Governance & Slashing

| Function                    | Roles     | Description                                                                 |
| :-------------------------- | :-------- | :-------------------------------------------------------------------------- |
| `propose_slash`             | Admin/Gov | Opens a `SlashProposal` for a specific identity bond.                       |
| `vote`                      | Governor  | Records an approval or rejection for a proposal. Supports delegation logic. |
| `execute_slash_if_approved` | Public    | Finalizes the movement of slashed funds to the treasury if quorum is met.   |

## 2. Role Definitions & Access Control

Managed via `access_control.rs`. All restricted paths emit `access_denied` on unauthorized calls.

- **Admin (`ADMIN_KEY`)**: Manages verifier registration, initializes governance, and sets global constants (BPS, leverage).
- **Verifier (`VERIFIER_PREFIX`)**: Authorized addresses that can validate identity claims.
- **Governor**: A set of addresses (defined in `governance_approval`) authorized to vote on slashing.
- **Identity Owner**: The address that owns the locked capital.

## 3. Backend Integration Notes

- **Nonce Handling**: Before calling `add_attestation`, the backend **must** fetch the current nonce via `get_nonce(identity)`. If the transaction fails, the nonce remains unchanged; if it succeeds, it increments.
- **Slashing Lifecycle**:
  1. Monitor `slash_proposed` events.
  2. Backend triggers notifications for `Governors`.
  3. Check `is_approved(proposal_id)` before attempting to call `execute_slash_if_approved` to save gas.
- **Storage Keys**: Verifier roles are stored in instance storage as `(Symbol("verifier"), Address)`. Check this before routing verifier-only UI actions.

## Function Reference

### `create_bond_with_rolling(caller, amount, duration)`

Creates a bond that auto-renews.

- **Validation**: Ensures `amount >= MinBondAmount` and `TotalSupply + amount <= SupplyCap`.
- **Event**: Emits `bond_created`.

### `add_attestation(verifier, subject, claim_hash, nonce, deadline)`

Verifiers use this to attach metadata/claims to a bonded identity.

- **Security**: Validates `verifier` role and prevents replay via `nonce`.
- **Replay Protection**: The `deadline` prevents "zombie" transactions from being executed during high network congestion.

### `propose_slash(proposer, identity, amount) -> u64`

Initiates a governance proposal to slash a specific bond.

- **Constraint**: `amount` cannot exceed the current locked balance of the identity.
