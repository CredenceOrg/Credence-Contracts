# Constructor Patterns

Audience: **contributors** adding a new Soroban contract to this workspace or reviewing an existing one.

Soroban has no native constructor; a contract's Wasm is deployed first and then initialised
in a separate transaction. This document records the one-shot initialisation pattern used
across every Credence contract so reviewers can verify behaviour against documented intent,
and new contributors do not need to read every commit to get it right.

See also:
- [DEPLOYMENT.md](DEPLOYMENT.md) — per-contract initialisation CLI commands and re-init safety table
- [STORAGE_KEYS.md](STORAGE_KEYS.md) — canonical naming convention for `DataKey` variants
- [access-control.md](access-control.md) — authority matrix for every entrypoint, including `initialize`
- [crates.md](crates.md) — workspace dependency graph

---

## The Canonical Pattern

Every deployable contract in this workspace exposes exactly one public `initialize` function.
The canonical form, taken from `contracts/templates/src/lib.rs`, is:

```rust
pub fn initialize(e: Env, admin: Address) {
    // 1. Re-init guard — must come first
    if e.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(&e, ContractError::AlreadyInitialized);
    }
    // 2. Admin authorisation
    admin.require_auth();
    // 3. Persist config to instance storage
    e.storage().instance().set(&DataKey::Admin, &admin);
    // 4. Emit an event so indexers can detect when a contract became live
    e.events().publish((Symbol::new(&e, "initialized"),), admin);
}
```

The four steps are ordered deliberately. Each one is explained below.

---

## Step 1 — Re-initialisation Guard

### Why

Soroban contracts have no constructor; `initialize` is a normal callable function.
Without a guard a second call overwrites the admin and all initial config, effectively
handing the contract to an attacker who front-runs the deployer.

### How

Check for any key that is unconditionally written during the first call.
`DataKey::Admin` is that key in every contract that has a single admin.

```rust
if e.storage().instance().has(&DataKey::Admin) {
    panic_with_error!(&e, ContractError::AlreadyInitialized);
}
```

The admin contract uses a separate sentinel key because it stores the admin inside
a richer `AdminInfo` struct rather than as a bare `Address`:

```rust
// contracts/admin/src/lib.rs
if e.storage().instance().has(&DataKey::Initialized) {
    panic_with_error!(&e, ContractError::AlreadyInitialized);
}
// ... later ...
e.storage().instance().set(&DataKey::Initialized, &true);
```

Both are acceptable. The important invariant is: **the guard key must be written
atomically with the rest of initialization, and the guard check must be the first
statement in the function body.**

### Known gaps

`credence_treasury` and `credence_multisig` currently have no re-init guard.
This is tracked in [DEPLOYMENT.md — Re-initialization Safety](DEPLOYMENT.md#re-initialization-safety).
When adding a re-init guard to those contracts, follow the pattern above.

---

## Step 2 — Admin Authorisation

### Why

`initialize` permanently sets the address that controls every subsequent admin-gated
entrypoint. If the function does not require authentication, a front-runner can call
it with their own address between the `deploy` and `initialize` transactions.

### How

```rust
admin.require_auth();
```

Call this after the re-init guard and before writing any state. Placing it after the
guard avoids charging auth overhead on the (cheap) rejection path when a contract is
already initialised.

### Argument-scoped auth (admin contract)

The admin contract uses `require_auth_for_args` to bind the authorization to the
specific arguments being passed, preventing an attacker from replaying an auth
for different parameter values:

```rust
// contracts/admin/src/lib.rs
super_admin
    .require_auth_for_args((super_admin.clone(), min_admins, max_admins).into_val(&e));
```

Use `require_auth_for_args` when the function accepts parameters that an attacker
could substitute while reusing the same auth token.

### Divergence to note

`timelock` does **not** call `require_auth` in its `initialize`. This means any
caller can set the admin without signing as that admin. This is a known deviation;
do not copy it in new contracts.

---

## Step 3 — Instance Storage and TTL Bump

### Why

All initialisation state must survive in instance storage, which is the ledger entry
shared by every invocation of the contract. Instance storage has a time-to-live (TTL)
that must be maintained; if it expires the contract becomes inaccessible.

### How

Bump the TTL as the very first statement, before any reads or writes, so that even a
rejected call extends the contract's reachable window:

```rust
bump_instance_ttl(&e);   // extends the instance entry's TTL
```

Then write every config key in a single uninterrupted sequence:

```rust
e.storage().instance().set(&DataKey::Admin, &admin);
e.storage().instance().set(&DataKey::Paused, &false);
e.storage().instance().set(&DataKey::PauseSignerCount, &0_u32);
e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
e.storage().instance().set(&DataKey::PauseProposalCounter, &0_u64);
```

Initialise every counter and flag your contract reads later, even to `0` or `false`.
Reading an uninitialised key will either return `None` (with `.get`) or panic (with
`.get_or_else(panic_with_error!)`), depending on the site. Setting a known-good
default at initialisation removes this ambiguity entirely.

### Storage tier

Use `instance()` for all config written during `initialize`. Do **not** use
`persistent()` or `temporary()` here; config must have the same TTL as the contract
instance itself and persistent storage is separately expirable.

---

## Step 4 — Initialization Event

### Why

Indexers and off-chain monitors need a reliable signal that a contract is live and
ready to accept calls. Emitting an event on initialization provides that signal
without requiring callers to poll `describe_config` or another view function.

### How

```rust
e.events().publish(
    (Symbol::new(&e, "initialized"),),
    admin.clone(),
);
```

The event topic is a single-element tuple containing the string `"initialized"`.
The data payload is the admin address so the event is self-describing and can be
cross-referenced with the deployer's transaction.

For contracts with richer init config (multisig, admin), include the full config
tuple in the payload:

```rust
// contracts/credence_multisig/src/multisig.rs
e.events().publish(
    (Symbol::new(&e, "multisig_initialized"),),
    (admin, signer_count, threshold),
);
```

See [EVENTS.md](EVENTS.md) for the canonical event catalog.

---

## Complete Example

The following is the complete, correct initialisation block for a new contract with a
single admin, a pause flag, and a per-record persistent store. It uses only
`soroban_sdk` primitives and satisfies `#![no_std]`.

```rust
#![no_std]
#![deny(clippy::float_arithmetic)]

use credence_errors::ContractError;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Helpers (defined in the workspace shared crate in practice)
// ---------------------------------------------------------------------------

fn bump_instance_ttl(e: &Env) {
    // 17_280 ledgers ≈ 24 hours at ~5 s/ledger; adjust to your contract's needs.
    const LEDGERS_TO_LIVE: u32 = 17_280;
    e.storage()
        .instance()
        .extend_ttl(LEDGERS_TO_LIVE, LEDGERS_TO_LIVE);
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MyContract;

#[contractimpl]
impl MyContract {
    /// One-shot initialisation. Panics with `AlreadyInitialized` if called more than once.
    pub fn initialize(e: Env, admin: Address) {
        bump_instance_ttl(&e);

        // Step 1 — re-init guard
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, ContractError::AlreadyInitialized);
        }

        // Step 2 — admin authorisation
        admin.require_auth();

        // Step 3 — persist config
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);

        // Step 4 — initialization event
        e.events()
            .publish((Symbol::new(&e, "initialized"),), admin);
    }
}
```

---

## Testing the Initialize Entrypoint

Every contract must have at minimum two tests for its `initialize` function:
a happy path that verifies the admin is persisted, and a sad path that verifies
double-initialization is rejected.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    // Reusable setup helper shared across the test module.
    fn setup() -> (Env, Address, MyContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(MyContract, ());
        let client = MyContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, admin, client)
    }

    /// Happy path: initialisation succeeds and admin is stored.
    #[test]
    fn initialize_stores_admin() {
        let (_env, admin, client) = setup();
        // Verify via a view function or a gated call that confirms admin identity.
        // e.g.: assert_eq!(client.get_admin(), admin);
        let _ = (admin, client); // use them to silence unused-variable lint
    }

    /// Sad path: second call must be rejected.
    #[test]
    #[should_panic(expected = "AlreadyInitialized")]
    fn initialize_rejects_double_init() {
        let (env, _admin, client) = setup();
        env.mock_all_auths();
        let attacker = Address::generate(&env);
        // The guard fires before auth is checked, so no mock_all_auths is needed
        // for the second call, but it is harmless to include one.
        client.initialize(&attacker);
    }
}
```

`env.mock_all_auths()` must be called before `register` and before each subsequent
invocation that requires auth. The `Address::generate` helper produces a unique
address from the test environment's RNG; never hard-code addresses in tests.

---

## Checklist for New Contracts

Copy this list into your PR description and tick each box before requesting review:

- [ ] `initialize` is the only function that sets foundational config (admin, token, etc.)
- [ ] Re-init guard (`has(&DataKey::Admin)` or equivalent) is the first statement
- [ ] `admin.require_auth()` (or `require_auth_for_args`) is called before any `set`
- [ ] All counters and flags are written to `instance()` storage during `initialize`
- [ ] `bump_instance_ttl` is called at the top of `initialize`
- [ ] An `"initialized"` event is emitted with the admin address (or full config tuple)
- [ ] Two tests exist: happy-path and double-init rejection
- [ ] `#![no_std]` — no `std::` imports anywhere in the new crate
- [ ] The contract is listed in the re-init safety table in [DEPLOYMENT.md](DEPLOYMENT.md)

---

## Cross-references

| Topic | Document |
|---|---|
| CLI commands to invoke `initialize` on each contract | [DEPLOYMENT.md](DEPLOYMENT.md) |
| `DataKey` naming rules | [STORAGE_KEYS.md](STORAGE_KEYS.md) |
| Event schema and topic conventions | [EVENTS.md](EVENTS.md) |
| Auth tree and entrypoint authority matrix | [access-control.md](access-control.md) |
| Contract upgrade flow (post-initialization) | [UPGRADE.md](UPGRADE.md) |
| Starting template for new contracts | [`contracts/templates/src/lib.rs`](../contracts/templates/src/lib.rs) |
