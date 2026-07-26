# Known Simplifications

## Current Limitations

1. **No appeal mechanism** — Decisions are final once finalized
2. **No timelocks** — Disputes resolve immediately upon quorum
3. **Single dispute per identity per epoch** — Cannot have overlappingtes
4. **No appeal bond** — Appeals are free (not yet implemented)
5. **Manual dispute initiation** — No automated trigger
6. **No graceful shutdown** — Cannot pause arbitration
7. **No fee split** — All slashed XLM goes to protocol
8. **No arbitrator rotation** — Same arbitrators in all disputes

## Resolved

### 4. Slashed Funds Are Now Transferred to Treasury

Slashed funds are now transferred to the configured slash treasury on every `slash()` call via `token_integration::transfer_from_contract`. Slashing reverts with `ContractError::TreasuryNotConfigured` if no treasury has been set via `set_slash_treasury(admin, treasury)`.

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

## 7. get_all_identities() Has No Pagination

**Where:** `contracts/credence_registry/src/lib.rs`

**What:** `get_all_identities()` returns the full list of registered identity addresses in a single call. There is no pagination, cursor, or limit parameter.

**Impact:** As the registry grows, this call will consume increasing amounts of ledger read budget and may eventually exceed Soroban's per-transaction resource limits.

**Production path:** Add a `get_identities_page(offset: u32, limit: u32)` function and deprecate the unbounded variant. Off-chain indexers should use event-based discovery (`identity_registered` events) rather than polling `get_all_identities()`. See [registry.md](registry.md).

## 9. Arbitration Voting Weights Are Not Stake-Backed

**Where:** `contracts/arbitration/src/lib.rs`

**What:** Arbitrator voting weights are set by the admin via `register_arbitrator(arbitrator, weight)` as arbitrary integers. They are not derived from or backed by any on-chain stake or bond balance.

**Impact:** The admin can assign any weight to any address, making the voting system fully centralized. There is no economic cost to being an arbitrator and no slashing risk for bad votes.

**Production path:** Derive arbitrator weight from the arbitrator's bond balance (queried from `credence_bond` via cross-contract call), or require arbitrators to stake tokens into the arbitration contract. This creates economic alignment and makes the system permissionless. See [arbitration.md](arbitration.md).

---

## 11. Multisig Proposals Have No Expiry

**Where:** `contracts/credence_multisig/src/multisig.rs`

**What:** Multisig proposals remain open indefinitely once created. There is no deadline after which a proposal automatically fails or can be cancelled.

**Impact:** Stale proposals accumulate in storage. A proposal created months ago could be approved and executed long after the intended context has changed.

**Production path:** Add an `expires_at` field to proposals and reject approval or execution of expired proposals. See [multisig.md](multisig.md).

---

---

## Summary Table

| # | Simplification | Contract | Production Path |
|---|---------------|----------|-----------------|
| 1 | Token transfer stubbed in tests | credence_bond | Configure live USDC via `set_usdc_token` |
| 2 | Single-bond-per-contract-instance | credence_bond | Multi-bond map storage |
| 3 | Treasury is pure accounting, no token custody | credence_treasury | Add real token transfers on withdrawal |
| 6 | Early-exit penalty dropped if no treasury | credence_bond | Require treasury before `withdraw_early` |
| 7 | `get_all_identities()` unbounded | credence_registry | Add pagination; use event-based indexing |
| 9 | Arbitrator weights not stake-backed | credence_arbitration | Derive weight from bond balance |
| 11 | Multisig proposals have no expiry | credence_multisig | Add `expires_at` to proposals |
