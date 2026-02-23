# Dispute Resolution Contract

Handles on-chain disputes against slash requests with staked arbitration voting.

---

## Overview

Any identity can challenge a slash request by opening a dispute with a stake. Arbitrators vote before the deadline. The majority outcome determines whether the stake is returned or forfeited.

---

## Flow
```
create_dispute → cast_vote (multiple arbitrators) → resolve_dispute
                                                  → expire_dispute (if unresolved)
```

---

## Functions

| Function | Who Calls | Description |
|----------|-----------|-------------|
| `create_dispute` | Disputer | Opens dispute, pulls stake into contract |
| `cast_vote` | Arbitrator | Vote before deadline |
| `resolve_dispute` | Anyone | Finalizes after deadline |
| `expire_dispute` | Anyone | Marks expired if unresolved |
| `get_dispute` | Anyone | Fetch dispute by ID |
| `has_voted` | Anyone | Check if address voted |
| `get_dispute_count` | Anyone | Total disputes |

---

## Dispute Lifecycle

| Status | Meaning |
|--------|---------|
| `Open` | Accepting votes |
| `Resolved` | Outcome determined |
| `Expired` | Deadline passed, no resolution |

---

## Outcomes

| Outcome | Result |
|---------|--------|
| `FavorDisputer` | Stake returned to disputer |
| `FavorSlasher` | Stake forfeited in contract |

---

## Requirements

- Minimum stake: **100 tokens**
- Disputer must call `token.approve(contract_id, stake)` before `create_dispute`
- `resolution_deadline` must be > 0 (duration in seconds added to current timestamp)
- Votes locked after deadline — resolution locked before deadline

---

## Error Reference

| Code | Error | Cause |
|------|-------|-------|
| `#1` | `DisputeNotFound` | Invalid dispute ID |
| `#2` | `AlreadyVoted` | Arbitrator voted twice |
| `#3` | `DisputeNotOpen` | Dispute already resolved/expired |
| `#4` | `DeadlineNotReached` | Too early to resolve/expire |
| `#5` | `DeadlineExpired` | Voting period over |
| `#7` | `InsufficientStake` | Stake below minimum (100) |
| `#8` | `InvalidDeadline` | Duration set to 0 |

---

## Security Notes

- One vote per arbitrator enforced via `Vote(dispute_id, address)` storage key
- State updated before token transfers — no re-entrancy risk
- Minimum stake prevents spam disputes
- Timestamps sourced from `env.ledger().timestamp()` — not manipulable by callers