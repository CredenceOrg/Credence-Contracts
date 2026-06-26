# Cross-Contract Call Graph & Authorization Flow

This document details the cross-contract call pathways, interface checks, callback architectures, and authorization checkpoints for the Credence protocol contracts.

---

## High-Level Call Graph

The following diagram illustrates how the core contracts (`credence_bond`, `credence_registry`, `credence_delegation`, and `credence_treasury`) interact with each other and with external dependencies (the USDC Token contract, custom Verifier contracts, and custom Callback contracts).

```mermaid
graph TD
    classDef auth fill:#e1f5fe,stroke:#0288d1,stroke-width:2px;
    classDef trust fill:#efebe9,stroke:#5d4037,stroke-width:2px;
    classDef ext fill:#eceff1,stroke:#607d8b,stroke-dasharray: 5 5;

    subgraph Legend
        AuthNode["require_auth Check"]:::auth
        TrustNode["CodeHash/Trustless Check"]:::trust
        ExtNode["External Target"]:::ext
    end

    subgraph Identity Layer
        Registry["credence_registry"]
    end

    subgraph Reputation & Stake
        Bond["credence_bond"]
        Treasury["credence_treasury"]
    end

    subgraph Rights Delegation
        Delegation["credence_delegation"]
    end

    subgraph External & Standards
        USDC["USDC Token Contract"]:::ext
        Verifier["Verifier Contract"]:::ext
        Callback["Callback Contract"]:::ext
    end

    %% Registry -> Bond (Interface Checks)
    Registry -->|supports_interface| Bond
    class Registry auth;

    %% Bond -> Registry (Trustless Registration)
    Bond -->|register_trustless| Registry
    Registry -->|get_contract_code_hash| Bond
    class Registry trust;

    %% Delegation -> Verifier
    Delegation -->|verify| Verifier

    %% Token custody & movements
    Bond -->|transfer_from / transfer| USDC
    Treasury -->|transfer / transfer_from| USDC

    %% Callbacks on state transition
    Bond -->|on_withdraw / on_slash / on_collect| Callback
```

---

## Detailed Call Edge Specifications

### 1. `credence_registry` $\leftrightarrow$ `credence_bond`

```mermaid
sequenceDiagram
    autonumber
    actor Admin
    actor Identity
    participant Registry as credence_registry
    participant Bond as credence_bond

    Note over Registry: Admin-driven Pairing
    Admin->>Registry: register_identity(identity, bond_contract, allow_non_interface)
    Note over Registry: Admin auth checked via require_auth()
    Registry->>Bond: supports_interface(IFACE_CREDENCE_BOND_V1)
    Bond-->>Registry: returns true/false
    Note over Registry: State updated & mapped

    Note over Bond: Trustless Self-Registration
    Identity->>Bond: initialize(admin, registry_address)
    Bond->>Registry: register_trustless(admin)
    Registry->>Bond: get_contract_code_hash()
    Bond-->>Registry: returns code_hash
    Note over Registry: Verified against pinned reference hash
```

- **`supports_interface` Check:** `register_identity` performs an ERC165-equivalent interface verification on the target `bond_contract` using the identifier `IFACE_CREDENCE_BOND_V1`.
- **`register_trustless` Hash Introspection:** To bypass the admin trust assumption, a bond contract can register itself with the registry. The registry calls `get_contract_code_hash` back on the caller and performs a constant-time memory comparison (`constant_time_eq`) against the admin-pinned reference WASM hash to ensure authenticity.

### 2. `credence_delegation` $\rightarrow$ `Verifier Contract`

```mermaid
sequenceDiagram
    autonumber
    actor Delegate
    participant Delegation as credence_delegation
    participant Verifier as Verifier Contract

    Delegate->>Delegation: verify(verifier_addr, owner, message, signature)
    Delegation->>Verifier: verify(owner, message, signature)
    Verifier-->>Delegation: returns bool (success status)
```

- **Dynamic Signature Dispatch:** The delegation contract dynamically forwards verification requests to specialized signature schemes. The target contract must implement:
  ```rust
  pub fn verify(e: Env, owner: Address, message: Bytes, signature: Bytes) -> bool
  ```
- **Rejection Propagation:** Any verification failure or panic in the external verifier rolls back the delegation contract call.

### 3. Core Contracts $\rightarrow$ `USDC Token` (Token Contract)

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant Contract as credence_bond / credence_treasury
    participant Token as USDC Token Contract

    User->>Contract: create_bond_with_rolling(amount, ...) / execute_withdrawal(...)
    Note over Contract: require_auth() verified
    Contract->>Token: allowance(owner, contract)
    Token-->>Contract: returns allowance_amount
    Contract->>Token: transfer_from(spender, owner, contract, amount)
    Note over Contract: balance delta verification (rejects fee-on-transfer tokens)
```

- **Front-Running & Allowance Guards:** Both the bond and treasury contracts invoke standard token methods (`transfer`, `transfer_from`, `allowance`, `approve`).
- **Balance-Delta Verification:** In custody actions, contracts query the token balance before and after execution to enforce that the exact expected amount is transferred, mitigating risks of fee-on-transfer tokens.

### 4. `credence_bond` $\rightarrow$ `Callback Contract`

```mermaid
sequenceDiagram
    autonumber
    actor Trigger
    participant Bond as credence_bond
    participant Callback as Callback Contract

    Trigger->>Bond: withdraw() / slash_bond() / collect_fees()
    Note over Bond: State transitioned
    Bond->>Callback: on_withdraw(amount) / on_slash(amount) / on_collect(amount)
```

- **Callback Hook Safety:** If a `callback` contract address is configured in the bond instance, it will be invoked on key lifecycle transitions:
  - `on_withdraw(withdraw_amount)`
  - `on_slash(slash_amount)`
  - `on_collect(fee_amount)`
- **Atomic Rollback Hook:** Since the call is inline and synchronous, any callback failures or panics roll back the entire transaction.

---

## Authorization Matrix

| Contract | Function | Caller / Auth | Nonce / Replay Check |
|---|---|---|---|
| `credence_bond` | `create_bond_with_rolling` | `identity.require_auth()` | No (one-off creation) |
| `credence_bond` | `add_attestation` | `attester.require_auth()` | Yes (`nonce::consume_nonce`) |
| `credence_bond` | `add_attestation_batch` | Each `item.attester.require_auth()` | Yes (Per-item `nonce`) |
| `credence_bond` | `slash_bond` | `admin.require_auth()` | No (Admin/governance trigger) |
| `credence_bond` | `collect_fees` | `admin.require_auth()` | No |
| `credence_registry` | `register_identity` | `admin.require_auth()` | No |
| `credence_delegation` | `create_delegation` | `owner.require_auth()` | Yes (`nonce`) |
| `credence_treasury` | `propose_withdrawal` | `signer.require_auth()` | Yes (`nonce`) |
