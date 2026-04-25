# Crate: Timelock

**Path:** `contracts/timelock`

## Overview

Ensures protocol safety by enforcing a mandatory delay for high-impact administrative changes.

## 1. Entrypoint Reference

| Function            | Roles  | Description                                                                     |
| :------------------ | :----- | :------------------------------------------------------------------------------ |
| `propose_operation` | Admin  | Queues a contract call (e.g., updating leverage constants) with a future `ETA`. |
| `execute_operation` | Public | Executes the queued call once `block_timestamp >= ETA`.                         |
| `cancel_operation`  | Gov    | Vetoes a pending operation before it is executed.                               |

## 2. Integration Notes

- **The Grace Period**: Operations have a `GRACE_PERIOD` (e.g., 24 hours) after the `ETA` during which they must be executed. If not executed, they expire and must be re-proposed.
- **Monitoring**: Backends should watch for `operation_proposed` events to provide a "Upcoming Changes" transparency dashboard for the community.
