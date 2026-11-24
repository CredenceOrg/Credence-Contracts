# Bond Event Catalog (Credence-Bond)

> Companion to [`EVENTS.md`](EVENTS.md). This catalog focuses on the
> `credence_bond` crate, with concrete emitter references, indexed-topic
> shapes, payload fields, and replay semantics.

The bond contract emits events from a few categories:

1. **Bond lifecycle** — creation, top-up, withdrawal, slashing, liquidation
2. **Tier transitions** — when a bond crosses a threshold
3. **Attestations** — add, batch add, revoke
4. **Attester management** — register / unregister
5. **Governance parameters** — every `set_*` in [`parameters.rs`](../../contracts/credence_bond/src/parameters.rs) emits `param_updated`
6. **Pause / Emergency** — see [`EVENTS.md`](EVENTS.md)
7. **Pull-payment claims** — `claim_added`, `claims_processed`, `claims_expired`
8. **Drift detection** — `bond_drift_detected` (informational, on invariant fail)
9. **Admin / Upgrade** — `admin_transferred`, upgrade-authorization events

All emitters live in
[`contracts/credence_bond/src/events.rs`](../../contracts/credence_bond/src/events.rs);
this document is the human-readable index used by off-chain indexers,
dashboards, and audit runners.

## Notation

- **Topics** are indexed for efficient filtering. The first topic is always a
  `Symbol` naming the event.
- **Data** is the unindexed payload.
- **Replay semantics** describe how an idempotent replayer mutates local state
  after consuming the event.
- Topic positions are 0-indexed; the name topic is at position 0.

## Quick reference

| Event                       | Indexed Topics (beyond name)              | Emitted by                                  |
| --------------------------- | ----------------------------------------- | ------------------------------------------- |
| `bond_created[_v2]`         | identity, amount, start_ts                | `create_bond`                               |
| `bond_increased[_v2]`       | identity, added_amount, new_total, ts     | `top_up`                                    |
| `bond_withdrawn[_v2]`       | identity, amount_withdrawn, remaining, ts | `withdraw`, `withdraw_early`                |
| `bond_slashed[_v2]`         | identity, slash_amount, total, ts, admin  | `slash_bond`                                |
| `early_exit_penalty`        | —                                         | `withdraw_early`                            |
| `early_exit_config_set`     | —                                         | `set_early_exit_config`                     |
| `bond_liquidated`           | identity                                  | `liquidate`                                 |
| `tier_changed[_v2]`         | identity                                  | any operation that crosses a tier boundary  |
| `attester_registered`       | —                                         | `register_attester`                         |
| `attester_unregistered`     | —                                         | `unregister_attester`                       |
| `attestation_added`         | subject                                   | `add_attestation`                           |
| `attestations_batch_added`  | subject                                   | `add_attestation_batch`                     |
| `attestation_revoked`       | subject                                   | `revoke_attestation`                        |
| `claim_added`               | recipient                                 | pull-payment creation (e.g. slash reward)   |
| `claims_processed`          | recipient                                 | `process_claims` / `process_claim_by_id`    |
| `claims_expired`            | recipient                                 | claim expiry sweep                          |
| `param_updated`             | key, category, admin                      | any governance `set_*`                      |
| `fee_config_updated`        | admin                                     | `set_fee_config`                            |
| `bond_drift_detected`       | subject                                   | post-write invariant drift detection        |
| `admin_transferred`         | —                                         | `transfer_admin`                            |
| `pause_*` / `unpaused`      | proposal_id / signer                      | see [`EVENTS.md`](EVENTS.md)                |

## Replay semantics

### `bond_created_v2`

Initialise the bond from `topics[1]=identity`, `topics[2]=amount`,
`topics[3]=start_ts`. Read duration, is_rolling, and end_ts from `data`.

- `bonded_amount = topics[2]`
- `bond_start = topics[3]`
- `bond_duration = data[0]`, `is_rolling = data[1]`, `bond_end_ts = data[2]`
- `slashed_amount = 0`, `active = true`

`notice_period_duration` is **not** carried — see
[`indexer-replay-contract.md`](indexer-replay-contract.md) for the
rolling-bond caveat.

### `bond_increased_v2`

- `bonded_amount = topics[3]` (absolute new total — replays must overwrite,
  never accumulate)
- `topics[2]` (added_amount) must equal `new_total − previous_total`
- If `data[0] = true`, the identity has crossed a tier boundary;
  `data[1]` is the new tier

### `bond_withdrawn_v2`

- `bonded_amount = topics[3]` (absolute remaining, not a delta)
- `data = (is_early, penalty_amount)` — early-exit info is informational;
  it does not alter the reconstructed balance
- A withdraw event does **not** flip `active = false`; full exits are
  signalled separately through `withdraw_bond` or `bond_liquidated`

### `bond_slashed_v2`

- `slashed_amount = topics[3]` (cumulative total, not the per-event delta)
- `topics[2]` (per-event delta) must equal `total − previous`
- Withdrawable balance is derived as `bonded_amount − slashed_amount`
- `topics[5]` is the admin that performed the slash
- `data[0]` is the reason; `data[1]` is a full-slash flag

### `bond_liquidated`

Finalise the bond and set `IdentityBond.active = false` plus
`DataKey::Liquidated(identity) = true`.

- `data[0]` is the residual swept to treasury (or 0 if fully slashed)
- `data[1]` is the reason symbol: `"fully_slashed"` or `"expired_unrenewed"`
- `data[2]` is the ledger timestamp
- `data[3]` is the admin / keeper that drove the liquidation
- Exactly one `bond_liquidated` per bond — `liquidate` is idempotent on an
  already-inactive bond so replayers can safely collapse duplicates

### `tier_changed_v2`

Informational. Reconstruct tier from the latest `bond_increased_v2` or
`bond_withdrawn_v2` event, or recompute from `bonded_amount` directly.
The data tuple `(old_tier, new_tier, timestamp)` exists for audit trails
only.

### `param_updated`

- `topics = (Symbol("param_updated"), key: Symbol, category: Symbol, admin: Address)`
- `data = (old_value: i128, new_value: i128)`
- Indexers can filter by `category` (`"fee"`, `"cooldown"`, `"tier"`, `"risk"`,
  `"borrow"`) for firehose subscription; `"key"` selects a single parameter

### `fee_config_updated`

Issue #1027 — emitted on every successful `set_fee_config` call. The event
intentionally carries **both** the treasury and `fee_bps` deltas since
`set_fee_config` updates both in a single call.

- `topics = (Symbol("fee_config_updated"), admin: Address)`
- `data = (old_treasury: Option<Address>, new_treasury: Address, old_fee_bps: u32, new_fee_bps: u32)`
- `old_treasury = None` ⇒ config was previously unset (initialization state)
- `old_fee_bps = 0` matches the same condition (defaults to 0)
- `new_fee_bps` is guaranteed to lie in `[MIN_FEE_BPS, MAX_FEE_BPS] =
  [0, 1_000]` (0%..10%) — see `fees.rs` for the bound constants
- Rejected (out-of-range) calls do **NOT** emit this event; storage remains
  unchanged
- A replayer that has tracked fee config from `fee_config_updated` MUST
  set `(treasury, fee_bps) = (data[1], data[3])`

### `claim_added`

- `topics = (Symbol("claim_added"), recipient: Address)`
- `data = (claim_type, amount, source_id)`

The claim's metadata, expiry, and processed flag live in storage and are
**not** carried in the event — replays must look them up by `source_id`
or scan `DataKey::ClaimById`.

### `claims_processed`

- `topics = (Symbol("claims_processed"), recipient: Address)`
- `data = (processed_count: u32, total_amount: i128, claim_types: Vec<ClaimType>)`

A replayer that has tracked claims from `claim_added` should mark each
matching `source_id` as paid once it sees `claims_processed`.

## Critical-flow event map

The table below records the events a live execution emits, in order. Tests in
[`contracts/credence_bond/src/test_events.rs`](../../contracts/credence_bond/src/test_events.rs)
and [`test_events_v2.rs`](../../contracts/credence_bond/src/test_events_v2.rs)
verify the ordering invariant for the flows marked with ✅.

| User flow                                       | Events (contract-scoped, in order)                          |
| ----------------------------------------------- | ----------------------------------------------------------- |
| `create_bond(amount, duration, is_rolling)` ✅  | `bond_created`, `bond_created_v2`, *(tier_changed_x2)*      |
| `top_up(amount)` ✅                             | `bond_increased`, `bond_increased_v2`, *(tier_changed_x2)*  |
| `withdraw(amount)` (post-lockup) ✅             | `bond_withdrawn`, `bond_withdrawn_v2`                       |
| `withdraw_early(amount)` ✅                     | `bond_withdrawn`, `bond_withdrawn_v2`, `early_exit_penalty` |
| `slash_bond(admin, amount)` ✅                  | `bond_slashed`, `bond_slashed_v2`, `claim_added` *(reward)* |
| `liquidate(admin)` ✅                           | `bond_liquidated`                                           |
| `set_*_protocol_parameter(...)`                | `param_updated`                                             |
| `set_fee_config(admin, treasury, fee_bps)` ✅  | `fee_config_updated` *(carries both old/new)*                |
| `add_attestation(...)`                          | `attestation_added`                                         |
| `add_attestation_batch(...)`                    | `attestations_batch_added`                                  |
| `revoke_attestation(...)`                       | `attestation_revoked`                                       |
| `register_attester(addr)` ✅                    | `attester_registered`                                       |
| `unregister_attester(addr)` ✅                  | `attester_unregistered`                                     |
| `transfer_admin(current, new)` ✅               | `admin_transferred`                                         |
| `add_pending_claim(...)` *(slash reward)* ✅    | `claim_added`                                               |
| `process_claims(user, ...)` ✅                  | `claims_processed`                                          |
| `set_early_exit_config(admin, ...)`             | `early_exit_config_set`                                     |

`*` denotes events only emitted when the predicate holds (tier boundary
crossed, slash reward > 0, etc.).

## Legacy v1 events

The following events do **not** carry the indexed v2 topics and should be
considered for migration to v2:

- `bond_created`, `bond_increased`, `bond_withdrawn`, `bond_slashed` —
  legacy `(Symbol, identity) → payload` shape
- `tier_changed` — old tier-change event without `old_tier`/`timestamp`

New integrations are encouraged to bind to the v2 variants directly. Legacy
events are emitted alongside v2 on every call for backward compatibility.

The tests in
[`contracts/credence_bond/src/test_events.rs`](../../contracts/credence_bond/src/test_events.rs)
pin down the legacy payload shape so that any future schema drift is caught
before it reaches production indexers.

## How to extend this catalog

New emitters must:

1. Be added to `contracts/credence_bond/src/events.rs` with a doc-comment
   block covering **topics**, **data**, and **replay semantics**.
2. Be mirrored in this catalog under "Quick reference" and "Critical-flow
   event map" if the flow is user-visible.
3. Be exercised by an assertion test under
   `contracts/credence_bond/src/test_events.rs` (v1) or
   `test_events_v2.rs` (v2).

## See also

- [`EVENTS.md`](EVENTS.md) — system-wide event spec (delegation, treasury, etc.)
- [`PATTERNS_EVENTS.md`](PATTERNS_EVENTS.md) — choosing between per-entity,
  per-transition, and per-request event shapes.
- [`indexer-replay-contract.md`](indexer-replay-contract.md) — replay rules
  and caveats for partially-evented flows.
- [`contracts/credence_bond/src/test_events.rs`](../../contracts/credence_bond/src/test_events.rs)
  — v1 event-payload assertions.
- [`contracts/credence_bond/src/test_events_v2.rs`](../../contracts/credence_bond/src/test_events_v2.rs)
  — v2 event-payload assertions including critical-flow runs.
- [`contracts/credence_bond/src/test_event_ordering.rs`](../../contracts/credence_bond/src/test_event_ordering.rs)
  — within-transaction event ordering and panic-safety tests.
- [`contracts/credence_bond/src/test_events_schema.rs`](../../contracts/credence_bond/src/test_events_schema.rs)
  — frozen-shape smoke tests that detect any add/remove of topics or data
  fields before merge.
