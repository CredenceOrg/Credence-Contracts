# Lease Model

This document describes lease semantics — issuance shape, scope, renewal, and
expiration — for Credence Soroban contracts.

---

## Audience: Contributor

Written for **contributors** implementing or auditing lease-gated entrypoints.
It records the intended hard rules so reviewers can check behaviour against
documented intent without reconstructing tribal knowledge from commits.

---

## What a lease is

A lease is a time-bounded, scoped authorization grant. It answers three
questions before a privileged op may proceed:

1. **Who?** — which `signer` may exercise it
2. **What?** — which operation bits (`scope`) are covered
3. **Until when?** — exclusive `expires_at` upper bound

The shared type lives in `credence_errors`:

```rust
use credence_errors::{lease_op, Lease};
use soroban_sdk::{Address, Env};

fn example_lease(e: &Env, signer: Address) -> Lease {
    Lease {
        signer,
        // Read + write, but not transfer.
        scope: lease_op::READ | lease_op::WRITE,
        // Hard cliff: valid while now < expires_at.
        expires_at: e.ledger().timestamp().saturating_add(86_400),
    }
}
```

### Scope bits (`lease_op`)

| Constant | Bit | Meaning |
| :--- | :--- | :--- |
| `lease_op::READ` | `1 << 0` | Read-only queries / views |
| `lease_op::WRITE` | `1 << 1` | State-mutating writes |
| `lease_op::RENEW` | `1 << 2` | Extend / renew the lease window |
| `lease_op::TRANSFER` | `1 << 3` | Re-assign signer / transfer custody |
| `lease_op::ALL` | union | Convenience mask for every defined op |

Scopes are bitmasks. A lease may grant any combination via `|`.

---

## Guards (defence-in-depth)

Call these **before** mutating storage or transferring value. They panic with
typed `ContractError` variants (not string panics).

### Scope must cover the op

```rust
use credence_errors::{require_matching_lease_scope, lease_op, Lease, ContractError};
use soroban_sdk::Env;

fn gated_write(e: &Env, lease: &Lease) {
    // Panics with ContractError::LeaseScopeMismatch (120) if WRITE is absent.
    require_matching_lease_scope(e, lease, lease_op::WRITE);
    // ... proceed with write ...
}
```

**Rule:** `(lease.scope & op) == op`. Partial overlap is rejected.

**Threat if missing:** a `READ`-only lease can be reused to drive `WRITE` /
`TRANSFER` when callers only check signer identity.

### Lease must not be expired

```rust
use credence_errors::{require_no_expired_lease, Lease};
use soroban_sdk::Env;

fn gated_op(e: &Env, lease: &Lease) {
    // Panics with ContractError::LeaseExpired (121) when now >= expires_at.
    require_no_expired_lease(e, lease);
    // ... proceed ...
}
```

**Rule (hard cliff):** `now < lease.expires_at` is accepted; `now == expires_at`
and `now > expires_at` are rejected. There is no grace window on this helper.

**Threat if missing:** a previously valid lease can be replayed after expiry.

---

## Lifecycle: issue → use → renew → expire

```text
                 issue(scope, expires_at)
                          │
                          ▼
                   ┌──────────────┐
          use ────►│   Active     │◄──── renew (extends expires_at;
                   │ now < exp    │      typically requires RENEW bit)
                   └──────┬───────┘
                          │ clock advances to expires_at
                          ▼
                   ┌──────────────┐
                   │   Expired    │  require_no_expired_lease rejects
                   │ now >= exp   │
                   └──────────────┘
```

### Issuance

- Choose the **minimum** scope needed for the intended workflow.
- Set `expires_at` strictly in the future relative to the issuing ledger.
- Bind `signer` to the identity that will authorize later calls.

### Use

On every gated entrypoint, check in this order (fail fast):

1. `require_no_expired_lease(e, &lease)`
2. `require_matching_lease_scope(e, &lease, op)`
3. Any signer / auth checks specific to the contract

### Renewal

Renewal is a normal op that updates `expires_at` (and optionally `scope`) under
an existing live lease. Recommended pattern:

1. Require the current lease is not expired.
2. Require `lease_op::RENEW` (or `ALL`) in scope.
3. Write the new `expires_at` with saturating arithmetic.
4. Emit a renewal event for indexers.

Renewal **does not** revive an already-expired lease through
`require_no_expired_lease` — issue a new lease instead.

### Expiration

Expiration is evaluated against `e.ledger().timestamp()`. Contracts must not
rely on wall-clock or off-chain clocks. In tests, advance time with:

```rust
use soroban_sdk::{testutils::Ledger, Env};

fn advance(e: &Env, ts: u64) {
    e.ledger().with_mut(|li| {
        li.timestamp = ts;
    });
}
```

Boundary cases locked by tests:

| State | Condition | `require_no_expired_lease` |
| :--- | :--- | :--- |
| Fresh | `expires_at = now + 1 day` | allows |
| Expiring soon | `expires_at = now + 1` | allows |
| Expired (boundary) | `expires_at = now` | rejects |
| Expired (past) | `expires_at < now` | rejects |

---

## Error codes

| Variant | Code | Recoverable? | Meaning |
| :--- | ---: | :---: | :--- |
| `LeaseScopeMismatch` | 120 | no | Scope bits do not cover `op` |
| `LeaseExpired` | 121 | no | `now >= expires_at` |

Wire codes are stable — do not renumber after deployment.

---

## Related docs

- [TIME_UNITS.md](TIME_UNITS.md) — seconds / ledger timestamps / test time travel
- [storage-ttl.md](storage-ttl.md) — Soroban storage TTL (distinct from lease expiry)
- [expiry-boundaries.md](expiry-boundaries.md) — delegation expiry window rules
- [security.md](security.md) — trust assumptions and defence-in-depth posture
