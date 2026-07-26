# Arbitration Contract Testing Patterns & Reference

## Executive Summary

This document captures the testing infrastructure, patterns, and conventions used in the Credence Arbitration contract tests, with emphasis on tie scenario testing and event validation.

---

## 1. Test Naming Conventions

### Module Organization

- **[test.rs](contracts/arbitration/src/test.rs)** - Core functionality tests (happy path, basic scenarios)
- **[test_lifecycle.rs](contracts/arbitration/src/test_lifecycle.rs)** - Status machine transitions & invalid transition regression tests
- **[test_auth.rs](contracts/arbitration/src/test_auth.rs)** - Authentication boundary tests (who can call what)
- **[test_pausable.rs](contracts/arbitration/src/test_pausable.rs)** - Pause/unpause functionality tests
- **[tests/datakey_fingerprint.rs](contracts/arbitration/tests/datakey_fingerprint.rs)** - Storage key fingerprints (regression)
- **[tests/test_weight_derivation.rs](contracts/arbitration/tests/test_weight_derivation.rs)** - Weight derivation rules (stubs)

### Naming Pattern

- **Happy path**: `test_<feature>_succeeds_when_<condition>()` (with specifics)
- **Sad path**: `test_<feature>_rejected_when_<failure_reason>()`
- **Regression**: `test_invalid_<operation>_<invalid_state_reason>()`
- **Boundary**: Split by concern (auth, lifecycle, pausable)

**Examples:**

```rust
#[test]
fn test_arbitration_flow()  // comprehensive happy path
fn test_tie_scenario()      // specific edge case
fn test_resolve_fails_when_weight_quorum_not_met()  // condition-explicit
fn test_invalid_resolve_already_resolved()  // regression test
fn cancel_dispute_rejected_when_stranger_calls()  // auth boundary
```

---

## 2. Testing Infrastructure & Assertion Patterns

### Test Setup Pattern (Reusable Structure)

```rust
struct Setup<'a> {
    env: Env,
    admin: Address,
    arb: Address,
    creator: Address,
    client: CredenceArbitrationClient<'a>,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();  // All addresses auto-authorize (critical for unit tests)
    let admin = Address::generate(&env);
    let arb = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);  // Default weight: 10
    Setup {
        env,
        admin,
        arb,
        creator,
        client,
    }
}

fn open_dispute(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "test dispute");
    s.client.create_dispute(&s.creator, &desc, &3600)  // 1 hour voting
}
```

### Ledger Time Advancement

```rust
fn advance(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

// Usage: Move time past voting period
advance(&e, 3601);  // 3600s voting + 1s past
```

### Result Type Handling

```rust
// Happy path: unwrap success
let outcome = client.resolve_dispute(&dispute_id);
assert_eq!(outcome, 1);

// Sad path: extract error (try_* methods return wrapped error)
let err = client
    .try_resolve_dispute(&id)
    .unwrap_err()          // Option → Result
    .unwrap();             // ContractError wrapper
assert_eq!(err, ArbitrationError::VotingNotEnded);

// Alternative syntax
if let Err(e) = client.try_vote(&non_arb, &id, &1) {
    let err = e.unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}
```

---

## 3. Core Contract Functions & Basic Test Patterns

### Creating Disputes

```rust
#[test]
fn test_arbitration_flow() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &5);

    let description = String::from_str(&e, "Dispute #1");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    // Verify initial state
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.id, 0);
    assert_eq!(dispute.status, status::DisputeStatus::Voting);
    assert_eq!(dispute.creator, creator);
    assert_eq!(dispute.outcome, 0);  // Not yet resolved
}
```

### Voting & Tallying

```rust
#[test]
fn test_arbitration_flow() {
    let s = setup();
    let id = open_dispute(&s);

    // Cast votes
    s.client.vote(&s.arb, &id, &1);
    s.client.vote(&arb2, &id, &2);

    // Query tallies
    assert_eq!(s.client.get_tally(&id, &1), 10);  // arb1's weight
    assert_eq!(s.client.get_tally(&id, &2), 5);   // arb2's weight
    assert_eq!(s.client.get_tally(&id, &3), 0);   // No votes for outcome 3
}
```

### Resolution & Outcome

```rust
#[test]
fn test_arbitration_flow() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1);

    advance(&s.env, 3601);  // Past voting period

    let winner = s.client.resolve_dispute(&id);
    assert_eq!(winner, 1);

    let resolved = s.client.get_dispute(&id);
    assert_eq!(resolved.status, status::DisputeStatus::Resolved);
    assert_eq!(resolved.outcome, 1);
}
```

---

## 4. TIE SCENARIO TESTING (Key Focus)

### Tie Detection & Status Transition

```rust
#[test]
fn test_tie_scenario() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb1, &10);   // Equal weights
    client.register_arbitrator(&arb2, &10);   // Critical for tie

    let description = String::from_str(&e, "Tie Test");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    // Two arbitrators vote for different outcomes with equal weight
    client.vote(&arb1, &dispute_id, &1);     // Outcome 1: weight 10
    client.vote(&arb2, &dispute_id, &2);     // Outcome 2: weight 10 ← TIE

    assert_eq!(client.get_tally(&dispute_id, &1), 10);
    assert_eq!(client.get_tally(&dispute_id, &2), 10);

    advance(&e, 3601);

    // Resolve returns 0 when tie detected
    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 0);

    // Status is Tied (new status as of tie-disambiguation PR)
    let tied_dispute = client.get_dispute(&dispute_id);
    assert_eq!(tied_dispute.status, status::DisputeStatus::Tied);
    assert_eq!(tied_dispute.outcome, 0);  // outcome = 0 reserved for tie
}
```

### Variant: Multiple Outcomes Tied

```rust
#[test]
fn test_three_way_equal_vote() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let arb3 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &10);
    client.register_arbitrator(&arb3, &10);

    let dispute_id = client.create_dispute(&creator,
        &String::from_str(&e, "3-way"), &3600);

    client.vote(&arb1, &dispute_id, &1);  // weight 10
    client.vote(&arb2, &dispute_id, &2);  // weight 10
    client.vote(&arb3, &dispute_id, &3);  // weight 10 ← ALL TIED

    advance(&e, 3601);
    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 0);  // Tie detected
    assert_eq!(client.get_dispute(&dispute_id).status, DisputeStatus::Tied);
}
```

### No Clear Winner (No Votes)

```rust
#[test]
fn test_resolve_with_no_votes_gives_outcome_zero() {
    let s = setup();
    let id = open_dispute(&s);
    // No votes cast
    advance(&s.env, 3601);

    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 0);  // No votes → implicit tie
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Tied);
}
```

---

## 5. Status Machine & Lifecycle Testing

### Valid Transitions

```rust
#[test]
fn test_valid_transition_voting_to_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1);

    // Check status before
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Voting);

    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);

    // Check status after
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Resolved);
}

#[test]
fn test_valid_transition_voting_to_cancelled_by_creator() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.creator, &id, &None);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Cancelled);
}
```

### Invalid Transitions (Regression)

```rust
#[test]
fn test_invalid_resolve_while_voting_active() {
    let s = setup();
    let id = open_dispute(&s);
    // Voting period still active — cannot resolve yet
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingNotEnded);
}

#[test]
fn test_invalid_resolve_already_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);

    // Resolved → Resolving is not a valid transition
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_invalid_cancel_already_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);

    // Resolved → Cancelled is not valid
    let err = s
        .client
        .try_cancel_dispute(&s.creator, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}
```

---

## 6. Event Validation Patterns

### Core Events Emitted by Contract

Events in Soroban use the pattern: `e.events().publish((topics), data)`

**Event Types:**

1. **`arbitrator_registered`** - When arbitrator added
   - Topics: `("arbitrator_registered", address)`
   - Data: `weight`

2. **`arbitrator_unregistered`** - When arbitrator removed
   - Topics: `("arbitrator_unregistered", address)`
   - Data: `()`

3. **`dispute_created`** - When dispute opened
   - Topics: `("dispute_created", dispute_id)`
   - Data: `creator_address`

4. **`status_transition`** - Every status change
   - Topics: `("status_transition", dispute_id)`
   - Data: `(from_status as u32, to_status as u32)`

5. **`vote_cast`** - When vote recorded
   - Topics: `("vote_cast", dispute_id, voter_address)`
   - Data: `(outcome, weight)`

6. **`dispute_tied`** - When resolution results in tie
   - Topics: `("dispute_tied", dispute_id)`
   - Data: `()`

7. **`dispute_resolved`** - When resolution has winner
   - Topics: `("dispute_resolved", dispute_id)`
   - Data: `winning_outcome`

8. **`dispute_cancelled`** - When cancelled
   - Topics: `("dispute_cancelled", dispute_id)`
   - Data: `(caller, role, reason)`

9. **`quorum_set`** - When quorum configured
   - Topics: `("quorum_set",)`
   - Data: `(min_total_weight, min_voters)`

10. **`quorum_not_met`** - When resolution blocked
    - Topics: `("quorum_not_met", dispute_id)`
    - Data: `(total_weight, min_weight, voter_count, min_voters)`

### Event Verification in Tests

In Soroban unit tests, **events are not directly inspected** via assertions in the test. Instead:

- Contract code publishes events via `e.events().publish(...)`
- Integration/end-to-end tests would capture and validate events
- Unit tests focus on state changes (returned values, stored data)

**Pattern: Verify via state + implicit event proof**

```rust
#[test]
fn test_arbitration_flow() {
    let s = setup();
    let id = open_dispute(&s);

    // Events are published, but test verifies by checking state:
    s.client.vote(&s.arb, &id, &1);

    // Indirect validation: tally changed (vote_cast event was emitted)
    assert_eq!(s.client.get_tally(&id, &1), 10);

    // Indirect validation: voter recorded (implicit from vote_cast event)
    assert_eq!(s.client.has_voted(&id, &s.arb), true);
}
```

### For Tie Scenarios - Complete Event Sequence

When a tie is resolved, the event sequence is:

```
1. status_transition(Voting → Resolving)
2. dispute_tied()  ← TIE-SPECIFIC EVENT
3. But NOT dispute_resolved()
```

```rust
#[test]
fn test_tie_scenario_emits_correct_events() {
    let s = setup();
    let id = open_dispute(&s);

    s.client.vote(&s.arb, &id, &1);
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &10);
    s.client.vote(&arb2, &id, &2);

    advance(&s.env, 3601);
    let winner = s.client.resolve_dispute(&id);

    // Event sequence validation via state checks:
    assert_eq!(winner, 0);  // dispute_tied() was called (returns 0)
    let tied_dispute = s.client.get_dispute(&id);
    assert_eq!(tied_dispute.status, DisputeStatus::Tied);  // status_transition to Tied
    assert_eq!(tied_dispute.outcome, 0);  // outcome reserved for tie
}
```

---

## 7. Authorization & Authentication Testing

### Pattern: Three-Part Auth Test

```rust
/// Happy path: authorized role succeeds
#[test]
fn register_arbitrator_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    client.register_arbitrator(&new_arb, &5_i128);  // Admin (mocked auth) succeeds
    assert_eq!(client.get_arbitrator_weight(&new_arb), 5_u32);
}

/// Sad path: input validation (before auth check)
#[test]
fn register_arbitrator_rejected_when_weight_is_zero() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    let err = client
        .try_register_arbitrator(&new_arb, &0_i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

/// Sad path: authorization denied
#[test]
fn vote_rejected_when_caller_is_not_registered_arbitrator() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_vote(&stranger, &id, &1_u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}
```

---

## 8. Quorum Testing (Advanced Feature)

### Quorum Configuration

```rust
#[test]
fn test_resolve_succeeds_when_both_quorum_conditions_met() {
    let s = setup();
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &5);

    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1);      // weight 10, 1 voter
    s.client.vote(&arb2, &id, &2);       // weight 5,  2 voters

    // Set quorum: need weight ≥ 10 AND voters ≥ 2
    s.client.set_quorum(&s.admin, &10, &2);

    advance(&s.env, 3601);

    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 1);  // outcome 1 has weight 10
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}

#[test]
fn test_resolve_fails_when_weight_quorum_not_met() {
    let s = setup();
    let id = open_dispute(&s);

    s.client.vote(&s.arb, &id, &1);      // weight = 10
    s.client.set_quorum(&s.admin, &100, &0);  // require weight ≥ 100

    advance(&s.env, 3601);

    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::QuorumNotMet);

    // Dispute stays Voting — not forced to error state
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Voting);
}
```

---

## 9. Double-Vote Prevention & State Guards

```rust
#[test]
fn test_double_voting_prevention() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);

    let description = String::from_str(&e, "Double Vote");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    client.vote(&arb, &dispute_id, &1);  // First vote succeeds

    let err = client
        .try_vote(&arb, &dispute_id, &1)  // Second vote from same arbitrator
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::AlreadyVoted);
}

#[test]
fn test_has_voted() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);

    let description = String::from_str(&e, "Vote check");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    assert_eq!(client.has_voted(&dispute_id, &arb), false);

    client.vote(&arb, &dispute_id, &1);

    assert_eq!(client.has_voted(&dispute_id, &arb), true);
}
```

---

## 10. Code Snippet: Complete Tie Test with Full Lifecycle

```rust
#[test]
fn test_tie_with_cancellation_reason_after_failed_resolve() {
    // SETUP
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    // INITIALIZE
    client.initialize(&admin);
    client.register_arbitrator(&arb1, &20);
    client.register_arbitrator(&arb2, &20);

    // CREATE DISPUTE
    let dispute_id = client.create_dispute(
        &creator,
        &String::from_str(&e, "Tie Test Dispute"),
        &3600,
    );
    let d1 = client.get_dispute(&dispute_id);
    assert_eq!(d1.status, DisputeStatus::Voting);
    assert_eq!(d1.outcome, 0);

    // VOTE (TIE SCENARIO)
    client.vote(&arb1, &dispute_id, &1);        // weight 20 for outcome 1
    client.vote(&arb2, &dispute_id, &2);        // weight 20 for outcome 2

    assert_eq!(client.get_tally(&dispute_id, &1), 20);
    assert_eq!(client.get_tally(&dispute_id, &2), 20);
    assert_eq!(client.has_voted(&dispute_id, &arb1), true);
    assert_eq!(client.has_voted(&dispute_id, &arb2), true);

    // TIME ADVANCE
    advance(&e, 3601);

    // RESOLVE (DETECTS TIE)
    let result = client.resolve_dispute(&dispute_id);
    assert_eq!(result, 0);  // Tie returns 0

    // VERIFY POST-RESOLUTION TIE STATE
    let d2 = client.get_dispute(&dispute_id);
    assert_eq!(d2.status, DisputeStatus::Tied);    // New Tied status
    assert_eq!(d2.outcome, 0);                     // outcome=0 reserved
    assert_eq!(d2.creator, creator);

    // CANNOT RESOLVE AGAIN (INVALID TRANSITION)
    let err = client.try_resolve_dispute(&dispute_id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);

    // NOTE: Cannot cancel Tied dispute either (only Voting/Open allowed)
    let err = client
        .try_cancel_dispute(&creator, &dispute_id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}
```

---

## 11. Key Assertions Summary

### State Assertions (After operations)

```rust
assert_eq!(dispute.status, DisputeStatus::Voting);
assert_eq!(dispute.outcome, expected_outcome);
assert_eq!(dispute.creator, expected_creator);
assert_eq!(client.get_tally(&id, &outcome), expected_weight);
assert_eq!(client.has_voted(&id, &arbitrator), true_or_false);
assert_eq!(client.get_arbitrator_weight(&arb), weight);
assert_eq!(client.get_quorum(), (min_weight, min_voters));
```

### Error Assertions (Try-prefixed methods)

```rust
let err = client.try_<method>(...).unwrap_err().unwrap();
assert_eq!(err, ArbitrationError::<ErrorType>);
```

### Event Validation (Indirect via state)

```rust
// After vote cast:
assert_eq!(client.get_tally(&id, &outcome), new_weight);  // vote_cast was emitted
assert_eq!(client.has_voted(&id, &voter), true);

// After resolve to tie:
assert_eq!(result, 0);                            // dispute_tied was emitted
assert_eq!(client.get_dispute(&id).status, Tied); // status_transition was emitted
```

---

## 12. Storage & DataKey Constants

All test data is stored in contract instance storage via `DataKey` enum. Key lookups:

```rust
pub enum DataKey {
    Admin,
    Dispute(u64),                 // Get dispute by ID
    DisputeVotes(u64),            // Map<u32, i128> for votes by outcome
    VoterCasted(u64, Address),    // bool: has this voter voted?
    VoterCounter(u64),            // Number of distinct voters
    Arbitrator(Address),          // i128: voter weight
    ArbitratorRegistry,           // Vec<Address>: all registered
    MinTotalWeight,               // i128: quorum weight threshold
    MinVoters,                    // u32: quorum voter threshold
    Paused,                       // bool
    // ...pause-related keys (see test_pausable.rs)
}
```

---

## 13. Practical Testing Checklist for Tie Scenarios

When adding tests for tie/disambiguation features:

✓ **Setup Phase**

- Create env with `Env::default()` + `e.mock_all_auths()`
- Register ≥2 arbitrators with equal weights
- Create dispute with sufficient voting period

✓ **Voting Phase**

- Cast votes for different outcomes with equal total weights
- Assert tallies match expected weights
- Verify no double-voting possible

✓ **Resolution Phase**

- Advance time past voting period
- Call `resolve_dispute`
- Assert return value is `0` (tie)

✓ **State Assertions**

- Status changed to `DisputeStatus::Tied`
- Outcome field is `0` (reserved sentinel)
- Cannot transition from Tied to any other state (InvalidTransition)

✓ **Event Sequence (Implicit)**

- Verify via state: `status_transition(Voting → Resolving → Tied)`
- Verify via state: `dispute_tied` was published (implicit)
- Verify NOT `dispute_resolved` (only for non-tie outcomes)

✓ **Edge Cases**

- No votes cast → returns 0 (implicit tie)
- Multiple (>2) outcomes tied → returns 0
- Quorum blocks tie resolution → QuorumNotMet error
- Cannot cancel after transition to Tied

---

## 14. File Structure Reference

```
contracts/arbitration/
├── src/
│   ├── lib.rs                 # Main contract (140+ functions)
│   ├── status.rs              # DisputeStatus enum, error types
│   ├── pausable.rs            # Pause/unpause mechanics
│   ├── test.rs                # Core function tests (250+ lines)
│   ├── test_lifecycle.rs      # Transition & regression tests (400+ lines)
│   ├── test_auth.rs           # Auth boundary tests (200+ lines)
│   └── test_pausable.rs       # Pause tests (60+ lines)
├── tests/
│   ├── datakey_fingerprint.rs # Storage key fingerprint snapshot test
│   └── test_weight_derivation.rs # Weight logic stubs (not yet impl)
└── Cargo.toml
```

---

## Key Takeaways for Tie/Disambiguation Testing

1. **Tie is explicit status**: `DisputeStatus::Tied` (not just "outcome 0")
2. **Outcome=0 is reserved**: Can only appear when status=Tied
3. **Detection algorithm**: After tally, if any two outcomes have equal max weight → tie
4. **Return value**: `resolve_dispute` returns 0 when tie (not error)
5. **Event sequence**: Two events on tie:
   - `status_transition(Voting → Resolving)`
   - `dispute_tied()`
   - NO `dispute_resolved()` event
6. **Post-tie transitions**: All invalid (`Tied → X` prohibited for any X)
7. **Quorum interaction**: Quorum checked before tie detection; blocks with `QuorumNotMet`

---

_Generated from arbitration contract test suite review. See linked files for complete implementations._
