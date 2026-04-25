# Crate: Credence Delegation

**Path:** `contracts/credence-delegation`

## Overview

Enables "Gasless" operations and delegated authority. It allows identity owners to delegate their management or attestation rights to other addresses (e.g., hot wallets or third-party verifiers) using off-chain signatures.

## 1. Entrypoint Reference

| Function               | Roles   | Description                                                                                                  |
| :--------------------- | :------ | :----------------------------------------------------------------------------------------------------------- |
| `delegate`             | Owner   | Synchronous delegation of rights to a `delegatee`.                                                           |
| `execute_delegated_op` | Relayer | Executes a management action (e.g., bond adjustment) on behalf of an owner via EIP-712-style signed payload. |
| `revoke_delegation`    | Owner   | Removes a previously granted delegation.                                                                     |

## 2. Integration Notes

- **Domain Tags**: Payloads are signed with a `DomainTag`. The backend must ensure the tag matches the specific action (e.g., `Management` vs `Attestation`) to avoid cross-action replays.
- **Relayer Pattern**: The backend can act as the `Relayer`, paying the XLM fee for the transaction while the user only provides a signature.
