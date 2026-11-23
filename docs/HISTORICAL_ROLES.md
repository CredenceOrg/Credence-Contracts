# Historical Role Assignments

## Audience

This document is written for **downstream integrators** (indexers, analytics dashboards, compliance tooling) who need to reconstruct the complete timeline of who held which administrative role at any point in the contract's history.

## Why We Track Role History

The Credence admin system supports three roles (SuperAdmin, Admin, Operator) with a strict hierarchy. Roles can be:

- **Assigned** — a new address receives a role
- **Reassigned** — an existing admin's role changes
- **Revoked** — an address loses its role entirely
- **Suspended** — temporarily inactive with auto-expiry
- **Deactivated** — indefinitely inactive until manual reactivation
- **Transferred** — ownership (SuperAdmin) moves via a two-step timelocked handoff

Off-chain systems (indexers, audit dashboards, incident-response runbooks) must answer questions like:

- "Which addresses could call `slash()` at block N?"
- "Did address X have Admin privileges when transaction Y was submitted?"
- "Show the full chain of custody for the SuperAdmin role."

Because Soroban storage only holds the *current* state, the authoritative history lives in **contract events**. This document specifies exactly which events to index and how to interpret them.

## Event Stream Specification

Every role-affecting operation emits one or more events. Indexers should treat the event stream as the source of truth.

### Core Events (always emitted)

| Event | Topics | Data | Meaning |
|-------|--------|------|---------|
| `admin_initialized` | `("admin_initialized",)` | `(super_admin: Address)` | Contract bootstrap; first SuperAdmin created |
| `admin_added` | `("admin_added",)` | `(admin_info: AdminInfo)` | New address granted a role |
| `admin_removed` | `("admin_removed",)` | `(admin_info: AdminInfo)` | Address fully removed from admin set |
| `admin_role_updated` | `("admin_role_updated",)` | `(address: Address, old_role: AdminRole, new_role: AdminRole)` | Existing admin's role changed |
| `admin_deactivated` | `("admin_deactivated",)` | `(admin_info: AdminInfo)` | Admin marked inactive (indefinite) |
| `admin_reactivated` | `("admin_reactivated",)` | `(admin_info: AdminInfo)` | Previously deactivated admin restored |
| `admin_suspended` | `("admin_suspended",)` | `(address: Address, until_ts: u64)` | Admin suspended until `until_ts` (auto-expiring) |
| `ownership_transfer_initiated` | `("ownership_transfer_initiated",)` | `(current_owner: Address, pending_owner: Address)` | Two-step SuperAdmin handoff started |
| `ownership_transfer_accepted` | `("ownership_transfer_accepted",)` | `(previous_owner: Address, new_owner: Address)` | Handoff completed |
| `admin_rotated` | `("admin_rotated",)` | `(old_admin: Address, new_admin: Address, ledger_seq: u32)` | Compact record of SuperAdmin rotation (emitted alongside `ownership_transfer_accepted`) |

### Supplemental Events (emitted alongside core events for ergonomic indexing)

| Event | Topics | Data | Emitted By |
|-------|--------|------|------------|
| `ROLE_ASSIGNED` | `("ROLE_ASSIGNED", address)` | `(role: AdminRole, assigned_by: Address)` | `add_admin`, `update_admin_role`, `reactivate_admin` |
| `ROLE_REVOKED` | `("ROLE_REVOKED", address)` | `(revoked_by: Address)` | `remove_admin`, `deactivate_admin` |

> **Note**: `ROLE_ASSIGNED` and `ROLE_REVOKED` are convenience events with a flat topic structure (`Symbol, Address`) that many indexers find easier to filter than the nested `AdminInfo` struct. They carry no information not already in the core events.

### AdminInfo Structure

All core events that carry an `AdminInfo` embed the same struct (defined in `contracts/admin/src/lib.rs`):

```rust
pub struct AdminInfo {
    pub address: Address,         // The admin address
    pub role: AdminRole,          // SuperAdmin | Admin | Operator
    pub assigned_at: u64,         // Ledger timestamp when this role was granted
    pub assigned_by: Address,     // Address that granted the role
    pub active: bool,             // False if deactivated (indefinite)
    pub suspended_until: u64,     // 0 = not suspended; > ledger.timestamp() = suspended
}
```

`AdminRole` is a `u32` enum: `SuperAdmin = 3`, `Admin = 2`, `Operator = 1`.

## Reconstructing Effective Authority at a Given Ledger

To answer "could address X call entrypoint Y at ledger L?":

1. **Replay events in ledger order** up to and including ledger L.
2. **Maintain a map** `address -> AdminInfo` reflecting the latest state.
3. **Apply each event**:
   - `admin_initialized` / `admin_added` / `admin_role_updated` / `admin_reactivated` → upsert `AdminInfo` (update `assigned_at`, `assigned_by`, `active=true`, `suspended_until=0`).
   - `admin_removed` → delete entry.
   - `admin_deactivated` → set `active = false`.
   - `admin_suspended` → set `suspended_until = until_ts`.
   - `ownership_transfer_accepted` / `admin_rotated` → the *new* SuperAdmin gets a fresh `AdminInfo` with `assigned_at = ledger_timestamp(L)`; the old SuperAdmin is removed (or deactivated if `remove_admin` is not called).
4. **Evaluate effective role** at ledger L:
   ```rust
   fn effective_role(info: &AdminInfo, ledger_ts: u64) -> Option<AdminRole> {
       if !info.active { return None; }
       if ledger_ts < info.suspended_until { return None; }
       Some(info.role)
   }
   ```
5. **Check authorization** against the entrypoint's required role (see [access-control.md](access-control.md) and [admin-roles.md](admin-roles.md) for the authority matrix).

## Complete Worked Example

Consider this sequence on a fresh contract (ledger timestamps in seconds):

| Ledger | Timestamp | Event | Effect on `address -> AdminInfo` |
|--------|-----------|-------|----------------------------------|
| 100 | 1000 | `admin_initialized(S=AdminA)` | `AdminA: {role=SuperAdmin, assigned_at=1000, assigned_by=AdminA, active=true, suspended_until=0}` |
| 110 | 1100 | `admin_added(AdminB, Admin)` | `AdminB: {role=Admin, assigned_at=1100, assigned_by=AdminA, ...}` |
| 120 | 1200 | `admin_added(OperatorC, Operator)` | `OperatorC: {role=Operator, assigned_at=1200, assigned_by=AdminB, ...}` |
| 130 | 1300 | `admin_role_updated(AdminB, SuperAdmin)` | `AdminB.role=SuperAdmin, assigned_at=1300, assigned_by=AdminA` |
| 140 | 1400 | `admin_suspended(OperatorC, 1500)` | `OperatorC.suspended_until=1500` |
| 150 | 1500 | (auto-expiry — no event) | `OperatorC.suspended_until=0` (implicit) |
| 160 | 1600 | `admin_deactivated(AdminB)` | `AdminB.active=false` |
| 170 | 1700 | `ownership_transfer_initiated(AdminA, AdminD)` | pending owner = AdminD |
| 180 | 1800 | `ownership_transfer_accepted(AdminD)` | `AdminD: {role=SuperAdmin, assigned_at=1800, assigned_by=AdminD, ...}`; `AdminA` removed |

### Queries at Specific Ledgers

| Query | Ledger | Result |
|-------|--------|--------|
| Who can call `slash()` (requires SuperAdmin)? | 105 | {AdminA} |
| Who can call `slash()`? | 115 | {AdminA} (AdminB is only Admin) |
| Who can call `slash()`? | 135 | {AdminA, AdminB} |
| Is OperatorC active? | 145 | No (suspended until 1500) |
| Is OperatorC active? | 155 | Yes (auto-reactivated) |
| Who is SuperAdmin? | 175 | AdminA (transfer pending) |
| Who is SuperAdmin? | 185 | AdminD |

## Indexer Implementation Checklist

- [ ] Ingest all core events listed above.
- [ ] Parse `AdminInfo` from `admin_added`, `admin_role_updated`, `admin_deactivated`, `admin_reactivated`, `admin_removed`.
- [ ] Track `admin_suspended` with `until_ts`; treat as a time-bounded `active=false` that self-clears.
- [ ] Handle `ownership_transfer_initiated` + `ownership_transfer_accepted` as an atomic SuperAdmin rotation (the `admin_rotated` event is a compact summary).
- [ ] Expose an RPC / GraphQL endpoint: `effective_admins(ledger_seq, min_role?) -> Vec<AdminInfo>`.
- [ ] Export CSV/Parquet snapshots for audit: one row per `(address, role, assigned_at, revoked_at, assigned_by, suspension_intervals[])`.

## Cross-References

- [admin-roles.md](admin-roles.md) — Full API reference, role hierarchy, authorization rules.
- [access-control.md](access-control.md) — Entrypoint × required-role matrix for the bond contract.
- [governance.md](governance.md) — Higher-level governance flows (multi-sig pause, emergency mode).

## Version History

| Version | Date | Author | Notes |
|---------|------|--------|-------|
| 1.0 | 2025-07-23 | Credence Team | Initial release |