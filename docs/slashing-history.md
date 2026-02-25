# Slashing History Module

## Overview

The Slashing History module provides permanent, queryable storage of all slashing events executed against bonded identities.

It ensures transparency, auditability, and accurate tracking of penalties applied over time.

This module integrates with the core `slashing.rs` logic and records every slash as an immutable history entry.

---

## Features

- **Append-only history** — Records cannot be modified or deleted
- **Per-identity indexing** — Efficient lookup by bonded identity
- **Timestamped events** — Uses ledger timestamp
- **Reason tracking** — Stores symbolic slash justification
- **Cumulative tracking** — Records total slashed after each event
- **Queryable records** — Supports full and indexed history retrieval
- **Derived totals** — Calculates total slashed from history

---

## Data Model

### SlashRecord

Represents a single slashing event.

| Field | Type | Description |
|------|------|-------------|
| identity | Address | Slashed bonded identity |
| slash_amount | i128 | Amount slashed in event |
| reason | Symbol | Slash justification |
| timestamp | u64 | Ledger timestamp |
| total_slashed_after | i128 | Cumulative slashed total |

---

## Storage Design

Storage uses a contracttype enum for efficient keying:
