# Sanctioned API Clients

**Audience:** Downstream Integrators

This document lists the officially supported client libraries for interacting with Credence smart contracts. These clients provide generated types, pre-configured RPC clients, and helper functions that simplify integration.

Using a sanctioned client is the recommended path for building front-ends, backends, or indexers. They are kept in sync with contract deployments and follow a predictable versioning scheme.

---

## 1. TypeScript / JavaScript Client (`@credence/client`)

The primary client for web front-ends, Node.js backends, and scripting.

| | |
|-|-|
| **Recommended Version** | `^1.2.0` |
| **Source** | `npm install @credence/client` |
| **Repository** | `https://github.com/CredenceOrg/credence-js` |

### Scope

- Full type safety with generated TypeScript types for all contract entrypoints, arguments, and return values.
- Pre-configured clients for `credence_bond`, `credence_delegation`, `credence_registry`, and other core contracts.
- Helpers for constructing signed payloads for relayed transactions (see `DELEGATION_HANDBOOK.md`).
- Event parsing utilities.

### Example: Querying the Bond Contract

```typescript
import { CredenceBondClient } from '@credence/client';
import { SorobanRpc } from '@stellar/stellar-sdk';

const RPC_URL = 'https://soroban-testnet.stellar.org';
const BOND_CONTRACT_ID = 'CD...7Z'; // Deployed bond contract ID

async function getBondState(identity: string) {
  const rpc = new SorobanRpc.Server(RPC_URL, { allowHttp: true });
  const client = new CredenceBondClient({
    rpc,
    contractId: BOND_CONTRACT_ID,
  });

  try {
    const bondState = await client.get_identity_state({ identity });
    console.log('Bond State:', bondState);
    return bondState;
  } catch (error) {
    console.error('Failed to fetch bond state:', error);
  }
}
```

---

## 2. Rust Client (`credence-client-rs`)

The recommended client for Rust-based services, such as relayers, indexers, or other on-chain Soroban contracts that need to call Credence contracts.

| | |
|-|-|
| **Recommended Version** | `^0.5.1` |
| **Source** | `cargo add credence-client-rs` |
| **Repository** | `https://github.com/CredenceOrg/Credence-Contracts` (part of this workspace) |

### Scope

- Provides generated client types that wrap the low-level `soroban_sdk::Env` calls.
- Exposes the same public API as the on-chain contracts.
- Includes shared types like `AdminRole`, `AdminInfo`, and `ContractError` for easy integration.

### Example: Checking a Delegation in another Contract

```rust
use soroban_sdk::{contract, contractimpl, Address, Env};
use credence_client_rs::{CredenceDelegationClient, DelegationType};

#[contract]
pub struct MyOtherContract;

#[contractimpl]
impl MyOtherContract {
    pub fn execute_delegated_action(
        e: Env,
        delegation_contract_id: Address,
        owner: Address,
        delegate: Address,
    ) {
        // Use the client to perform a cross-contract call
        let client = CredenceDelegationClient::new(&e, &delegation_contract_id);

        // This call will panic with a `ContractError` if the delegation is not active
        client.check_delegation_active(&owner, &delegate, &DelegationType::Management);

        // ... proceed with action now that delegate authority is confirmed ...
    }
}
```

---

## 3. Versioning and Stability

Both client libraries follow Semantic Versioning (SemVer).

- **MAJOR** version bumps indicate a breaking change in the contract's public API that requires client-side updates.
- **MINOR** version bumps introduce new, backward-compatible functionality (e.g., new view functions, new events).
- **PATCH** version bumps include bug fixes or documentation updates that do not change behavior.

Always check the `CHANGELOG.md` in the respective client repository before upgrading to a new major version.

---

## 4. Direct Contract Interaction

If you choose not to use a client library, you can interact with the contracts directly using the `soroban-cli` or by constructing raw Soroban RPC requests.

For this path, the following documents are your source of truth:

- **`docs/DEPLOYMENT.md`**: Contract deployment and initialization instructions.
- **`docs/EVENTS.md`**: The canonical catalog of all events emitted by the contracts.
- **`docs/error-codes-wire.md`**: A guide to the wire-stable error codes.
- **Per-contract docs**: `DELEGATION_HANDBOOK.md`, `BOND_ISSUANCE.md`, etc., for entrypoint-specific details.
