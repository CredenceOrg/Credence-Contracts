# Lease Scopes

Available scopes and their granted permissions.

---

## Audience: Contributor & Integrator

Written for **contributors** implementing lease-gated entrypoints and
**integrators** building off-chain tooling around the Credence delegation
system. It records every scope bit, what each one authorises, and how to
combine them safely.

---

## Overview

A lease scope is a `u32` bitmask that answers the question **"what may the
signer do?"**.  Every lease carries a `scope` field — a union of one or more
[`lease_op`] bits — that explicitly lists which operations the lease covers.

Without scoping, a lease that authorises a read-only query could be replayed
against a state-mutating entrypoint.  Scopes let the issuer grant the
**minimum** privilege needed for a given workflow.

The shared type lives in `credence_errors`:

```rust
use credence_errors::{lease_op, Lease};
use soroban_sdk::{Address, Env};

fn example_scoped_lease(e: &Env, signer: Address) -> Lease {
    Lease {
        signer,
        scope: lease_op::READ | lease_op::WRITE,
        expires_at: e.ledger().timestamp().saturating_add(86_400),
    }
}
```

---

## Scope bits

| Constant               | Bit       | Hex       | Meaning                                                                 |
| :--------------------- | :-------- | :-------- | :---------------------------------------------------------------------- |
| `lease_op::READ`       | `1 << 0`  | `0x1`     | **Read-only** queries / views / balance checks                          |
| `lease_op::WRITE`      | `1 << 1`  | `0x2`     | **State-mutating** writes (create, top-up, transfer-in)                 |
| `lease_op::RENEW`      | `1 << 2`  | `0x4`     | **Extend / renew** an existing lease or bond window                     |
| `lease_op::TRANSFER`   | `1 << 3`  | `0x8`     | **Transfer** custody or re-assign the lease signer                      |
| `lease_op::ALL`        | —         | `0xF`     | **Convenience mask** granting every defined op (READ \| WRITE \| RENEW \| TRANSFER) |

Scopes are **bitmasks**.  A lease may grant any combination via `|`:

```rust
// Read + write, but not renew or transfer.
let scope = lease_op::READ | lease_op::WRITE;   // 0x3

// Full administrative access.
let scope = lease_op::ALL;                       // 0xF

// Read-only access.
let scope = lease_op::READ;                      // 0x1
```

---

## Guard functions

The `credence_errors::lease` module provides two defence-in-depth guards:

### `require_matching_lease_scope(e, lease, op)`

Panics with [`ContractError::LeaseScopeMismatch`] (code **121**) when the
lease's scope does not cover **every** bit in `op`.

**Rule:** `(lease.scope & op) == op`.  Partial overlap is rejected.

```rust
use credence_errors::{require_matching_lease_scope, lease_op, Lease};
use soroban_sdk::Env;

fn gated_write(e: &Env, lease: &Lease) {
    require_matching_lease_scope(e, lease, lease_op::WRITE);
    // Proceed with the write — scope covers WRITE.
}
```

**Threat mitigated:** a `READ`-only lease being replayed to drive `WRITE` or
`TRANSFER` when callers only verify the signer identity.

### `require_no_expired_lease(e, lease)`

Panics with [`ContractError::LeaseExpired`] (code **122**) when
`now >= lease.expires_at`.

**Rule (hard cliff):** `now < expires_at` is accepted; equality is rejected.

```rust
use credence_errors::{require_no_expired_lease, Lease};
use soroban_sdk::Env;

fn time_gated_op(e: &Env, lease: &Lease) {
    require_no_expired_lease(e, lease);
    // Proceed — lease is not expired.
}
```

---

## Recommended check order

On every gated entrypoint, check in this order (fail fast):

1. `require_no_expired_lease(e, &lease)` — enforce time bounds first
2. `require_matching_lease_scope(e, &lease, op)` — then check scope
3. Any signer / auth checks specific to the contract

Checking expiry before scope is deliberate: an expired lease has no
authority at all, regardless of its scope bits.

---

## Scope combinations and use cases

| Combination         | Value  | Typical use case                                                  |
| :------------------ | :----- | :---------------------------------------------------------------- |
| `READ`              | `0x1`  | Public dashboards, indexers, read-only monitoring                 |
| `READ \| WRITE`     | `0x3`  | Bond creation, attestations, standard operator actions            |
| `READ \| WRITE \| RENEW` | `0x7` | Operator that can extend bond/delegation windows               |
| `ALL`               | `0xF`  | Administrative leases, treasury operations, full custody         |
| `RENEW`             | `0x4`  | Dedicated renewal bots that should not mutate state               |
| `READ \| RENEW`     | `0x5`  | Indexers that can also bump expiry on stale entries               |

---

## Principle of least privilege

When issuing a lease:

- Grant the **minimum** scope needed for the intended workflow.
- Prefer `READ | WRITE` over `ALL` unless transfer/renewal is genuinely
  required.
- Narrower scopes reduce the blast radius of a compromised signer key.

---

## Error codes

| Variant              | Code | Recoverable | Meaning                                 |
| :------------------- | ---: | :---------: | :-------------------------------------- |
| `LeaseScopeMismatch` | 121  | no          | Scope bits do not cover the requested op |
| `LeaseExpired`       | 122  | no          | `now >= lease.expires_at`               |
| `LeaseSignerMismatch`| 126  | no          | `lease.signer != calling actor`         |

Wire codes are stable — do not renumber after deployment.

---

## Related docs

- [LEASE_MODEL.md](LEASE_MODEL.md) — lease lifecycle, issuance, renewal, and
  expiration semantics
- [LEASE_SIGNATURES.md](LEASE_SIGNATURES.md) — signature format for relayed
  delegation payloads
- [DELEGATION_HANDBOOK.md](DELEGATION_HANDBOOK.md) — end-to-end relayed-action
  flow and nonce model
- [access-control.md](access-control.md) — Credence access control module and
  role hierarchy
- [TIME_UNITS.md](TIME_UNITS.md) — seconds / ledger timestamps / test time
  travel
- [security.md](security.md) — trust assumptions and defence-in-depth posture
