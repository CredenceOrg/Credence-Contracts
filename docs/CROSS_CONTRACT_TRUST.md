# Cross-Contract Trust Models

**Audience:** Contributors

This document explains which contracts the Credence protocol calls, what we trust them for, and how failure in those dependencies affects us.

When modifying core contracts, contributors must preserve these trust assumptions and ensure that malicious or buggy external contracts cannot compromise user funds or system invariants.

## 1. Verifier Contracts

**Caller:** `credence_delegation`  
**Callee:** Custom Signature Verifier Contracts

When a delegate acts on behalf of an owner, `credence_delegation` dynamically forwards signature verification to a custom verifier contract.

**What we trust it for:**
We trust the verifier to return a `bool` indicating whether the signature is valid for the given message and owner.

```rust
// Example verification call in credence_delegation
let is_valid: bool = client.verify(&owner, &message, &signature);
```

**Failure mode:**
If a verifier returns `true` for a malicious signature, an attacker can impersonate the owner. If a verifier panics or loops forever, the delegation fails (Denial of Service).

## 2. USDC Token Contract

**Caller:** `credence_bond` and `credence_treasury`  
**Callee:** External Token (typically USDC)

Our core contracts hold user funds and interact with the token contract to move them.

**What we trust it for:**
We trust it to implement the standard token interface properly (e.g., `transfer_from`, `transfer`, `allowance`). We also trust that if it returns success, the tokens actually moved. To mitigate risks with non-standard tokens (like fee-on-transfer tokens), we perform balance-delta verification.

```rust
// Concrete example: transferring USDC during bond creation
let balance_before = token.balance(&contract_address);
token.transfer_from(&spender, &owner, &contract_address, &amount);
let balance_after = token.balance(&contract_address);
assert!(balance_after == balance_before + amount);
```

**Failure mode:**
If the token contract has a bug or is paused, users cannot create bonds, withdraw, or be slashed.

## 3. Callback Contracts

**Caller:** `credence_bond`  
**Callee:** Optional Callback Hooks configured by the bond

Bonds can be configured to invoke a callback upon specific lifecycle events like withdrawals or slashing.

**What we trust it for:**
We trust the callback contract *not* to panic.

```rust
// Concrete example: calling back on withdrawal
if let Some(callback_addr) = config.callback_hook {
    let client = CallbackClient::new(&env, &callback_addr);
    client.on_withdraw(&withdraw_amount);
}
```

**Failure mode:**
Since the callback is synchronous, if the callback contract panics, it rolls back the entire transaction. A malicious callback could prevent withdrawals or slashing by unconditionally reverting.

## 4. Credence Registry

**Caller:** `credence_bond`  
**Callee:** `credence_registry`

**What we trust it for:**
Bonds can perform trustless self-registration by calling `register_trustless` on the registry. The bond trusts the registry to map it properly without requiring admin intervention. 

For full details on the pathways, see the [Cross-Contract Call Graph](cross-contract-call-graph.md).
