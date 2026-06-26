# Testnet Deployment Runbook

This runbook covers the full lifecycle for deploying the Credence contract suite to Soroban testnet: build → size check → deploy → initialize → cross-contract wiring → verification. Follow the steps in order; each section depends on the previous one completing successfully.

Related docs (read these, not duplicated here):
- [Reproducible WASM builds and hash verification](wasm-reproducibility.md)
- [Contract architecture and state layout](architecture.md)
- [Known simplifications and production gaps](known-simplifications.md)

---

## Prerequisites

- **Rust 1.89.0** — pinned in [`rust-toolchain.toml`](../rust-toolchain.toml); `rustup` installs it automatically on first `cargo` invocation. (The pin was raised from 1.85.1 due to soroban-client transitive dependencies on `icu_*` 2.2.0 and `zip` 8.x, which require rustc 1.86 and 1.88 respectively.)
- **Soroban CLI** — `cargo install soroban-cli`. See the [Stellar setup guide](https://developers.stellar.org/docs/smart-contracts/getting-started/setup) for version requirements.
- **Testnet account** — fund a keypair via [Friendbot](https://friendbot.stellar.org/?addr=<YOUR_PUBLIC_KEY>).
- **Testnet USDC asset contract** — the treasury contract requires a real Stellar asset contract address at initialization time, not a mock. Use the canonical testnet USDC or deploy a test token first.

See [README.md — Prerequisites](../README.md#prerequisites) for the full environment setup checklist.

---

## Step 1 — Build WASM Artifacts

Build only the deployable WASM crates. The workspace also contains library crates (`credence_errors`, `credence_math`) and scaffolding (`templates`, `credence_admin_cli`) that are not deployed to the chain.

```bash
cargo build \
  --target wasm32-unknown-unknown \
  --release \
  --locked \
  -p credence_bond \
  -p credence_delegation \
  -p credence_treasury \
  -p admin \
  -p credence_multisig \
  -p arbitration \
  -p timelock
```

`--locked` is required: it enforces `Cargo.lock` and ensures byte-for-byte reproducible output. Omitting it allows dependency drift that changes the resulting WASM hash.

For SHA-256 hash verification before deployment, see [docs/wasm-reproducibility.md](wasm-reproducibility.md).

---

## Step 2 — Size Check

The build profile (`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = "symbols"`) is already configured in the root `Cargo.toml`. Verify every artifact stays within the 64 KB limit:

```bash
bash scripts/check_wasm_size.sh
```

If the script exits non-zero, it prints the name of the offending contract and its size. There is no further reduction available without code changes — do not proceed to deploy until the check passes.

Artifacts are located at:
```
target/wasm32-unknown-unknown/release/credence_bond.wasm
target/wasm32-unknown-unknown/release/credence_delegation.wasm
target/wasm32-unknown-unknown/release/credence_treasury.wasm
target/wasm32-unknown-unknown/release/admin.wasm
target/wasm32-unknown-unknown/release/credence_multisig.wasm
target/wasm32-unknown-unknown/release/credence_arbitration.wasm
target/wasm32-unknown-unknown/release/timelock.wasm
```

---

## Step 3 — Deploy and Initialize

Each contract is standalone at initialization — no contract calls another during `initialize`. Cross-contract wiring is done after all contracts are deployed (Step 4).

Deploy in the order listed below. Within each contract block: deploy first (capture the contract ID), then immediately run all initialization and config calls before moving to the next contract.

### Re-initialization Safety

Some contracts guard against being initialized twice; others do not. **Never re-run `initialize` on a deployed contract** unless you understand the consequence.

| Contract | Guard | Behaviour if `initialize` is called again |
|---|---|---|
| `admin` | `DataKey::Initialized` flag | Panics — `AlreadyInitialized` |
| `credence_arbitration` | `has(&DataKey::Admin)` | Returns `AlreadyInitialized` error |
| `timelock` | `has(&DataKey::Admin)` | Panics |
| `credence_bond` | implicit | Panics — `AlreadyInitialized` |
| `credence_delegation` | `has(&DataKey::Admin)` | Panics |
| `credence_treasury` | **none** | **Overwrites existing state — dangerous** ⚠️ |
| `credence_multisig` | **none** | **Overwrites existing state — dangerous** ⚠️ |

---

### 3a — admin

```bash
ADMIN_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/admin.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "ADMIN_CONTRACT_ID=$ADMIN_CONTRACT_ID"
```

Initialize with the super admin address, minimum admin count, and maximum admin count:

```bash
soroban contract invoke \
  --id "$ADMIN_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --super_admin "$SUPER_ADMIN" \
  --min_admins 1 \
  --max_admins 10
```

Optionally add an operator-level admin immediately after:

```bash
soroban contract invoke \
  --id "$ADMIN_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- add_admin \
  --caller "$SUPER_ADMIN" \
  --new_admin "$DEPLOY_ADMIN" \
  --role '{"Operator": null}'
```

---

### 3b — timelock

```bash
TIMELOCK_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/timelock.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "TIMELOCK_CONTRACT_ID=$TIMELOCK_CONTRACT_ID"
```

```bash
soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN"
```

No post-init configuration required. The minimum queue delay is 86 400 seconds (24 hours); this is hardcoded and cannot be changed after deployment.

---

### 3c — credence_multisig

```bash
MULTISIG_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_multisig.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "MULTISIG_CONTRACT_ID=$MULTISIG_CONTRACT_ID"
```

Signers and threshold are set at initialization. `threshold` must be `> 0` and `<= len(signers)`.

```bash
soroban contract invoke \
  --id "$MULTISIG_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN" \
  --signers "[\"$SIGNER_1\",\"$SIGNER_2\",\"$SIGNER_3\"]" \
  --threshold "$MULTISIG_THRESHOLD"
```

⚠️ This contract has **no double-init guard**. If `initialize` is called again, it overwrites signers and threshold. Protect against accidental re-invocation at the ops level after this step.

---

### 3d — credence_arbitration

```bash
ARBITRATION_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_arbitration.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "ARBITRATION_CONTRACT_ID=$ARBITRATION_CONTRACT_ID"
```

`initialize` returns `Result<(), ArbitrationError>` — the CLI exit code reflects success or failure.

```bash
soroban contract invoke \
  --id "$ARBITRATION_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN"
```

Register each arbitrator immediately after init. `weight` must be `> 0`; weights are admin-assigned integers, not derived from on-chain stake (see [known-simplifications.md #9](known-simplifications.md)).

```bash
soroban contract invoke \
  --id "$ARBITRATION_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- register_arbitrator \
  --arbitrator "<ARBITRATOR_ADDRESS>" \
  --weight 100
```

Repeat for each arbitrator.

---

### 3e — credence_treasury

The treasury requires a live token address at init. `USDC_TOKEN_ADDRESS` must be a real Stellar asset contract — not the mock token used in tests (see [known-simplifications.md #1](known-simplifications.md)).

```bash
TREASURY_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_treasury.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "TREASURY_CONTRACT_ID=$TREASURY_CONTRACT_ID"
```

```bash
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN" \
  --token "$USDC_TOKEN_ADDRESS"
```

⚠️ `credence_treasury` has **no double-init guard** — a second `initialize` call overwrites all state.

Configure multi-sig signers and threshold:

```bash
# Add each withdrawal signer
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- add_signer \
  --signer "$SIGNER_1"

soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- add_signer \
  --signer "$SIGNER_2"

soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- add_signer \
  --signer "$SIGNER_3"

# Set withdrawal threshold
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- set_threshold \
  --threshold "$MULTISIG_THRESHOLD"
```

The bond contract (`$BOND_CONTRACT_ID`) will be added as an authorized depositor in the wiring step (Step 4) once it is deployed.

> **Known limitation:** In the reference implementation, `receive_fee` updates internal accounting without performing an actual token transfer. The treasury is a pure accounting system. See [known-simplifications.md #3 and #5](known-simplifications.md) before targeting mainnet.

---

### 3f — credence_bond

```bash
BOND_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_bond.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "BOND_CONTRACT_ID=$BOND_CONTRACT_ID"
```

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN"
```

Configure early-exit parameters (sets the treasury address the bond contract will report fees to):

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- set_early_exit_config \
  --admin "$DEPLOY_ADMIN" \
  --treasury "$TREASURY_CONTRACT_ID" \
  --penalty_bps "$EARLY_EXIT_PENALTY_BPS"
```

Configure weighted-attestation parameters (basis-point multiplier and maximum weight cap):

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- set_weight_config \
  --admin "$DEPLOY_ADMIN" \
  --multiplier_bps 10000 \
  --max_weight 1000
```

Register attesters (repeat for each):

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- register_attester \
  --attester "<ATTESTER_ADDRESS>"
```

---

### 3g — credence_delegation

```bash
DELEGATION_CONTRACT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_delegation.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "DELEGATION_CONTRACT_ID=$DELEGATION_CONTRACT_ID"
```

```bash
soroban contract invoke \
  --id "$DELEGATION_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- initialize \
  --admin "$DEPLOY_ADMIN"
```

Optionally register cryptographic verifiers for each supported signature scheme (`scheme`: `0` = Ed25519, `1` = Secp256r1, `2` = MLDSA44):

```bash
soroban contract invoke \
  --id "$DELEGATION_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- register_verifier \
  --admin "$DEPLOY_ADMIN" \
  --scheme 0 \
  --verifier_id "<VERIFIER_CONTRACT_ADDRESS>"
```

---

## Step 4 — Cross-Contract Wiring

With all contracts deployed, the following wiring calls bind the suite together. Calls in rows 2–4 were issued during Step 3 as part of per-contract configuration; they are listed here again as the canonical wiring record and to confirm ordering.

| Wiring call | Caller | Target address argument | Must happen after |
|---|---|---|---|
| `treasury.add_depositor(bond_id)` | admin | `$BOND_CONTRACT_ID` | bond deployed (Step 3f) |
| `bond.set_early_exit_config(admin, treasury_id, penalty_bps)` | admin | `$TREASURY_CONTRACT_ID` | treasury deployed (Step 3e) |
| `arbitration.register_arbitrator(addr, weight)` | admin | each arbitrator address | arbitration init (Step 3d) |
| `bond.set_weight_config(admin, multiplier_bps, max_weight)` *(optional)* | admin | — | bond init (Step 3f) |

The delegation and timelock contracts have no inbound cross-contract wiring required at deploy time.

### 4a — Authorize the bond contract as a treasury depositor

This is the only wiring call that cannot be done during per-contract initialization because `$BOND_CONTRACT_ID` does not exist until Step 3f completes.

```bash
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- add_depositor \
  --depositor "$BOND_CONTRACT_ID"
```

This authorizes the bond contract to call `treasury.receive_fee()` when collecting early-exit penalties. Without this call, fee-reporting will be rejected by the treasury.

---

## Step 5 — Verification Checklist

Run each read-only getter to confirm the expected state. None of these calls mutate state.

### credence_bond

```bash
# Confirm admin and early-exit config (treasury address + penalty)
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- describe_config
```

```bash
# Confirm an attester is registered
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- is_attester \
  --attester "<ATTESTER_ADDRESS>"
# Expected: true
```

### credence_treasury

```bash
# Confirm bond contract is an authorized depositor
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --network "$NETWORK" \
  -- is_depositor \
  --address "$BOND_CONTRACT_ID"
# Expected: true

# Confirm withdrawal threshold
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_threshold
# Expected: $MULTISIG_THRESHOLD

# Confirm admin
soroban contract invoke \
  --id "$TREASURY_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_admin
```

### credence_multisig

```bash
soroban contract invoke \
  --id "$MULTISIG_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_signers

soroban contract invoke \
  --id "$MULTISIG_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_threshold
```

### admin

```bash
soroban contract invoke \
  --id "$ADMIN_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_owner

soroban contract invoke \
  --id "$ADMIN_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_all_admins
```

### timelock

```bash
soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_admin
```

### credence_delegation (smoke test)

```bash
soroban contract invoke \
  --id "$DELEGATION_CONTRACT_ID" \
  --network "$NETWORK" \
  -- is_paused
# Expected: false
```

### credence_arbitration

There is no `get_arbitrators` list getter on `credence_arbitration`. Confirm registration by submitting a test dispute and verifying that an arbitrator's vote is accepted, or by indexing the `arbitrator_registered` events emitted during `register_arbitrator` calls.

---

## Step 6 — Rollback and Redo

**Soroban contracts are immutable once deployed.** Redeploying a contract creates a new contract ID, which invalidates any cross-contract wiring that referenced the old ID.

| Scenario | Action |
|---|---|
| Wrong config setter value | Call the setter again — setters are not one-shot; admin can update at any time. |
| Wrong `initialize` argument on a guarded contract | The contract panics on re-init. Deploy a new instance; update `deploy_addresses.env`; re-run all wiring steps. |
| Wrong `initialize` argument on `credence_treasury` or `credence_multisig` | These have no guard — calling `initialize` again overwrites state. Do this only if no funds or proposals exist. Then re-run wiring. |
| Full re-deploy of one contract | Update its `$CONTRACT_ID` variable in `deploy_addresses.env`. Re-run all wiring calls that reference it (both as caller and as argument). |

Keep `deploy_addresses.env` in a secure location. It is the source of truth for all contract IDs across the deploy and is required to run any subsequent admin call.

---

## Summary: Deployment Order

```
Step 3a  deploy + init   admin
Step 3b  deploy + init   timelock
Step 3c  deploy + init   credence_multisig  (signers + threshold at init)
Step 3d  deploy + init   credence_arbitration  (+register_arbitrator calls)
Step 3e  deploy + init   credence_treasury  (+add_signer, set_threshold)
Step 3f  deploy + init   credence_bond  (+set_early_exit_config, set_weight_config, register_attester)
Step 3g  deploy + init   credence_delegation  (+register_verifier optional)
Step 4a  wire            treasury.add_depositor($BOND_CONTRACT_ID)
Step 5   verify          read-back getters on all contracts
```

---

## Appendix — `deploy_addresses.env` Template

Populate this file as you complete each deploy step. Source it before running any `soroban contract invoke` command.

```bash
# deploy_addresses.env
export NETWORK="testnet"
export ADMIN_KEY="<secret-key-alias-or-path>"

# Contract IDs — fill in as each contract is deployed
export ADMIN_CONTRACT_ID=""
export TIMELOCK_CONTRACT_ID=""
export MULTISIG_CONTRACT_ID=""
export ARBITRATION_CONTRACT_ID=""
export TREASURY_CONTRACT_ID=""
export BOND_CONTRACT_ID=""
export DELEGATION_CONTRACT_ID=""

# Protocol config
export USDC_TOKEN_ADDRESS=""       # Live testnet Stellar asset contract
export EARLY_EXIT_PENALTY_BPS=500  # 5% early-exit penalty (example)
export MULTISIG_THRESHOLD=2        # Require 2-of-N signers
export SIGNER_1=""
export SIGNER_2=""
export SIGNER_3=""
export SUPER_ADMIN=""              # Address of the initial super admin
export DEPLOY_ADMIN=""             # Address used for per-contract admin roles
```

```bash
source deploy_addresses.env
```

Keep this file in a secure location. It is the source of truth for all contract IDs and is required to run any subsequent admin or wiring call.
