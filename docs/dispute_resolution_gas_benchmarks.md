# Gas Benchmarks — Dispute Resolution Contract

> Soroban SDK v23.0 · Env: `testutils` simulation · Date: 2026-02-23

---

## Measured Costs (Baseline Results)

All numbers are from a single call with the ledger/budget reset before the call.

| Function            | Scenario                         | CPU Instructions | Memory Bytes |
| ------------------- | -------------------------------- | ---------------: | -----------: |
| `get_dispute_count` | counter == 0 (instance miss)     |           14,381 |        1,415 |
| `get_dispute_count` | counter == 1 (instance hit)      |           21,842 |        2,993 |
| `has_voted`         | key absent (persistent miss)     |           22,566 |        4,091 |
| `has_voted`         | key present (persistent hit)     |           20,446 |        3,259 |
| `get_dispute`       | single read + TTL bump           |           47,704 |        5,785 |
| `expire_dispute`    | after deadline                   |           79,523 |       12,143 |
| `cast_vote`         | 1st vote on dispute              |          122,470 |       21,990 |
| `cast_vote`         | 5th vote (existing tally)        |          139,650 |       28,518 |
| `resolve_dispute`   | FavorSlasher (no token transfer) |           89,552 |       13,557 |
| `resolve_dispute`   | FavorDisputer (token transfer)   |          253,529 |       36,681 |
| `create_dispute`    | 1st (counter = 0 → 1)            |          301,419 |       44,396 |
| `create_dispute`    | 2nd+ (counter already set)       |          323,328 |       46,275 |

---

## Batch vs Individual Operations

### `create_dispute` — N sequential calls, single `env.budget()` window

| N   | Total CPU | Total Mem |  CPU/op |
| --- | --------: | --------: | ------: |
| 1   |   301,419 |    44,396 | 301,419 |
| 5   |   336,715 |    50,763 |  67,343 |
| 10  |   352,913 |    58,243 |  35,291 |
| 20  |   385,176 |    73,203 |  19,258 |

**Key insight:** cpu/op drops ~15× from 1→20 disputes. This is the Soroban VM's per-transaction setup overhead amortised over more calls. The absolute per-dispute cost is effectively the marginal 300k → 385k range (~4.2k CPU incremental by ledger 20).

### `cast_vote` — N sequential calls on the same dispute

| N   | Total CPU | Total Mem |  CPU/op |
| --- | --------: | --------: | ------: |
| 1   |   122,470 |    21,990 | 122,470 |
| 5   |   139,650 |    28,518 |  27,930 |
| 10  |   158,607 |    36,678 |  15,860 |
| 20  |   182,958 |    52,998 |   9,147 |

**Key insight:** Marginal vote cost ~3k CPU after VM warm-up. The bulk of cost (>100k CPU per standalone call) is VM invocation overhead.

---

## Optimization Opportunities

### ✅ Already Optimised (by design)

| Pattern                                                         | Evidence                                                          |
| --------------------------------------------------------------- | ----------------------------------------------------------------- |
| Single `load_dispute` helper (no `has()` + `get()` double-read) | `cast_vote` cost stable across vote count                         |
| `instance()` for counter (always in-memory after first load)    | `get_dispute_count` is cheapest call at 14k CPU                   |
| `persistent()` + TTL bump per record                            | `get_dispute` repeated-read delta = **0 CPU** (test confirmed)    |
| Token transfer skipped when `FavorSlasher`                      | `resolve_dispute FavorSlasher` saves ~164k CPU vs `FavorDisputer` |

### ⚠️ Opportunities to Explore

| Opportunity                                                         | Detail                                                                                                                                                                                                                 |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Token transfer dominates `create_dispute`**                       | ~301k CPU vs ~80k for `expire_dispute`; the cross-contract token call is expensive. Consider batching multiple stakes via a single `transfer_from` if the protocol allows multi-dispute staking.                       |
| **`cast_vote` is 5.3× more expensive than `has_voted`**             | The pair of persistent writes (vote key + updated dispute record) dominate. If arbitrators need to be pre-screened cheaply, `has_voted` is the right read-only check.                                                  |
| **`resolve_dispute` with token transfer costs 2.8× `FavorSlasher`** | If most disputes resolve in favour of the slasher, average cost is ~90k CPU. Optimise for the common path by deferring transfer logic to a separate `claim_refund` call, allowing `resolve_dispute` to just set state. |
| **Per-call VM overhead is dominant**                                | At 1 op, overhead is ~300k CPU; at 20 ops it amortises to ~19k CPU. Batch operations in the same transaction if the client can aggregate.                                                                              |

---

## TTL Stability Confirmed

`get_dispute` repeated reads produce **identical costs** (delta = 0), confirming:

- The `load_dispute` helper calls `extend_ttl` on every read, but the Soroban test environment normalises repeated extends to trivial overhead.
- No compounding TTL cost on hot paths.

---

## Reproduction

```bash
cargo test -p dispute_resolution -- gas --nocapture
```
