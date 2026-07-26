# Event Schema Stability & Version Bump Discipline

This document defines the stability guarantees for Credence contract event schemas and the discipline required when introducing event version changes. It is intended for contributors modifying contract event emissions.

## Audience

**Contributors**: Engineers modifying contract event emissions. If you are adding, removing, or changing any event in the Credence contracts, follow this discipline.

## Stability Guarantees

### Non-Negotiable Guarantees

Once an event is emitted in production, the following guarantees apply:

1. **Topic Position Stability**: The position of each topic in an event's topic array never changes. If `bond_created` has `identity` at topic position 1, it remains at position 1 forever.

2. **Topic Type Stability**: The Soroban type of each topic never changes. An `Address` topic never becomes a `Symbol` or `i128`.

3. **Data Position Stability**: The position of each data field in an event's data payload never changes.

4. **Data Type Stability**: The Soroban type of each data field never changes.

5. **Event Name Uniqueness**: Event names (the Symbol at topic position 0) are never reused for different semantics. A `bond_created` event always means bond creation.

### What These Guarantees Mean

**Indexers rely on these guarantees to parse events without breaking.** If you violate any of these guarantees, existing indexers will fail to parse events correctly, potentially causing:

- Incorrect state reconstruction
- Missing or corrupted analytics data
- Failed transaction monitoring
- Broken user-facing dashboards

## Version Bump Discipline

### When to Bump Event Versions

You MUST create a new event version (e.g., `bond_created_v2`) when:

1. **Adding a new indexed field**: Adding a topic that should be filterable at the ledger level
2. **Changing field semantics**: A field's meaning changes in a way that breaks existing parsers
3. **Restructuring data**: Reordering or grouping data fields for better organization
4. **Adding critical context**: Including additional fields that are essential for proper interpretation

You MAY create a new event version when:

1. **Adding non-critical data fields**: Adding optional data fields that enhance but aren't required for basic parsing
2. **Performance optimization**: Restructuring for better indexing efficiency (with backward compatibility)

You MUST NOT create a new event version when:

1. **Fixing typos in event names**: This breaks existing indexers
2. **Changing field order**: This breaks positional parsing
3. **Removing fields**: This breaks parsers expecting those fields
4. **Changing types**: This breaks type-dependent parsing

### Version Bump Procedure

#### Step 1: Assess Impact

Before bumping an event version, answer these questions:

- **Which indexers consume this event?** Check with the backend team.
- **What is the migration path for existing indexers?** Plan for dual emission.
- **Is this change necessary?** Can the goal be achieved without breaking changes?

#### Step 2: Design the New Version

Design the new event version following these principles:

1. **Preserve all existing fields**: The new version must include all fields from the old version
2. **Add new fields at the end**: Append new topics/data fields, don't insert in the middle
3. **Use descriptive names**: Include `_v2`, `_v3` suffixes for clarity
4. **Document the delta**: Clearly explain what changed and why

**Example: Adding an indexed timestamp field**

```rust
// OLD VERSION (v1)
e.events().publish(
    (Symbol::new(&e, "bond_created"),),
    (identity, amount, duration),  // timestamp in data, not indexed
);

// NEW VERSION (v2)
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"),),
    (identity, amount, timestamp, duration),  // timestamp now indexed at topic position 3
);
```

#### Step 3: Implement Dual Emission

During the migration period, emit both versions:

```rust
// Emit both versions for backward compatibility
events::emit_bond_created(&e, &identity, amount, duration);
events::emit_bond_created_v2(&e, &identity, amount, timestamp, duration);
```

**Dual emission duration**: Minimum 4 weeks to allow indexers to migrate. Coordinate with the backend team before removing v1 emission.

#### Step 4: Update Documentation

Update the following documents:

1. **`docs/EVENTS.md`**: Add the new event specification with complete topic/data documentation
2. **`docs/event-indexing.md`**: Update indexing guidance if the new version changes query patterns
3. **This document**: Add the version bump to the changelog section below

#### Step 5: Add Tests

Add comprehensive tests for the new event version:

```rust
#[test]
fn test_bond_created_v2_emission() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);
    
    // Setup: register token, admin, etc.
    // ...
    
    // Call create_bond
    client.create_bond(&identity, &amount, &duration, &is_rolling);
    
    // Verify both v1 and v2 events are emitted
    let events = env.events().all();
    let v1_events: Vec<_> = events
        .iter()
        .filter(|e| e.topics[0] == Symbol::new(&e, "bond_created"))
        .collect();
    let v2_events: Vec<_> = events
        .iter()
        .filter(|e| e.topics[0] == Symbol::new(&e, "bond_created_v2"))
        .collect();
    
    assert_eq!(v1_events.len(), 1);
    assert_eq!(v2_events.len(), 1);
    
    // Verify v2 has indexed timestamp
    let v2_event = &v2_events[0];
    assert_eq!(v2_event.topics[3], timestamp);  // timestamp at position 3
}
```

#### Step 6: Coordinate Deprecation

Before removing v1 event emission:

1. Confirm all known indexers have migrated to v2
2. Announce deprecation timeline to the integration team
3. Update documentation to mark v1 as deprecated
4. Remove v1 emission in a coordinated release

## Examples of Good vs Bad Changes

### Good Change: Adding Indexed Field

**Scenario**: Indexers need to filter bonds by creation timestamp efficiently.

**Approach**: Create `bond_created_v2` with timestamp as an indexed topic.

```rust
// GOOD: New version with added indexed field
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"),),
    (identity, amount, timestamp, duration),  // timestamp added as topic
);
```

**Why**: Preserves all existing fields, adds new field at end, enables efficient filtering.

### Bad Change: Reordering Fields

**Scenario**: You want to group identity-related fields together.

**Wrong Approach**: Reorder topics to put identity fields first.

```rust
// BAD: Reordering breaks positional parsing
e.events().publish(
    (Symbol::new(&e, "bond_created"),),  // Reusing old name
    (identity, timestamp, amount, duration),  // timestamp moved before amount
);
```

**Why**: Existing indexers expect `amount` at topic position 2. This breaks them.

**Correct Approach**: Create a new version with the desired order.

```rust
// GOOD: New version with reordered fields
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"),),
    (identity, timestamp, amount, duration),
);
```

### Good Change: Adding Optional Context

**Scenario**: You want to include the admin address in slash events for accountability.

**Approach**: Create `bond_slashed_v2` with admin as an indexed topic.

```rust
// GOOD: New version with added context
e.events().publish(
    (Symbol::new(&e, "bond_slashed_v2"),),
    (identity, slash_amount, total_slashed, timestamp, admin),  // admin added
);
```

**Why**: Adds accountability without breaking existing parsers.

### Bad Change: Changing Field Semantics

**Scenario**: You want to change `duration` from seconds to milliseconds.

**Wrong Approach**: Change the unit without version bump.

```rust
// BAD: Changing semantics breaks interpretation
e.events().publish(
    (Symbol::new(&e, "bond_created"),),
    (identity, amount, duration_ms),  // Now milliseconds, not seconds
);
```

**Why**: Existing indexers interpret the field as seconds, causing 1000x errors.

**Correct Approach**: Create a new version with a new field name.

```rust
// GOOD: New version with new field name
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"),),
    (identity, amount, duration, duration_ms),  // Both fields present
);
```

## Event Version Changelog

### 2026-07: Bond Lifecycle V2 Events

**Events**: `bond_created_v2`, `bond_withdrawn_v2`, `bond_increased_v2`, `bond_slashed_v2`

**Changes**:
- Added indexed `i128` amount fields for efficient range queries
- Added indexed `u64` timestamp fields for time-based filtering
- Added indexed `Address` admin field to `bond_slashed_v2` for accountability
- Added `bool` early withdrawal flag to `bond_withdrawn_v2`
- Added `BondTier` field to `bond_increased_v2` for tier change tracking

**Migration**: Dual emission period 2026-07 to 2026-08. V1 emission removed after indexer migration confirmed.

**Documentation**: See `docs/EVENT_INDEXING_MIGRATION.md` for detailed migration guide.

### 2026-06: Missing Lifecycle Events

**Events**: `bond_created`, `bond_withdrawn`, `bond_topped_up`, `bond_duration_extended`

**Changes**:
- Added `bond_created` event (previously silent)
- Added `bond_withdrawn` event (previously silent)
- Added `bond_topped_up` event (previously silent)
- Added `bond_duration_extended` event (previously silent)

**Rationale**: These operations were completely silent, making state reconstruction impossible without contract storage queries.

**Migration**: No backward compatibility concerns (new events, no changes to existing events).

## Testing Requirements

All event version changes must include:

1. **Unit tests**: Verify both v1 and v2 events are emitted correctly
2. **Data consistency tests**: Validate that v1 and v2 events contain consistent data
3. **Integration tests**: Test indexer parsing of both versions
4. **Regression tests**: Ensure existing functionality is not broken

## Review Checklist

Before submitting a PR that changes event schemas:

- [ ] Have you assessed the impact on existing indexers?
- [ ] Have you consulted with the backend team?
- [ ] Are you creating a new version rather than modifying an existing one?
- [ ] Does the new version preserve all existing fields?
- [ ] Are new fields added at the end of topic/data arrays?
- [ ] Are you implementing dual emission during migration?
- [ ] Have you updated `docs/EVENTS.md`?
- [ ] Have you updated `docs/event-indexing.md` if needed?
- [ ] Have you added this document's changelog section?
- [ ] Have you added comprehensive tests?
- [ ] Have you coordinated the deprecation timeline with the integration team?

## Emergency Schema Changes

In rare emergency cases (security vulnerabilities, critical bugs), you may need to break schema stability. In these cases:

1. **Escalate immediately**: Notify the tech lead and backend team
2. **Coordinate emergency migration**: Work with indexers to deploy hotfixes
3. **Document the break**: Clearly document why stability was violated
4. **Plan recovery**: Document how to restore stability after the emergency

Emergency changes should be exceptionally rare. If you find yourself needing them frequently, this indicates a deeper issue with event design.

## Additional Resources

- [Event Specification](./EVENTS.md) - Complete event schema reference
- [Event Indexing Guide](./event-indexing.md) - Indexer implementation guidance
- [Event Indexing Migration](./EVENT_INDEXING_MIGRATION.md) - Historical migration details
- [Contributing Guide](../CONTRIBUTING.md) - General contribution guidelines
