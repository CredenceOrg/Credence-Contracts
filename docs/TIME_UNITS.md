# Time Units and Representation

This document describes how time (seconds, days, and epochs) is represented, calculated, and tested across all Credence Soroban contracts.

---

## Audience: Contributor

This document is written for **contributors** developing or auditing Credence smart contracts. It defines our time representation standards to ensure consistency and correctness across all crates in the workspace.

---

## Core Time Representation

All contracts in the Credence protocol use **seconds** as the base unit for time durations, offsets, and absolute timestamps. 

1. **Type**: Time values are represented as `u64` integers.
2. **Epoch**: Absolute timestamps represent the number of seconds elapsed since the Unix epoch (January 1, 1970 00:00:00 UTC).
3. **No Float/Decimal Time**: Floating-point numbers are not supported in Soroban smart contracts. All calculations must use integer arithmetic.
4. **No `std` Time Primitives**: Contracts must maintain `#[no_std]` discipline. Do not use standard library time primitives like `std::time::SystemTime`. Use `soroban_sdk` environment primitives instead.

---

## Retrieving Current Time

The current ledger time is retrieved using the Soroban SDK's environment ledger interface:

```rust
use soroban_sdk::Env;

pub fn get_current_time(e: &Env) -> u64 {
    e.ledger().timestamp()
}
```

This returns a `u64` representing the Unix timestamp of the current ledger.

---

## Common Time Conversions

When defining durations, prefer explicit constants representing seconds:

| Period | Seconds (u64) | Formula / Expression |
| :--- | :--- | :--- |
| **1 Minute** | `60` | `60` |
| **1 Hour** | `3,600` | `60 * 60` |
| **1 Day** | `86,400` | `24 * 60 * 60` |
| **1 Week** | `604,800` | `7 * 24 * 60 * 60` |
| **1 Year (365 days)** | `31,536,000` | `365 * 24 * 60 * 60` |

### Concrete Code Example: Proposal Expiry

In the `credence_treasury` contract, proposal expiry is computed by adding a Time-To-Live (TTL) duration in seconds to the current ledger timestamp.

```rust
use soroban_sdk::Env;

pub struct Proposal {
    pub proposed_at: u64,
    pub expires_at: u64,
}

pub fn propose(e: &Env, ttl_seconds: u64) -> Proposal {
    let now = e.ledger().timestamp();
    // Use saturating_add to prevent overflow on extreme ledger timestamps
    let expires_at = now.saturating_add(ttl_seconds);
    
    Proposal {
        proposed_at: now,
        expires_at,
    }
}

pub fn is_expired(e: &Env, proposal: &Proposal) -> bool {
    e.ledger().timestamp() >= proposal.expires_at
}
```

---

## Testing & Advancing Time

To test time-dependent logic (such as checking if a timelock or cooldown has expired), modify the ledger timestamp within the unit test using the mock environment.

### Concrete Code Example: Advancing the Ledger

Below is a complete test example showing how to initialize a ledger timestamp, perform an action, advance the mock ledger time, and verify the resulting state.

```rust
use soroban_sdk::{Env, testutils::Ledger};

#[test]
fn test_cooldown_expiration() {
    let e = Env::default();
    
    // Set initial mock timestamp to 1,000,000 (roughly 11.5 days since epoch)
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
    });
    
    let proposed_at = e.ledger().timestamp();
    assert_eq!(proposed_at, 1_000_000);
    
    // A 1-day cooldown period (86,400 seconds)
    let cooldown_period = 86_400; 
    let expires_at = proposed_at.saturating_add(cooldown_period);
    
    // Verify not expired yet
    assert!(e.ledger().timestamp() < expires_at);
    
    // Mock the passing of time by advancing the ledger timestamp by 24 hours
    e.ledger().with_mut(|li| {
        li.timestamp += cooldown_period;
    });
    
    // Verify that the current time is exactly at the expiration threshold
    assert_eq!(e.ledger().timestamp(), 1_086_400);
    assert!(e.ledger().timestamp() >= expires_at);
}
```
