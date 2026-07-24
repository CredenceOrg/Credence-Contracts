# Ledger Time Handbook

How ledger time relates to wall time across the Credence contracts: which clock
each mechanism uses, what the comparison semantics are, and how tests control
time. Read this before touching any expiry, cooldown, timelock, epoch, or TTL
code path.

Audience: contributors.

---

## The two clocks

Soroban exposes two distinct notions of time via `Env::ledger()`:

| Accessor | Type | Meaning | Credence usage |
| --- | --- | --- | --- |
| `e.ledger().timestamp()` | `u64` | Ledger close time, in Unix seconds. Agreed by validators through SCP; close to wall time but not a precise clock. | All human-meaningful deadlines: delegation expiry, cooldown windows, admin suspensions, timelock ETA/expiry, recorded-at fields. |
| `e.ledger().sequence()` | `u32` | Ledger sequence number. Strictly increasing, one ledger per ~5 seconds on average (not guaranteed). | Ordering and rate windows that must be immune to timestamp manipulation: proposal epochs (`pausable.rs`), storage TTL thresholds. |

Rule of thumb: **deadlines humans reason about use `timestamp()`; anything that
must advance at a steady, manipulation-resistant cadence uses `sequence()`.**

### Why not wall time for everything

`timestamp()` is the ledger close time chosen during consensus. It tracks wall
time loosely and can deviate by seconds to minutes. That is acceptable for
day-scale deadlines (a 365-day delegation cap, a 24-hour cooldown) but never
assume:

- sub-minute precision,
- that two consecutive ledgers have distinct timestamps,
- that `timestamp()` and the submitting client's wall clock agree.

Conversely, `sequence()` advances even if validators stall timestamp
negotiation, which is why epoch-style windows derive from it.

---

## Where each mechanism lives

### Delegation expiry — `credence_delegation` (timestamp)

Invariant enforced at creation (see `docs/expiry-boundaries.md`):

```
now < expires_at <= now + MAX_DELEGATION_DURATION
```

- `now = e.ledger().timestamp()`, captured **once** at function entry
  (`contracts/credence_delegation/src/lib.rs` — "The ledger timestamp is
  captured once at function entry") and reused for every comparison in the
  call. Never re-read the clock mid-call; a mid-call ledger advance would make
  the lower-bound check and the upper-bound check disagree.
- Lower bound is strict (`expires_at <= now` panics with `ExpiryInPast`):
  zero-duration and already-expired delegations are invalid.
- Upper bound uses `saturating_add(MAX_DELEGATION_DURATION)` so the check
  cannot overflow at extreme ledger times.
- Liveness (`is_active`) is `!d.revoked && d.expires_at > timestamp()`:
  a delegation is dead **at** its expiry second, not after it.

### Cooldown withdrawals — `credence_bond` (timestamp)

`docs/cooldown.md`: `CooldownRequest.requested_at` stores `timestamp()` at
request time; execution requires
`current_time >= requested_at + cooldown_period`. The comparison is inclusive:
at exactly `requested_at + period` the withdrawal is executable.

### Admin suspension — `admin` (timestamp)

`suspended_until` is a `timestamp()` deadline. While
`timestamp() < suspended_until` the admin is treated as suspended; at
`timestamp() >= suspended_until` the suspension is over
(`contracts/admin/src/lib.rs:637`, `:899`). `suspend_admin` requires
`until_ts > timestamp()` and panics `AdminSuspended` otherwise, so there is no
way to schedule a past deadline; the suspension simply ends when the deadline
passes.

### Timelock operations — `timelock` (timestamp)

Operations carry both an `eta` (earliest execution time) and an `expires_at`
(latest execution time), both compared against `timestamp()`:

- execution before `eta` is rejected,
- execution at `timestamp() > expires_at` is rejected (the operation has
  lapsed; the boundary tests execute successfully at
  `li.timestamp = op.expires_at` and reject at `op.expires_at + 1`).

The valid execution window is therefore `eta <= timestamp() <= expires_at`
(`expires_at` is inclusive).

### Proposal epochs — `credence_delegation/src/pausable.rs` (sequence)

```rust
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;
// epoch = ledger_sequence / PROPOSAL_EPOCH_SIZE
```

Proposals are bucketed into epochs of 100 ledgers (~8 minutes at the 5-second
target). Sequence-based epochs cannot be gamed by nudging close-time
timestamps and stay well-defined even if wall time drifts.

### Recorded-at fields (timestamp, informational only)

`assigned_at` (`admin`), `proposed_at` (`credence_multisig`), `updated_at`
(`templates`) record when something happened for audit/display. These fields
are never used for authorization decisions — deadlines always compare against
a freshly read `timestamp()` at decision time.

### Storage TTL (sequence-denominated)

`docs/storage-ttl.md`: `extend_ttl(threshold, extend_to)` parameters are
measured in **ledgers** (sequence numbers), not seconds. `STORAGE_TTL_EXTEND_TO`
and `PERSISTENT_TTL_MAX` are ledger counts; treat the "~N months" comments as
estimates that assume the ~5 s/ledger average. If you need "at least one year
of retention", reason in ledgers and convert with the target ledger close
interval, not with wall seconds.

---

## Comparison-semantics cheat sheet

| Mechanism | Boundary that is **invalid** | Boundary that is **valid** |
| --- | --- | --- |
| Delegation creation | `expires_at <= now` | `expires_at == now + 1` |
| Delegation liveness | `timestamp() >= expires_at` | `timestamp() == expires_at - 1` |
| Cooldown execution | `timestamp() < requested_at + period` | `timestamp() == requested_at + period` |
| Admin suspension active | — | active while `timestamp() < suspended_until` |
| Timelock execution | `timestamp() < eta`, `timestamp() > expires_at` | `timestamp() == eta`, `timestamp() == expires_at` |
| Multisig proposal expiry | `timestamp() >= expires_at` | `timestamp() == expires_at - 1` |

When adding a new time gate, pick the inclusive/exclusive sides deliberately
and document them in this table. Off-by-one at an expiry boundary is the most
common time bug class in this repo's review history.

---

## Testing time

Tests control both clocks through the ledger test handle:

```rust
env.ledger().with_mut(|li| li.timestamp = op.eta);      // jump close time
env.ledger().with_mut(|li| li.sequence_number = 500);   // jump sequence
```

Patterns used in the suite (e.g. `timelock/src/lib.rs` tests):

- set the clock to exactly the boundary (`eta`, `expires_at`,
  `expires_at + 1`) and assert accept/reject on each side;
- capture `now` once in the test and derive all offsets from it, mirroring the
  production "capture once at entry" rule;
- for epoch logic, set `sequence_number` to multiples of
  `PROPOSAL_EPOCH_SIZE` plus/minus one to cover epoch rollover.

Do not write tests that sleep or depend on the host wall clock — all time
must come from the mocked ledger.

---

## Hardening checklist for new time-gated code

- [ ] Read `e.ledger().timestamp()` once at entry; pass the value down.
- [ ] Use `saturating_add`/`saturating_sub` for deadline arithmetic.
- [ ] State the inclusive/exclusive boundary in the doc comment and add
      boundary-exact tests (value, value + 1, value - 1).
- [ ] Use `sequence()` (not `timestamp()`) for ordering, idempotency windows,
      and TTL math.
- [ ] Never persist "now" and use it later for authorization; re-read at
      decision time.
- [ ] Reject past deadlines at input validation, not at first use.

---

## References

- `docs/expiry-boundaries.md` — delegation expiry boundary design and tests
- `docs/cooldown.md` — cooldown window mechanics
- `docs/credence-timelock.md`, `docs/timelock.md` — timelock operation model
- `docs/storage-ttl.md` — TTL constants and bump policy
- Soroban ledger semantics: <https://developers.stellar.org/docs/learn/fundamentals/stellar-data-structures/ledgers>
