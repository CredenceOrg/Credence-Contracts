# Bytes32 Input Canonicalisation

This document describes the canonicalisation rules for fixed-size 32-byte arrays (`BytesN<32>`) accepted by the Credence smart contracts, and details the default-value rejection checks implemented as a security baseline.

## Audience
This document is written for smart contract contributors and downstream integrators who interact with Credence smart contract entrypoints accepting `BytesN<32>` arguments.

## Representation of Bytes32
In Soroban, 32-byte fields (such as cryptographic hashes, public keys, and unique identifiers) are typically represented by the `BytesN<32>` type.
`BytesN<32>` represents a fixed-length wrapper around a 32-byte array on the host environment.

## Validation and Default Value Rejection
As a defence-in-depth measure, all `BytesN<32>` inputs representing critical configuration, identifier, or cryptographic keys must be validated to ensure they are not default-initialized (i.e. all-zeros). Accidental zero inputs typically arise from client-side integration bugs where uninitialized fields are sent.

### Rejection Helper
The contract provides a shared validator function to verify that a `BytesN<32>` is not all-zero:

```rust
use soroban_sdk::{BytesN, Env};
use credence_errors::ContractError;

pub fn require_non_zero_bytes32(e: &Env, x: &BytesN<32>) {
    if x.clone().to_array() == [0u8; 32] {
        ::soroban_sdk::panic_with_error!(e, ContractError::ZeroBytes32);
    }
}
```

If the input is all-zero, the execution immediately panics with the typed error code `ContractError::ZeroBytes32` (error code `116`).

## Threat Model & Security Implications

### 1. Default-value Bypass
If an entrypoint uses `BytesN<32>` as a key to look up a profile, admin state, or auth requirement, an uninitialized zero-key lookup might match an empty or default storage slot, resulting in unintended authorization bypass or storage corruption.

### 2. Confused Deputy on Setup
During contract initialization or configuration changes, passing an all-zero `BytesN<32>` (such as a default hash or registry key) might set the target address or key to a zero-state. An attacker could exploit this by invoking functions against the zero-value address/registry, or the contract could permanently lock up key features if it expects a valid external component.

### 3. Misconfigured Callback / Integration
Downstream integrators might fail to construct a correct payload, resulting in default client-side buffers (e.g. `Buffer.alloc(32)` in Node.js) being transmitted. Rejecting these early at the contract boundary prevents invalid state transitions.

## Concrete Example

Consider a registry entrypoint that sets a cryptographic salt or configuration identifier:

```rust
use soroban_sdk::{contractimpl, BytesN, Env};

pub struct RegistryContract;

#[contractimpl]
impl RegistryContract {
    pub fn set_salt(env: Env, salt: BytesN<32>) {
        // Enforce security guard
        credence_errors::require_non_zero_bytes32(&env, &salt);
        
        // Write to storage...
    }
}
```

If `set_salt` is called with:
* `[0x00, 0x00, ..., 0x00]`: Panics immediately with `ContractError::ZeroBytes32` (code `116`).
* `[0x01, 0x00, ..., 0x00]`: Passes verification.
* `[0xff, 0xff, ..., 0xff]`: Passes verification.
