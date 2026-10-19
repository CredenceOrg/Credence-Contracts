# Crate: credence_bond

**Path:** `contracts/credence_bond`

## Overview

The bond crate is the core collateral and lifecycle contract for Credence identities. It records bond state, enforces the owner-authenticated lifecycle transitions, and exposes the admin-only slashing path used by governance.

This page is intentionally scoped to the public entrypoints and integration concerns that matter most to integrators and operators reviewing the crate.

## Entrypoints

| Entrypoint | Required role | Notes |
| :--------- | :------------ | :---- |
| `initialize` | Admin | Stores the contract admin and marks the instance as initialized. |
| `set_accepted_tokens` | Admin | Updates the list of accepted token addresses for collateral flow. |
| `create_bond` | Identity owner | Creates a new bond and transfers collateral from the identity into the contract. |
| `top_up` | Identity owner | Adds more collateral to an existing bond. |
| `extend_duration` | Identity owner | Extends the unlock deadline for an existing bond. |
| `request_withdrawal` | Identity owner | Starts the notice period for rolling bonds. |
| `withdraw` | Identity owner | Releases collateral once the lockup or notice window has elapsed. |
| `slash` | Admin | Applies a slash against a bond and updates the slashed balance. |

## Required roles

- **Admin**: Can initialize the contract, manage the accepted-token list, and slash bonds.
- **Identity owner**: Must be the transaction signer for any bond lifecycle operation that touches that identity's balance.

## Backend integration notes

- Treat the identity address as the transaction signer for `create_bond`, `top_up`, `extend_duration`, `request_withdrawal`, and `withdraw`.
- For rolling bonds, backends should call `request_withdrawal` first and only attempt `withdraw` once the notice period has elapsed.
- A slash reduces the effective withdrawable balance; off-chain UIs should use the persisted slashed amount when presenting available collateral.
- Event consumers should watch for bond lifecycle and slash events so they can update state without polling the contract repeatedly.

For the contributor-facing lifecycle reference, see [Bond State Transitions](bond-state-transitions.md).
