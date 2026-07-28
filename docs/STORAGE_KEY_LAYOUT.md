# Storage Key Layout & Collision Safety

This document catalogs the storage keys each Credence contract owns, what
each key is used for, and the rules for adding new keys without colliding
with existing ones. It complements two existing documents which this one
does not duplicate:

- [STORAGE_KEYS.md](STORAGE_KEYS.md) — naming *convention* for key enums and variants.
- [datakey-fingerprint.md](datakey-fingerprint.md) — why renaming/retyping a variant orphans ledger state, and how the fingerprint test pins encodings.

Read those first if you're unfamiliar with how a `#[contracttype]` enum is
encoded — the collision rules below depend on it.

## How keys are encoded (the one fact that matters for collisions)

A Soroban `#[contracttype]` enum is encoded as an `ScVal::Vec` whose first
element is a `Symbol` of the **variant name**, followed by its fields in
declaration order. The ledger key is derived from **variant name + field
shape** — not from the enum's Rust type name, and not from declaration
order.

This has two direct consequences:

1. **Two different enums with a same-named, same-shaped unit variant collide.**
   `enum A { Admin }` and `enum B { Admin }` both encode to the identical
   `ScVal`. If both are ever written into the *same* storage instance, they
   alias the same ledger slot. The Rust type checker does not catch this —
   it's a runtime hazard, not a compile error.
2. **A single enum can never collide with itself.** Rust already forbids two
   variants sharing a name within one `enum` declaration, and reordering,
   appending, or adding new variants elsewhere in the enum has no effect on
   any other variant's key. Within one enum, the only two ways to break an
   existing key are covered in [datakey-fingerprint.md](datakey-fingerprint.md):
   renaming a variant, or changing its field count/types.

So collision safety across a contract comes down to auditing every
`#[contracttype]` enum that contract writes into the same storage tier
(instance / persistent / temporary), for overlapping variant names and
shapes.

## Storage tiers in play

Soroban exposes three independent storage tiers per contract
(`env.storage().instance()`, `.persistent()`, `.temporary()`); each tier is
its own keyspace. A key collision is only possible between two writes into
the **same tier of the same contract instance** — an `instance` key and a
`persistent` key with the identical encoding do not collide with each
other. Each contract's key catalog below is grouped by tier.

## Per-contract key catalog

### admin

Single enum `DataKey` (`contracts/admin/src/lib.rs`), all **instance** storage.

| Variant | Used for |
|---|---|
| `AdminList` | `Vec<Address>` of all admin addresses |
| `AdminInfo(Address)` | Per-address role, assignment metadata, active/suspended state |
| `RoleAdmins(AdminRole)` | `Vec<Address>` of admins per role |
| `Initialized` | One-time init guard |
| `MinAdmins` / `MaxAdmins` | Admin-count bounds |
| `Paused` | Global pause flag |
| `PauseSigner(Address)` | Authorized pause-signer flag |
| `PauseSignerCount` | Cached signer count |
| `PauseThreshold` | Approvals required to pause/unpause |
| `PauseProposalCounter` | Legacy proposal-id counter (retained for compatibility) |
| `PauseProposal(u64)` | Proposed pause/unpause action, keyed by hash-derived id |
| `PauseApproval(u64, Address)` | Per-signer approval flag for a proposal |
| `PauseApprovalCount(u64)` | Running approval tally |
| `Owner` | Current contract owner |
| `PendingOwner` | Pending owner (two-step transfer) |
| `TransferProposedAt` | Timestamp an ownership transfer was proposed (timelock gate) |

TTL: `STORAGE_TTL_EXTEND_TO = 31_536_000` ledgers, bumped at half-life via `bump_instance_ttl()`.

### arbitration

Single enum `DataKey` (`contracts/arbitration/src/lib.rs`), all **instance** storage.

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u64)`, `PauseApproval(u64, Address)`, `PauseApprovalCount(u64)` | Pause multisig (same pattern as `admin`) |
| `Arbitrator(Address)` | Voting weight for a registered arbitrator |
| `Dispute(u64)` | Dispute record by id |
| `DisputeCounter` | Next dispute id |
| `DisputeVotes(u64)` | Per-outcome weight tally for a dispute |
| `VoterCasted(u64, Address)` | Has-voted flag per (dispute, voter) |
| `VoterCounter(u64)` | Distinct-voter count per dispute (quorum check) |
| `ArbitratorRegistry` | `Vec<Address>` of all registered arbitrators |
| `MinTotalWeight` / `MinVoters` | Quorum configuration |
| `ActiveDispute(Address)` | Tracks a creator's in-flight dispute (one-at-a-time guard) |

TTL: same `31_536_000` / half-life pattern as `admin`.

### credence_bond

The largest contract, with one primary enum plus several sub-namespace
enums. See [Known issue](#known-issue-credence_bonds-datakey-has-duplicate-variant-declarations)
below before relying on the `DataKey` table.

**`DataKey`** (`contracts/credence_bond/src/lib.rs`) — **instance** storage
unless noted. This contract is deployed one-per-identity, so `Bond` holds a
single record rather than being keyed by address.

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u64)`, `PauseApproval(u64, Address)`, `PauseApprovalCount(u64)` | Pause multisig |
| `Bond` | The single `IdentityBond` record this contract instance owns |
| `Attester(Address)` | Attester registration/stake info |
| `Attestation(u64)` | Attestation record by id |
| `AttestationCounter` | Next attestation id |
| `SubjectAttestations(Address)` | List of attestation ids for a subject |
| `SubjectAttestationCount(Address)` | Count of attestations for a subject |
| `Nonce(Address)` | Replay-prevention nonce |
| `AttesterStake(Address)` | Attester's staked amount |
| `WeightConfig` | Weighted-attestation multiplier/max config |
| `EarlyExitConfig` | Early-exit treasury + penalty bps |
| `GraceWindow` | Signature-deadline grace window (seconds) |
| `BondToken` | Token used for deposits/payouts |
| `TierThresholds` | Bronze/Silver/Gold tier cutoffs |
| `LastCollateralIncreaseLedger` | Ledger of last top-up (same-ledger slash guard) |
| `PendingClaims(Address)` | **persistent** — pull-payment claims queued for a user |
| `ClaimableAmount(Address)` | **persistent** — total claimable amount for a user |
| `ClaimCounter` | **persistent** — monotonic claim-id counter |
| `ClaimById(u64)` | **persistent** — individual claim record by id |
| `Upgrade(UpgradeKey)` | Namespace wrapper — sub-keyed by `UpgradeKey`, see below |
| `SettlingFlag` | Reentrancy guard for token-call sequences |
| `LiquidationTreasury` | Optional residual-fund sweep recipient |
| `Liquidated(Address)` | Per-identity liquidated flag |
| `SlashTreasury` | Treasury receiving `slash()` proceeds |
| `IdempotencyKey(Bytes)` | Dedup key for webhook-triggered admin operations |
| `BorrowFrozen` | Global borrow-freeze flag |
| `ExecutedOp(BytesN<32>)` | Executed-upgrade replay guard |

**`UpgradeKey`** (`lib.rs`, sub-keyed under `DataKey::Upgrade`, **instance**):
`Auth(Address)`, `AuthorizedUpgraders`, `Implementation`, `Admin`,
`PndgUpgrAdmin`, `Proposal(u64)`, `NextProposalId`, `History`. Reached only
via `DataKey::Upgrade(UpgradeKey::X)`, so it cannot collide with a top-level
`DataKey` variant regardless of name overlap (`UpgradeKey::Admin` is a field
of `DataKey::Upgrade`, not a top-level key).

**`EmergencyDataKey`** (`emergency.rs`, **persistent**): `Record(u64)`,
`Transition(u64)`, `RecordSeq`, `TransitionSeq` — emergency-mode audit log.

**`DrainDataKey`** (`emergency_drain.rs`, **persistent**): `DrainRecord(u64)`,
`DrainSeq` — emergency-drain audit log and sequencing.

**`SlashStorageKey`** (`slash_history.rs`, **persistent**): `SlashCount(Address)`,
`SlashRecord(Address, u32)` — per-identity slash history.

**`ParameterKey`** (`parameters.rs`, **instance**): `ProtocolFeeBps`,
`AttestationFeeBps`, `WithdrawalCooldownSecs`, `SlashCooldownSecs`,
`BronzeThreshold`, `SilverThreshold`, `GoldThreshold`, `PlatinumThreshold`,
`MaxLeverage` — governance-tunable protocol parameters.

**`storage::DataKey`** (`storage.rs`, **instance**, ⚠️ see below):
`AcceptedTokens` — list of accepted deposit tokens.

TTL: instance keys use `STORAGE_TTL_EXTEND_TO = MAX_BOND_DURATION_SECONDS /
SECONDS_PER_LEDGER` (~`6_307_200` ledgers), half-life threshold, via
`bump_instance_ttl()`. Nonces use their own window:
`NONCE_TTL_THRESHOLD = 259_200`, `NONCE_TTL_EXTEND_TO = 518_400`. Persistent
entries (`claims`, `EmergencyDataKey`, `DrainDataKey`, `SlashStorageKey`)
extend via `crate::PERSISTENT_TTL_MAX / 2` → `crate::PERSISTENT_TTL_MAX` —
**this constant is referenced but not currently defined anywhere in the
crate** (see [Known issue](#known-issue-credence_bonds-datakey-has-duplicate-variant-declarations)); it needs a real declaration before the crate builds.

⚠️ **Two enums both named `DataKey` write into the same instance storage.**
`lib.rs::DataKey` and `storage.rs::DataKey` are separate `#[contracttype]`
declarations, both instance-scoped, in the same deployed contract. They
don't collide today — `storage::DataKey::AcceptedTokens` has no matching
name in `lib.rs::DataKey` — but the pairing is fragile: adding a bare unit
variant literally named `AcceptedTokens` to `lib.rs::DataKey` in the future
would silently alias `storage::DataKey::AcceptedTokens`'s ledger slot, and
the Rust compiler would not flag it (different Rust types, identical
`ScVal` encoding). New contributors adding storage-owning modules to this
crate should route new keys through the top-level `DataKey` (or a
`DataKey`-nested sub-key like `UpgradeKey`) rather than declaring another
freestanding `DataKey`-named enum in a submodule.

#### Known issue: `credence_bond`'s `DataKey` has duplicate variant declarations

As of this writing, `contracts/credence_bond/src/lib.rs`'s `DataKey` enum
declares `PauseSigner(Address)`, `PauseApproval(u64, Address)`,
`PauseApprovalCount(u64)`, and `PauseProposal(u64)` **twice each** — once
near the top of the enum body and again under the `// --- Pausable
functionality variants ---` section. Rust does not permit duplicate variant
names in one enum, so this is a build-blocking defect, most likely leftover
from a merge-conflict resolution. It predates and is out of scope for this
documentation change; the table above lists each variant once. Anyone
picking this up should remove one copy of each duplicate (keeping whichever
carries the doc comment) and also define the missing `PERSISTENT_TTL_MAX`
constant noted above.

### credence_delegation

Single enum `DataKey` (`contracts/credence_delegation/src/lib.rs`). The enum's
own doc comment already documents wire-stability rules and points at the
pinned fingerprint test in `tests/datakey_fingerprint.rs`.

| Variant | Tier | Used for |
|---|---|---|
| `Admin` | instance | Admin address |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u64)`, `PauseApproval(u64, Address)`, `PauseApprovalCount(u64)` | instance | Pause multisig |
| `Delegation(Address, Address, DelegationType)` | **persistent** | The delegation record (owner, delegate, type) |
| `Nonce(Address)` | **persistent** | Per-identity replay nonce |
| `Verifier(u32)` | instance | Scheme tag → verifier contract address (Ed25519=0, Secp256r1=1, MLDSA44=2) |
| `RevocationGracePeriod` | instance | Post-expiry grace window override (seconds) |

TTL: instance keys use the standard `31_536_000` / half-life pattern.
Persistent keys use a dynamic, expiry-derived TTL: `ttl_for_expiry()` (in
`nonce.rs`) converts `expires_at` into a ledger offset plus
`LEDGER_BUMP_BUFFER = 17_280`, capped at `MAX_TTL = 3_110_400`; `Nonce`
entries additionally floor at `MIN_NONCE_TTL = 518_400`.

### credence_errors

No storage keys. Pure shared library — error enum, role enum, and lease
helpers used by other contracts; it never calls `env.storage()` itself.

### credence_math

No storage keys. Pure arithmetic/rounding library with no `#[contract]`.

### credence_multisig

Single enum `DataKey` (`contracts/credence_multisig/src/multisig.rs`), all
**instance** storage.

| Variant | Used for |
|---|---|
| `Admin` | Admin address (can initialize, add/remove signers initially) |
| `Signer(Address)` | Multisig signer flag |
| `SignerCount` | Cached signer count |
| `Threshold` | Signatures required to execute a proposal |
| `ProposalCounter` | Next proposal id |
| `Proposal(u64)` | Proposal record |
| `Signature(u64, Address)` | Per-(proposal, signer) signature flag |
| `SignatureCount(u64)` | Cached signature count per proposal |
| `SignerList` | `Vec<Address>` enumeration of all signers |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u64)`, `PauseApproval(u64, Address)`, `PauseApprovalCount(u64)` | Pause multisig |
| `MaxPauseSigners` | Admin-configured cap on pause-signer count |
| `ExecutedOp(BytesN<32>)` | Deterministic op-hash replay guard for executed proposals |

TTL: standard `31_536_000` / half-life pattern.

### credence_registry

Two enums, both **instance** storage: `storage::DataKey` (the production
key set) and `idempotency::StorageKey` (currently unused by any production
entry point — see note).

**`storage::DataKey`** (`contracts/credence_registry/src/storage.rs`):

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u32)`, `PauseApproval(u32, Address)`, `PauseApprovalCount(u32)` | Pause multisig — note the proposal id here is `u32`, not `u64` as in every other contract's pause subsystem |
| `IdentityToBond(Address)` | Forward mapping: identity → registry entry |
| `BondToIdentity(Address)` | Reverse mapping: bond contract → identity |
| `RegisteredIdentities` | Insertion-ordered `Vec<Address>` of all registered identities |
| `AllowNonInterface(Address)` | Audit flag when a bond opted out of the interface check |
| `BondCodeHash` | Admin-pinned WASM code hash for trustless bond self-registration |

**`idempotency::StorageKey`** (`contracts/credence_registry/src/idempotency.rs`):
`Idempotent(BytesN<32>)` — maps a dedup key to a cached result. No
production call in `lib.rs` currently invokes `Idempotency::handle`; it is
exercised only by its own unit tests. Its single variant name does not
overlap with any `storage::DataKey` variant, so it introduces no collision
risk if wired in later — flagged here only so it isn't mistaken for dead
code to delete.

TTL: standard `31_536_000` / half-life pattern.

### credence_treasury

Single enum `DataKey` (`contracts/credence_treasury/src/treasury.rs`), all
**instance** storage.

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `Paused`, `PauseSigner(Address)`, `PauseSignerCount`, `PauseThreshold`, `PauseProposalCounter`, `PauseProposal(u64)`, `PauseApproval(u64, Address)`, `PauseApprovalCount(u64)` | Pause multisig |
| `TotalBalance` | Sum of all fund sources |
| `BalanceBySource(FundSource)` | Available balance per source (ProtocolFee, SlashedFunds) |
| `CumulativeReceived` | Lifetime cumulative total across all sources |
| `CumulativeReceivedBySource(FundSource)` | Lifetime cumulative per source |
| `Depositor(Address)` | Authorized-depositor flag |
| `Signer(Address)` | Multisig signer flag |
| `SignerCount` | Cached signer count |
| `Threshold` | Approvals required to execute a withdrawal |
| `ProposalCounter` | Next withdrawal-proposal id |
| `Proposal(u64)` | Withdrawal proposal record |
| `Approval(u64, Address)` | Per-(proposal, signer) approval flag |
| `ApprovalCount(u64)` | Cached approval count |
| `MinLiquidity` | Floor balance that must remain after a withdrawal |
| `Token` | Managed token address |
| `ProposalTtl` | Withdrawal-proposal expiry window (default 7 days) |
| `Corridor(Address)` | Admin-allowlisted settlement destination flag |

TTL: standard `31_536_000` / half-life pattern.

### fixed_duration_bond

Single enum `DataKey` (`contracts/fixed_duration_bond/src/lib.rs`, private to
the crate), all **instance** storage.

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `Token` | Deposit/payout token |
| `Bond(Address)` | Per-owner bond record (one active bond per address) |
| `FeeConfig` | Optional creation-fee config (treasury + bps) |
| `PenaltyConfig` | Optional early-exit penalty config (treasury + bps) |
| `AccumulatedFees` | Accrued creation fees pending collection |

TTL: no `extend_ttl` calls in this crate; instance entries rely on Soroban's
default TTL behavior rather than an explicit bump policy.

### templates

Single enum `DataKey` (`contracts/templates/src/lib.rs`), all **instance**
storage. This crate is a copy-paste starting point for new contracts, not a
deployed protocol contract.

| Variant | Used for |
|---|---|
| `Admin` | Contract administrator |
| `Record(Address)` | Per-identity record (`value`, `updated_at`, `expires_at`) |

TTL: no `extend_ttl` calls.

### timelock

Single enum `DataKey` (`contracts/timelock/src/lib.rs`), all **instance**
storage.

| Variant | Used for |
|---|---|
| `Admin` | Admin address |
| `OperationCounter` | Next operation id |
| `Operation(u64)` | Queued operation (op hash, eta, expiry, status) |
| `ExecutedOp(BytesN<32>)` | Replay guard keyed by op hash |

TTL: standard `31_536_000` / half-life pattern via `bump_instance_ttl()`.
Separately, `min_delay_seconds() = 86_400` and `GRACE_PERIOD = 86_400` are
business-logic timers (minimum queue delay, post-ETA execution window) —
not ledger-entry TTLs — and shouldn't be confused with the storage bump
policy above.

## Adding a new storage key safely

Follow this checklist when a contract needs a new piece of persistent
state:

1. **Add a variant to the contract's existing `DataKey` enum** (or the
   relevant sub-key enum, e.g. `UpgradeKey` in `credence_bond`) rather than
   declaring a new freestanding enum. One key enum per contract per storage
   purpose keeps the "what could collide with what" question trivial —
   you only ever have to scan one enum body.
2. **Append, don't insert.** Add the new variant at the end of the enum.
   Appending never changes any existing variant's encoding (see
   [How keys are encoded](#how-keys-are-encoded-the-one-fact-that-matters-for-collisions)
   above); inserting in the middle is equally safe *for the key encoding*
   but makes diffs harder to review, since it's not obvious at a glance
   whether a reviewer is looking at a real reorder or an insertion.
3. **Give it a unique variant name within that enum.** The compiler
   enforces this per-enum for free — that's the easy case. The unsafe case
   is a *second* enum in the same contract with an overlapping variant
   name written to the *same storage tier* (see the `credence_bond`
   ⚠️ note above for a live example). Before adding a new key-holding enum
   to a contract, grep that contract's `src/` for other `#[contracttype]`
   enums and confirm no variant name/shape overlaps.
4. **Pick the right storage tier.** Use `instance` for small,
   frequently-read config-like state; `persistent` for per-entity records
   that must survive independent of the contract instance's own bumping
   (and must be explicitly `extend_ttl`'d — see each contract's TTL policy
   above); `temporary` for data that is fine to expire and disappear.
   Instance-tier and persistent-tier writes cannot collide with each other
   even if the encoded key is identical, but mixing the same conceptual key
   across tiers is confusing — pick one tier per key and keep it there.
5. **Never rename or retype an existing variant.** Both operations move the
   key and orphan whatever is currently stored under the old encoding. See
   [datakey-fingerprint.md](datakey-fingerprint.md) for the full rule set
   and the fingerprint test that pins `credence_delegation`'s encodings
   (the same rule applies to `credence_bond`'s `DataKey`, which carries the
   identical stability doc-comment but does not yet have its own pinned
   fingerprint test).
6. **Document the new variant in place.** A short `///` doc comment above
   the variant stating what it stores and its value type (see the
   `PendingClaims(Address)`-style comments already present in
   `credence_bond::DataKey` for the pattern) is enough — no need to also
   restate it here unless the key introduces a new namespace or tier.
7. **If two contracts' storage is ever merged or a contract starts
   delegating storage to another module in-crate**, re-run the collision
   check in step 3 across the combined variant set, not just within the
   new module.

## Collision risk summary

| Contract | Same-tier, cross-enum collision risk |
|---|---|
| admin, arbitration, credence_delegation, credence_multisig, credence_treasury, fixed_duration_bond, templates, timelock | None — exactly one production key enum per storage tier. |
| credence_bond | ⚠️ `lib.rs::DataKey` and `storage.rs::DataKey` are two same-named enums both in instance storage (see contract section above). No current overlap, but no structural guard against a future one either. |
| credence_registry | None currently — `storage::DataKey` and `idempotency::StorageKey` have disjoint variant names; the latter is presently unused by production code. |
| credence_errors, credence_math | N/A — no storage. |
