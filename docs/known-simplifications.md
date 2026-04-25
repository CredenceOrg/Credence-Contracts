# Known Simplifications

This document consolidates all known simplifications, stubs, and intentional limitations in the Credence Contracts reference implementation. It is the single source of truth for anything that is deliberately incomplete or deferred.

Each entry describes what is simplified, why, and what a production implementation would do instead.

---

## 1. Token Transfer is Stubbed in credence_bond

**Where:** `contracts/credence_bond/src/`

**What:** The bond contract's token transfer calls (`transfer_from`, `transfer`) are wired to a Soroban token interface, but the reference implementation uses a mock/test token rather than a live USDC contract on mainnet. In tests, `Env::default()` with `mock_all_auths()` is used, meaning no real token approval or balance check occurs against a deployed token contract.

**Impact:** The accounting logic (bonded amounts, slashing, fees, penalties) is fully implemented and correct. Only the external token call is stubbed for testing purposes.

**Production path:** Configure a real USDC token address via `set_usdc_token(admin, token, network)` before deployment. The balance-delta check in `token_integration.rs` will then enforce transfer integrity against the live token. See [token-integration.md](token-integration.md).

---

## 2. Single-Bond-Per-Contract-Instance Storage Model

**Where:** `contracts/credence_bond/src/lib.rs`

**What:** The bond contract stores one bond per contract instance (keyed by a single storage slot), not a per-identity map. Each identity that wants a bond deploys its own contract instance.

**Impact:** This simplifies the storage model and avoids cross-identity data leakage, but it means the registry contract (`credence_registry`) is required to track which contract instance belongs to which identity. Batch operations across identities require iterating registry entries off-chain.

**Production path:** A multi-bond contract with a `Map<Address, IdentityBond>` storage layout would allow a single contract to serve many identities. The registry would still be useful for discovery but would not be strictly required for storage. See [registry.md](registry.md).

---

## 3. Treasury is a Pure Accounting System (No Token Custody)

**Where:** `contracts/credence_treasury/src/`

**What:** The treasury contract tracks fee balances and withdrawal records internally but does not hold tokens directly. `receive_fee()` accepts fee reports from bond contracts without an actual token transfer. `execute_withdrawal()` updates internal balance tracking without moving tokens.

**Impact:** Fee accounting is correct and auditable on-chain, but the treasury cannot actually disburse funds without an additional integration layer that connects the accounting records to a real token transfer.

**Production path:** The treasury should be extended to hold a token balance and execute real `transfer()` calls on `execute_withdrawal()`. The bond contract's fee collection path would then call `transfer` to the treasury address rather than just reporting. See [treasury.md](treasury.md).

---

## 4. Admin Role is Non-Transferable After Initialization (credence_bond)

**Where:** `contracts/credence_bond/src/lib.rs`, `contracts/credence_bond/src/access_control.rs`

**What:** In the bond contract, the admin address is set once at initialization and cannot be changed. There is no `transfer_admin` function on the bond contract itself (unlike `credence_registry`, which does support admin transfer).

**Impact:** If the admin key is lost or compromised, the contract cannot be re-administered without a contract upgrade or migration.

**Production path:** Add an `transfer_admin(current_admin, new_admin)` function with dual-auth (both addresses must sign) to allow safe key rotation. The `admin` contract (`contracts/admin/`) provides a more complete role management model that can be adopted. See [admin-roles.md](admin-roles.md).

---

## 5. Slashed Funds Are Not Transferred to Treasury

**Where:** `contracts/credence_bond/src/slashing.rs`

**What:** When a bond is slashed, `slashed_amount` is incremented and the withdrawable balance is reduced, but no tokens are actually moved. The slashed value sits locked in the contract with no mechanism to transfer it out.

**Impact:** Slashing correctly prevents the identity from withdrawing the slashed portion, but the protocol does not capture those funds. In a production system, slashed tokens would be transferred to the treasury or burned.

**Production path:** After updating `slashed_amount`, call `transfer(treasury, slash_amount)` to move the slashed tokens to the treasury address. This requires the treasury to be configured and the token integration to be live. See [slashing.md](slashing.md) and [treasury.md](treasury.md).

---

## 6. Early-Exit Penalty Transfer to Treasury is Conditional

**Where:** `contracts/credence_bond/src/early_exit_penalty.rs`

**What:** The early-exit penalty is calculated correctly and deducted from the withdrawal amount, but the penalty portion is only transferred to the treasury if a treasury address is configured. If no treasury is set, the penalty is silently dropped (the identity receives `amount - penalty` and the penalty is not sent anywhere).

**Impact:** In a test or unconfigured environment, penalty funds are effectively burned. The accounting is correct from the identity's perspective but the protocol does not capture the penalty revenue.

**Production path:** Require a treasury address to be set before `withdraw_early` is callable, or revert if no treasury is configured. See [early-exit.md](early-exit.md).

---

## 7. get_all_identities() Has No Pagination

**Where:** `contracts/credence_registry/src/lib.rs`

**What:** `get_all_identities()` returns the full list of registered identity addresses in a single call. There is no pagination, cursor, or limit parameter.

**Impact:** As the registry grows, this call will consume increasing amounts of ledger read budget and may eventually exceed Soroban's per-transaction resource limits.

**Production path:** Add a `get_identities_page(offset: u32, limit: u32)` function and deprecate the unbounded variant. Off-chain indexers should use event-based discovery (`identity_registered` events) rather than polling `get_all_identities()`. See [registry.md](registry.md).

---

## 8. Delegation Expiry is Not Enforced On-Chain at Write Time

**Where:** `contracts/credence_delegation/src/lib.rs`

**What:** When `delegate()` is called, `expires_at` must be a future timestamp. However, expired delegations are not automatically cleaned up — they remain in storage indefinitely. `is_valid_delegate()` correctly returns `false` for expired delegations, but the storage entry persists.

**Impact:** Storage grows unboundedly as expired delegations accumulate. There is no on-chain garbage collection.

**Production path:** Add a `cleanup_expired(owner, delegate, delegation_type)` function that anyone can call to remove expired entries and reclaim storage rent. Alternatively, use Soroban's TTL-based storage expiry for delegation entries. See [delegation.md](delegation.md).

---

## 9. Arbitration Voting Weights Are Not Stake-Backed

**Where:** `contracts/arbitration/src/lib.rs`

**What:** Arbitrator voting weights are set by the admin via `register_arbitrator(arbitrator, weight)` as arbitrary integers. They are not derived from or backed by any on-chain stake or bond balance.

**Impact:** The admin can assign any weight to any address, making the voting system fully centralized. There is no economic cost to being an arbitrator and no slashing risk for bad votes.

**Production path:** Derive arbitrator weight from the arbitrator's bond balance (queried from `credence_bond` via cross-contract call), or require arbitrators to stake tokens into the arbitration contract. This creates economic alignment and makes the system permissionless. See [arbitration.md](arbitration.md).

---

## 10. Timelock Has No Execution Guard Against Replays

**Where:** `contracts/timelock/src/`

**What:** The timelock contract queues and executes delayed operations, but executed operations are not permanently marked as consumed in a way that prevents re-queuing the same operation with the same parameters immediately after execution.

**Impact:** An admin could re-queue and re-execute the same operation multiple times if not careful. There is no nonce or execution receipt stored per operation hash.

**Production path:** Store a set of executed operation hashes and reject re-queuing any hash that has already been executed. See [timelock.md](timelock.md).

---

## 11. Multisig Proposals Have No Expiry

**Where:** `contracts/credence_multisig/src/multisig.rs`

**What:** Multisig proposals remain open indefinitely once created. There is no deadline after which a proposal automatically fails or can be cancelled.

**Impact:** Stale proposals accumulate in storage. A proposal created months ago could be approved and executed long after the intended context has changed.

**Production path:** Add an `expires_at` field to proposals and reject approval or execution of expired proposals. See [multisig.md](multisig.md).

---

## 12. No Cross-Contract Calls Between Bond and Registry

**Where:** `contracts/credence_bond/src/`, `contracts/credence_registry/src/`

**What:** The bond contract and registry contract are independent. The bond contract does not call the registry to register itself on creation, and the registry does not validate that a registered bond contract address is a genuine deployed bond contract.

**Impact:** Registration is a manual admin step. There is no automatic or trustless binding between a deployed bond contract and its registry entry. A malicious admin could register an arbitrary address as a bond contract.

**Production path:** The bond contract should call `registry.register(identity, self)` during `initialize()`, or the registry should verify the bond contract's code hash before accepting registration. See [registry.md](registry.md).

---

## Summary Table

| # | Simplification | Contract | Production Path |
|---|---------------|----------|-----------------|
| 1 | Token transfer stubbed in tests | credence_bond | Configure live USDC via `set_usdc_token` |
| 2 | Single-bond-per-contract-instance | credence_bond | Multi-bond map storage |
| 3 | Treasury is pure accounting, no token custody | credence_treasury | Add real token transfers on withdrawal |
| 4 | Admin non-transferable in bond contract | credence_bond | Add `transfer_admin` with dual-auth |
| 5 | Slashed funds not transferred to treasury | credence_bond | Call `transfer(treasury, slash_amount)` post-slash |
| 6 | Early-exit penalty dropped if no treasury | credence_bond | Require treasury before `withdraw_early` |
| 7 | `get_all_identities()` unbounded | credence_registry | Add pagination; use event-based indexing |
| 8 | Expired delegations not cleaned up | credence_delegation | TTL storage or explicit cleanup function |
| 9 | Arbitrator weights not stake-backed | credence_arbitration | Derive weight from bond balance |
| 10 | Timelock allows operation replay | timelock | Store executed operation hashes |
| 11 | Multisig proposals have no expiry | credence_multisig | Add `expires_at` to proposals |
| 12 | No cross-contract bond↔registry binding | credence_bond + registry | Auto-register on bond init or verify code hash |
