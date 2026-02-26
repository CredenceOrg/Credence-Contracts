# Token Integration (USDC)

This document describes how the Credence bond contract integrates with Stellar token contracts for USDC-denominated bonds.

## Overview

The bond contract uses Soroban token interfaces for all value movements:

- `create_bond` and `top_up` move tokens from identity to contract with `transfer_from`.
- `withdraw_bond` and `withdraw_early` move tokens from contract to recipients with `transfer`.
- `set_usdc_token(admin, token, network)` stores a USDC token address plus network label (`mainnet` or `testnet`).

## Contract API

- `set_token(admin, token)`
  - Backward-compatible token setter.
  - Admin-only.
- `set_usdc_token(admin, token, network)`
  - Admin-only USDC-specific setter.
  - Network must be `mainnet` or `testnet`.
- `get_usdc_token()`
  - Returns currently configured token address.
- `get_usdc_network()`
  - Returns configured USDC network label when available.

## Security Model

Token handling is centralized in `contracts/credence_bond/src/token_integration.rs` with the following controls:

1. **Admin-gated token configuration**
   - Only stored admin can set token address.
2. **Allowance pre-checks**
   - Before `transfer_from`, contract checks `allowance(owner, contract)`.
   - If allowance is insufficient, call fails with `insufficient token allowance`.
3. **Non-negative amount validation**
   - All token transfer helper paths reject negative amounts.
4. **No-op zero transfers**
   - Amount `0` exits early without external token calls.
5. **Single integration layer**
   - Prevents duplicated transfer logic and keeps security review surface small.

## Assumptions

- Admin sets a valid USDC token contract address for the intended Stellar network.
- Identity accounts grant approvals to the bond contract before `create_bond` and `top_up`.
- Token contract adheres to Soroban token interface semantics.

## Test Coverage (Integration-Specific)

`contracts/credence_bond/src/token_integration_test.rs` covers:

- USDC token/network configuration and retrieval.
- Rejection of unsupported network label.
- Successful token movement into contract during `create_bond`.
- Failure on missing allowance for `create_bond`.
- Failure when `top_up` exceeds remaining allowance.
- Successful token movement back to identity on withdrawal.

Run targeted tests:

```bash
cargo test -p credence_bond token_integration_test -- --nocapture
```

Run full package tests:

```bash
cargo test -p credence_bond -- --nocapture
```
