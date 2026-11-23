# Event Patterns for Contributors

This guide explains the three event patterns used in the Credence contracts so new contributors can choose the right shape when they add or review an emitter.

## The short version

Use the event pattern that matches the semantic unit you are recording:

- Per-entity events describe the existence or current state of a contract-owned object.
- Per-transition events describe a change from one state to another.
- Per-request events describe an intent or request that may later be accepted, rejected, or fulfilled.

The repo already uses all three patterns in the bond contract, and the conventions are easiest to follow when you pick the pattern before naming the event.

## 1. Per-entity events

Choose a per-entity pattern when the event is mainly about the creation, registration, or durable existence of an object.

Typical characteristics:

- The event names a thing that now exists.
- The entity identity is the main subject.
- The payload usually carries the entity's current state or key fields.

Real examples:

- `bond_created_v2` from the bond creation flow documents that an identity now has a bond.
- `verifier_registered` documents that a verifier has been registered and can participate in later operations.

Why this pattern helps:

- It makes the lifecycle easy to reconstruct from the first moment an entity appears.
- It is a good fit for indexers that need to answer questions like "which bonds exist?" or "which verifiers are registered?"

## 2. Per-transition events

Choose a per-transition pattern when the event is mainly about movement from one state to another.

Typical characteristics:

- The event should make the transition obvious.
- The payload usually includes before/after values, or a clear old/new state pair.
- The event is a useful audit trail for state changes that happen during execution.

Real examples:

- `bond_withdrawn_v2` from the withdrawal flow records the amount moved and the remaining bond balance.
- `tier_changed_v2` records that the bond crossed a tier threshold and now occupies a different tier.

Why this pattern helps:

- It is the best fit for replay and state reconstruction because downstream consumers can replay transitions in order and recover the current state.
- It makes the event useful for indexers even when the contract state is not read again.

## 3. Per-request events

Choose a per-request pattern when the contract records intent, authorization, or a request that may later be processed.

Typical characteristics:

- The event is about a request being submitted rather than the final effect being applied.
- The payload captures the request context, timestamp, or target, but not necessarily the final end state.
- The request may later be accepted, rejected, or superseded.

Real examples:

- `withdrawal_requested` in the rolling-bond flow records that an identity requested a withdrawal.
- The request event is distinct from the later execution or renewal event because it documents the intent at the time it was made.

Why this pattern helps:

- It gives operators and support teams a clear record of what was requested, even if the subsequent transition is delayed or changed by later logic.
- It is useful for debugging workflows that span multiple ledger entries or multiple contract calls.

## How to decide which pattern to use

When you add a new event, ask one of these questions first:

1. Is this mainly about an object that now exists? Use per-entity.
2. Is this mainly about a state change from A to B? Use per-transition.
3. Is this mainly about an intent or request that occurred? Use per-request.

A practical rule is to name the event after the thing you want reviewers to see first. If the thing is the entity, use a creation or registration style. If the thing is the transition, use a change style. If the thing is the request, use a request style.

## Contributor conventions

The existing bond contract uses a few conventions that are worth keeping:

- Prefer past-tense verbs such as `created`, `withdrawn`, `requested`, or `registered`.
- Keep the entity identity in the event topics and the business details in the event data.
- For new indexer-facing events, prefer a `*_v2` variant when you need indexed timestamps, amounts, or other filterable fields.
- During migration periods, keep the older `v1` event in place when backwards compatibility matters.

## Example shape

A transition event usually answers the question "what changed?". A request event usually answers the question "what was asked for?".

```rust
e.events().publish(
    (
        Symbol::new(&e, "bond_withdrawn_v2"),
        identity.clone(),
        amount,
        remaining,
        now,
    ),
    (early_exit, penalty),
);
```

That shape is a transition event because it carries the change and the resulting balance in a way that is easy to replay.

## Related references

- [EVENTS.md](EVENTS.md) for the full emitted-event reference.
- [event-indexing.md](event-indexing.md) for replay and indexer guidance.
