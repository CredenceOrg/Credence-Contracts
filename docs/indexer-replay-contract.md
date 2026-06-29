# Indexer Replay Contract

Downstream indexers never read contract storage. They observe only the **event
stream** and must reconstruct each bond's `IdentityBond` from it. This document
specifies the contract between the `credence_bond` events and a conforming
replayer, and is enforced by the test suite in
[`contracts/credence_bond/tests/indexer_replay.rs`](../contracts/credence_bond/tests/indexer_replay.rs).

> **The replay invariant:** folding a pure `apply(event, state)` over a bond's
> complete event history, in emission order, MUST yield exactly the
> `IdentityBond` stored on-chain. A divergence means the contract is missing an
> event or emitting a mismatched payload — a silent break for every indexer.

## The replayer

The replayer is a pure function with no clock, storage, or `Env` access:

```text
apply(state: Option<IdentityBond>, event) -> Option<IdentityBond>
```

It starts at `None`, is initialized by the genesis event, and is mutated by each
subsequent event. Purity is the point: it is exactly what an off-chain indexer
can run against a stream pulled from RPC.

## Authoritative events

Each lifecycle event carries **absolute** post-state values (not just deltas), so
replay is order-sensitive but not arithmetic-sensitive: a replayer assigns the
carried value rather than recomputing it.

| Event (topic 0)      | Reconstruction effect                                              | Source |
|----------------------|--------------------------------------------------------------------|--------|
| `bond_created_v2`    | **Genesis.** Init `identity`, `bonded_amount=amount`, `bond_start`, `bond_duration`, `is_rolling`; `slashed_amount=0`, `active=true`. | `create_bond` |
| `bond_increased_v2`  | `bonded_amount = new_total`                                         | `top_up` |
| `bond_withdrawn_v2`  | `bonded_amount = remaining`                                         | `withdraw`, `withdraw_early` |
| `bond_slashed_v2`    | `slashed_amount = total_slashed`                                   | `slash_bond` |

Payload field positions are documented on each emitter in
[`src/events.rs`](../contracts/credence_bond/src/events.rs) under
`# Replay semantics`.

## Informational / ignored events

These appear in the stream but must **not** drive reconstruction. A conforming
replayer skips any topic it does not model:

- `tier_changed` — derived purely from `bonded_amount`; recompute, never replay.
- `bond_created` / `bond_increased` / `bond_withdrawn` / `bond_slashed` (the
  legacy non-`_v2` variants) — superseded by the `_v2` events that carry indexed
  topics. A v2 replayer ignores them to avoid double-counting.
- `attester_registered`, `claim_added`, `param_updated`, upgrade/admin-transfer
  events, etc. — orthogonal to bond financial state.

## Known gaps (deliberately uncovered)

The current event schema does not let an indexer rebuild **every** field of a
rolling bond:

- `notice_period_duration` is not carried by `bond_created_v2`.
- `withdrawal_requested_at` is only partially evented (`withdrawal_requested`,
  `bond_renewed`).

For **non-rolling** bonds both fields are always `0`, so full-struct equality is
exact — which is why the test scenarios use non-rolling bonds. Extending coverage
to rolling bonds requires adding those fields to the emitted payloads first; that
is tracked as a separate schema change, not papered over in the replayer.

## Triage when a replay test fails

1. **Reconstructed state is empty (`None`).** The genesis `bond_created_v2` event
   was not emitted by `create_bond`. Wire the emitter.
2. **A balance field is off by one operation.** An entrypoint mutated storage but
   did not emit its event (e.g. `top_up` without `bond_increased_v2`). The
   negative control `dropping_topup_event_diverges` demonstrates exactly this
   failure mode.
3. **A field diverges that no event carries** (e.g. `notice_period_duration` on a
   rolling bond). This is the known-gap above — fix the schema, not the test.
4. **Payload positions mismatch.** The decoder in the test reads fields by topic
   index; if an emitter reorders its topics, update both the emitter doc and the
   decoder together — they are the two halves of this contract.

## Adding a new bond-mutating entrypoint

1. Emit a `_v2` event whose topics carry the **absolute** resulting state.
2. Document its `# Replay semantics` on the emitter.
3. Add a `BondEvent` variant + decode arm + `apply` arm in the test.
4. Add an end-to-end scenario that exercises it and asserts the replay invariant.
