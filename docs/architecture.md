# Credence Smart Contract Architecture

## Table of Contents
- [Overview](#overview)
- [System Architecture](#system-architecture)
- [Contract Components](#contract-components)
- [Data Flow](#data-flow)
- [Storage Patterns](#storage-patterns)
- [Security Architecture](#security-architecture)
- [Integration Patterns](#integration-patterns)

## Overview

The Credence protocol is a comprehensive trust and reputation system built on Stellar using Soroban smart contracts. The architecture consists of modular, interacting contracts that collectively enable decentralized identity bonding, attestations, governance, and dispute resolution.

### Design Principles

1. **Modularity**: Each contract has a focused responsibility
2. **Interoperability**: Contracts communicate through well-defined interfaces
3. **Security-First**: Reentrancy guards, access controls, and validation at every layer
4. **Upgradability**: Admin-controlled upgrades with governance oversight
5. **Gas Efficiency**: Optimized storage patterns and batch operations
6. **Auditability**: Comprehensive event emission for all state changes

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Credence Protocol Layer                          │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                   ┌────────────────┼────────────────┐
                   │                │                │
         ┌─────────▼────────┐  ┌───▼────────┐  ┌───▼──────────┐
         │  User Interface  │  │   Oracle   │  │ Off-Chain    │
         │   Applications   │  │  Services  │  │  Indexers    │
         └─────────┬────────┘  └───┬────────┘  └───┬──────────┘
                   │                │                │
┌──────────────────┴────────────────┴────────────────┴──────────────────┐
│                        Contract Layer (Soroban)                        │
├────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌──────────────────┐      ┌──────────────────┐                      │
│  │  Admin Contract  │◄────►│ Error Contract   │                      │
│  │  (Role Mgmt)     │      │ (Standard Errors)│                      │
│  └────────┬─────────┘      └──────────────────┘                      │
│           │                                                            │
│  ┌────────▼────────────────────────────────────────────────┐         │
│  │           Credence Bond Contract (Core)                  │         │
│  ├──────────────────────────────────────────────────────────┤         │
│  │ Modules:                                                  │         │
│  │ • Bond Management      • Governance Approval              │         │
│  │ • Attestations         • Slashing & Slash History         │         │
│  │ • Weighted Attestations • Evidence Storage               │         │
│  │ • Tiered Bonds         • Access Control                   │         │
│  │ • Rolling Bonds        • Math & Safety Checks             │         │
│  │ • Early Exit Penalty   • Batch Operations                 │         │
│  │ • Fee Management       • Reentrancy Protection            │         │
│  └──────┬─────────────┬─────────────┬────────────┬──────────┘         │
│         │             │             │            │                     │
│  ┌──────▼──────┐ ┌───▼──────┐ ┌───▼────────┐ ┌─▼───────────┐        │
│  │  Registry   │ │ Treasury │ │ Delegation │ │ Arbitration │        │
│  │  Contract   │ │ Contract │ │ Contract   │ │  Contract   │        │
│  └─────────────┘ └──────────┘ └────────────┘ └─────────────┘        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                   ┌────────────────┼────────────────┐
                   │                │                │
            ┌──────▼──────┐  ┌─────▼──────┐  ┌─────▼──────┐
            │   Stellar   │  │   Token    │  │  Storage   │
            │    Core     │  │  Contract  │  │   Layer    │
            └─────────────┘  └────────────┘  └────────────┘
```

### Contract Interaction Flow

```
User/DApp
    │
    ├──► Registry Contract ───► Lookup bond contract for identity
    │         │
    │         └──► Returns bond contract address
    │
    ├──► Bond Contract (Primary Entry Point)
    │         │
    │         ├──► Create Bond (with fee calculation)
    │         │        │
    │         │        └──► Treasury Contract (receives fees)
    │         │
    │         ├──► Add/Revoke Attestations
    │         │
    │         ├──► Slash Request
    │         │        │
    │         │        ├──► Governance Module (approval voting)
    │         │        │
    │         │        └──► Evidence Storage (IPFS hashes)
    │         │
    │         └──► Dispute Initiation
    │                  │
    │                  └──► Arbitration Contract (weighted voting)
    │
    ├──► Delegation Contract
    │         │
    │         └──► Delegate attestation/management rights
    │
    └──► Admin Contract
              │
              └──► Role management and access control
```

## Contract Components

### 1. Credence Bond Contract (Core)

**Purpose**: Central contract managing identity bonds, attestations, and core protocol logic.

**Key Responsibilities**:
- Bond lifecycle management (create, top-up, extend, withdraw)
- Attestation system (add, revoke, weighted attestations)
- Slashing mechanism with governance approval
- Fee collection and distribution
- Tiered bond system (Bronze/Silver/Gold/Platinum)
- Rolling bonds with notice periods
- Early exit penalties
- Batch operations for gas efficiency
- Evidence storage for slash proposals
- Access control (admin/verifier roles)

**Storage Structure**:
```rust
enum DataKey {
    Admin,                              // Admin address
    Bond,                               // Current bond state
    Token,                              // ERC-20 token for bonding

    // Attestations
    Attester(Address),                  // Registered attesters
    Attestation(u64),                   // Attestation records by ID
    AttestationCounter,                 // Auto-incrementing ID
    SubjectAttestations(Address),       // Attestations for an identity
    SubjectAttestationCount(Address),   // Count per identity
    AttesterStake(Address),             // Stake for weighted attestations

    // Security
    Nonce(Address),                     // Replay protection

    // Governance
    GovernanceNextProposalId,           // Proposal ID counter
    GovernanceProposal(u64),            // Slash proposals
    GovernanceVote(u64, Address),       // Votes on proposals
    GovernanceDelegate(Address),        // Vote delegation
    GovernanceGovernors,                // Governor addresses
    GovernanceQuorumBps,                // Quorum threshold (bps)
    GovernanceMinGovernors,             // Minimum governor count

    // Fees
    FeeTreasury,                        // Treasury contract address
    FeeBps,                             // Fee in basis points

    // Evidence
    EvidenceCounter,                    // Evidence ID counter
    Evidence(u64),                      // Evidence records
    ProposalEvidence(u64),              // Evidence linked to proposals
    HashExists(String),                 // Hash uniqueness tracking
}
```

**Main Data Structures**:
```rust
struct IdentityBond {
    identity: Address,
    bonded_amount: i128,
    bond_start: u64,
    bond_duration: u64,
    slashed_amount: i128,
    active: bool,
    is_rolling: bool,
    withdrawal_requested_at: u64,
    notice_period_duration: u64,
}

struct Attestation {
    id: u64,
    attester: Address,
    subject: Address,
    data: String,
    timestamp: u64,
    weight: u64,
    revoked: bool,
}

struct Evidence {
    id: u64,
    proposal_id: u64,
    hash: String,
    hash_type: EvidenceType,  // IPFS, SHA256, Other
    description: Option<String>,
    submitted_by: Address,
    submitted_at: u64,
}
```

### 2. Registry Contract

**Purpose**: Maintains mapping between identities and their bond contract addresses.

**Key Responsibilities**:
- Register identity-to-bond mappings
- Bidirectional lookup (identity ↔ bond contract)
- Track registration status
- Support activation/deactivation
- Idempotency for duplicate registrations

**Storage Structure**:
```rust
enum DataKey {
    Admin,                              // Admin address
    IdentityToBond(Address),            // Identity → RegistryEntry
    BondToIdentity(Address),            // Bond contract → Identity
    RegisteredIdentities,               // List of all identities
}

struct RegistryEntry {
    identity: Address,
    bond_contract: Address,
    registered_at: u64,
    active: bool,
}
```

**Usage Pattern**:
```
1. User creates bond contract
2. Bond contract calls Registry.register_identity()
3. Registry stores bidirectional mapping
4. Future lookups use Registry.get_bond_contract(identity)
```

### 3. Treasury Contract

**Purpose**: Multi-signature treasury for collecting and managing protocol fees.

**Key Responsibilities**:
- Receive bond creation fees
- Multi-sig withdrawal proposals
- Signer management with thresholds
- Proposal voting and execution
- Fund tracking by source

**Storage Structure**:
```rust
enum DataKey {
    Admin,                              // Admin address
    Signers,                            // List of authorized signers
    Threshold,                          // Required signatures
    NextProposalId,                     // Proposal counter
    Proposal(u64),                      // Withdrawal proposals
    Approvals(u64),                     // Approval tracking per proposal
    FundSources,                        // Track fund origins
}

struct WithdrawalProposal {
    id: u64,
    recipient: Address,
    amount: i128,
    proposer: Address,
    approvals: u32,
    executed: bool,
    created_at: u64,
}
```

**Multi-Sig Flow**:
```
1. Signer proposes withdrawal
2. Other signers approve via propose_approve()
3. Once threshold met, proposer executes
4. Funds transferred, proposal marked executed
```

### 4. Delegation Contract

**Purpose**: Enable users to delegate attestation and management rights to other addresses.

**Key Responsibilities**:
- Create delegations with expiry
- Support different delegation types (Attestation, Management)
- Validate delegation status
- Revoke delegations
- Track delegation history

**Storage Structure**:
```rust
enum DataKey {
    Admin,
    Delegation(Address, Address, DelegationType),  // (owner, delegate, type)
}

struct Delegation {
    owner: Address,
    delegate: Address,
    delegation_type: DelegationType,
    expires_at: u64,
    revoked: bool,
}

enum DelegationType {
    Attestation,  // Can create attestations on behalf of owner
    Management,   // Can manage bond settings
}
```

**Delegation Flow**:
```
1. Owner calls delegate(delegate_addr, type, expiry)
2. Delegate can act on behalf of owner
3. Bond contract checks is_valid_delegate() before allowing action
4. Owner can revoke anytime via revoke_delegation()
```

### 5. Arbitration Contract

**Purpose**: Weighted voting system for dispute resolution.

**Key Responsibilities**:
- Create disputes for arbitration
- Register arbitrators with voting weights
- Conduct weighted voting
- Resolve disputes based on vote outcomes
- Prevent double voting

**Storage Structure**:
```rust
enum DataKey {
    Admin,
    Arbitrator(Address),                // Arbitrator → weight
    Dispute(u64),                       // Dispute records
    DisputeCounter,                     // Dispute ID counter
    DisputeVotes(u64),                  // Outcome → total weight
    VoterCasted(u64, Address),          // Track who voted
}

struct Dispute {
    id: u64,
    creator: Address,
    description: String,
    voting_start: u64,
    voting_end: u64,
    resolved: bool,
    outcome: u32,  // 0 = unresolved/tie, >0 = specific outcome
}
```

**Arbitration Flow**:
```
1. User creates dispute via create_dispute()
2. Arbitrators vote with their weights
3. Votes accumulate per outcome
4. After voting period, resolve_dispute() determines winner
5. Outcome recorded on-chain
```

### 6. Admin Contract

**Purpose**: Hierarchical role-based access control across the protocol.

**Key Responsibilities**:
- Define role hierarchy (SuperAdmin > Admin > Operator)
- Assign and revoke roles
- Prevent last admin removal
- Track role changes
- Provide role validation

**Storage Structure**:
```rust
enum DataKey {
    AdminList,                          // All admin addresses
    AdminInfo(Address),                 // Address → AdminInfo
    RoleAdmins(AdminRole),              // Role → addresses with that role
}

struct AdminInfo {
    address: Address,
    role: AdminRole,
    assigned_at: u64,
    assigned_by: Address,
    active: bool,
}

enum AdminRole {
    SuperAdmin = 3,
    Admin = 2,
    Operator = 1,
}
```

### 7. Credence Errors Contract

**Purpose**: Standardized error codes across all contracts.

**Key Responsibilities**:
- Define protocol-wide error codes
- Provide error messages
- Enable consistent error handling

## Data Flow

### Bond Creation Flow

```
┌─────────┐
│  User   │
└────┬────┘
     │ 1. create_bond(amount, duration)
     │
┌────▼──────────────────────────────────────────────────────────┐
│  Bond Contract                                                 │
│                                                                │
│  2. Calculate fee (amount * fee_bps / 10000)                  │
│  3. Validate parameters (amount > 0, duration > 0)            │
│  4. Check no existing bond                                     │
│                                                                │
│  5. Transfer tokens from user                                  │
│     ├─► Token.transfer_from(user, contract, amount + fee)    │
│     │                                                          │
│  6. Send fee to treasury                                       │
│     ├─► Token.transfer(contract, treasury, fee)              │
│     └─► Treasury.receive_fee(fee, user)                      │
│                                                                │
│  7. Create bond record                                         │
│     └─► Storage.set(Bond, IdentityBond{...})                 │
│                                                                │
│  8. Determine tier (Bronze/Silver/Gold/Platinum)              │
│                                                                │
│  9. Register in Registry                                       │
│     └─► Registry.register_identity(identity, bond_contract)  │
│                                                                │
│  10. Emit bond_created event                                   │
└────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────┐
│ Bond Created    │
│ Registry Updated│
│ Fee Collected   │
└─────────────────┘
```

### Attestation Flow

```
┌──────────┐
│ Attester │
└────┬─────┘
     │ 1. add_attestation(subject, data, weight)
     │
┌────▼──────────────────────────────────────────────────────────┐
│  Bond Contract                                                 │
│                                                                │
│  2. Check if attester is registered                           │
│     └─► Storage.has(Attester(attester))                      │
│                                                                │
│  3. Generate nonce for replay protection                       │
│     └─► nonce = Storage.get(Nonce(subject)) + 1              │
│                                                                │
│  4. Validate weight (if weighted attestations enabled)        │
│     └─► weight <= attester_stake                             │
│                                                                │
│  5. Create attestation record                                  │
│     ├─► attestation_id = counter++                           │
│     ├─► attestation = Attestation{...}                       │
│     └─► Storage.set(Attestation(id), attestation)           │
│                                                                │
│  6. Link to subject                                            │
│     ├─► subject_attestations = get(SubjectAttestations)     │
│     ├─► subject_attestations.push(attestation_id)           │
│     └─► Storage.set(SubjectAttestations(subject), list)     │
│                                                                │
│  7. Increment subject attestation count                        │
│                                                                │
│  8. Emit attestation_added event                               │
└────────────────────────────────────────────────────────────────┘
     │
     ▼
┌────────────────────┐
│ Attestation Stored │
│ Subject Updated    │
│ Nonce Incremented  │
└────────────────────┘
```

### Slash Proposal Flow (with Governance)

```
┌────────┐
│ Admin/ │
│Governor│
└───┬────┘
    │ 1. propose_slash(amount, evidence)
    │
┌───▼──────────────────────────────────────────────────────────┐
│  Bond Contract - Governance Module                           │
│                                                               │
│  2. Validate proposer is governor                            │
│     └─► is_governor(proposer)                               │
│                                                               │
│  3. Create slash proposal                                     │
│     ├─► proposal_id = next_proposal_id++                    │
│     └─► proposal = SlashProposal{                           │
│             id, amount, proposer,                            │
│             status: Pending, votes: 0                        │
│         }                                                     │
│                                                               │
│  4. Store evidence (if provided)                              │
│     └─► Evidence.submit_evidence(proposal_id, hash, type)   │
│                                                               │
│  5. Emit proposal_created event                               │
└───────────────────────────────────────────────────────────────┘
    │
    │ Governors vote
    │
┌───▼──────────────────────────────────────────────────────────┐
│  Voting Phase                                                 │
│                                                               │
│  Governors call: vote_slash_proposal(proposal_id, approve)   │
│                                                               │
│  For each vote:                                               │
│  ├─► Check governor status                                   │
│  ├─► Check not double voting                                 │
│  ├─► Record vote                                             │
│  └─► Increment approval count if approved                    │
│                                                               │
│  Check quorum:                                                │
│  └─► approvals / total_governors >= quorum_bps              │
└───────────────────────────────────────────────────────────────┘
    │
    │ Quorum reached
    │
┌───▼──────────────────────────────────────────────────────────┐
│  Execution Phase                                              │
│                                                               │
│  Proposer calls: execute_slash_proposal(proposal_id)         │
│                                                               │
│  1. Verify quorum reached                                     │
│  2. Update bond:                                              │
│     ├─► slashed_amount += slash_amount                      │
│     ├─► Ensure slashed_amount <= bonded_amount              │
│     └─► Update tier if needed                               │
│                                                               │
│  3. Record in slash history                                   │
│  4. Mark proposal as Executed                                 │
│  5. Emit slash_executed event                                 │
└───────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────┐
│ Bond Slashed     │
│ History Recorded │
│ Tier Updated     │
└──────────────────┘
```

### Batch Bond Creation Flow

```
┌─────────┐
│  Admin  │
└────┬────┘
     │ create_batch_bonds(Vec<BatchBondParams>)
     │
┌────▼──────────────────────────────────────────────────────────┐
│  Bond Contract - Batch Module                                 │
│                                                                │
│  Validation Phase (Atomic - all or nothing):                  │
│  ├─► Check params_list not empty                             │
│  ├─► For each bond:                                           │
│  │    ├─► Validate amount > 0, duration > 0                  │
│  │    ├─► Check no duplicate identities in batch             │
│  │    └─► Check bond doesn't already exist                   │
│  └─► Calculate total amount (with overflow check)            │
│                                                                │
│  Creation Phase (if all valid):                               │
│  ├─► For each bond in batch:                                 │
│  │    ├─► Create IdentityBond record                         │
│  │    ├─► Register in Registry                               │
│  │    ├─► Calculate and collect fees                         │
│  │    └─► Emit individual bond_created event                 │
│  │                                                            │
│  └─► Emit batch_bonds_created event with count               │
│                                                                │
│  Gas Optimization:                                             │
│  └─► Single transaction for multiple bonds                   │
│      (reduces gas cost vs individual creates)                 │
└────────────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────┐
│ Multiple Bonds       │
│ Created Atomically   │
│ Batch Event Emitted  │
└──────────────────────┘
```

## Storage Patterns

### 1. Instance Storage (Contract State)

Used for frequently accessed, contract-scoped data:
- Admin addresses
- Configuration parameters
- Counters
- Current bond state

**Pattern**:
```rust
e.storage().instance().set(&DataKey::Admin, &admin);
let admin: Address = e.storage().instance().get(&DataKey::Admin).unwrap();
```

**TTL Management**:
```rust
e.storage().instance().extend_ttl(100, 100);  // Extend by 100 ledgers
```

### 2. Persistent Storage (Cross-Contract Data)

Used for long-term data that needs to persist:
- Attestation records
- Historical data
- Registry mappings

**Pattern**:
```rust
e.storage().persistent().set(&DataKey::Attestation(id), &attestation);
```

### 3. Temporary Storage (Ephemeral Data)

Used for short-lived data:
- Nonces
- Temporary locks
- Session data

**Pattern**:
```rust
e.storage().temporary().set(&key, &value);
```

### 4. Mapping Patterns

**One-to-One Mapping**:
```rust
// Identity → Bond Contract
DataKey::IdentityToBond(Address) → RegistryEntry
```

**One-to-Many Mapping**:
```rust
// Subject → List of Attestation IDs
DataKey::SubjectAttestations(Address) → Vec<u64>
```

**Bidirectional Mapping**:
```rust
// Identity ↔ Bond Contract
DataKey::IdentityToBond(Address) → RegistryEntry
DataKey::BondToIdentity(Address) → Address
```

### 5. Counter Pattern

Auto-incrementing IDs for records:
```rust
let counter: u64 = e.storage()
    .instance()
    .get(&DataKey::AttestationCounter)
    .unwrap_or(0);

let next_id = counter.checked_add(1).expect("counter overflow");
e.storage().instance().set(&DataKey::AttestationCounter, &next_id);
```

### 6. Existence Checking Pattern

Efficient boolean lookups:
```rust
// Instead of storing full object
DataKey::HashExists(String) → bool

// Check
if e.storage().instance().has(&DataKey::HashExists(&hash)) {
    panic!("hash already exists");
}
```

## Security Architecture

### 1. Access Control Hierarchy

```
┌────────────────────────────────────────┐
│          SuperAdmin                    │
│  • Full system control                 │
│  • Can manage all roles                │
│  • Emergency functions                 │
└──────────┬────────────────────────────┘
           │
┌──────────▼────────────────────────────┐
│            Admin                       │
│  • Protocol parameter changes          │
│  • Operator management                 │
│  • Normal admin functions              │
└──────────┬────────────────────────────┘
           │
┌──────────▼────────────────────────────┐
│          Operator                      │
│  • Limited operational tasks           │
│  • Read operations                     │
│  • No critical functions               │
└────────────────────────────────────────┘
```

### 2. Reentrancy Protection

**Lock Pattern**:
```rust
fn withdraw_bond_full(e: Env, identity: Address) -> i128 {
    // 1. Acquire lock
    Self::acquire_lock(&e);

    // 2. Checks
    let bond = get_bond(&e);
    validate_withdrawal(&bond);

    // 3. Effects
    update_bond_state(&e, &bond);

    // 4. Interactions
    let result = external_call(&e);

    // 5. Release lock
    Self::release_lock(&e);

    result
}
```

**Checks-Effects-Interactions (CEI) Pattern**:
```rust
// ✓ CORRECT
pub fn slash_bond(e: Env, amount: i128) {
    // 1. Checks
    let bond = get_bond(&e);
    require(bond.active);
    require(amount <= available_amount);

    // 2. Effects
    bond.slashed_amount += amount;
    save_bond(&e, &bond);

    // 3. Interactions
    emit_event(&e, "slashed", amount);
    call_external_contract(&e);
}
```

### 3. Input Validation

**Multi-Layer Validation**:
```rust
pub fn create_bond(e: Env, amount: i128, duration: u64) {
    // Layer 1: Type safety (Rust type system)
    // amount is i128, duration is u64

    // Layer 2: Business logic validation
    require(amount > 0, "amount must be positive");
    require(duration > 0, "duration must be positive");
    require(duration <= MAX_DURATION, "duration too long");

    // Layer 3: State validation
    require(!has_active_bond(&e), "bond already exists");

    // Layer 4: Overflow protection
    let total = amount.checked_add(fee).expect("overflow");
}
```

### 4. Authorization Patterns

**Signature-Based Auth**:
```rust
pub fn create_bond(e: Env, identity: Address, amount: i128) {
    identity.require_auth();  // Stellar signature verification
    // ... rest of function
}
```

**Role-Based Auth**:
```rust
pub fn slash_bond(e: Env, admin: Address, amount: i128) {
    require_admin(&e, &admin);  // Check admin role
    // ... rest of function
}
```

**Multi-Sig Auth** (Treasury):
```rust
pub fn execute_withdrawal(e: Env, proposal_id: u64) {
    let proposal = get_proposal(&e, proposal_id);
    let threshold = get_threshold(&e);
    require(proposal.approvals >= threshold);
    // ... execute
}
```

### 5. Replay Protection

**Nonce-Based**:
```rust
pub fn add_attestation(e: Env, subject: Address, nonce: u64) {
    let expected_nonce = get_nonce(&e, &subject);
    require(nonce == expected_nonce, "invalid nonce");

    // Increment nonce
    set_nonce(&e, &subject, expected_nonce + 1);
}
```

### 6. Overflow Protection

**Checked Arithmetic**:
```rust
// ✓ CORRECT
let new_amount = bond.bonded_amount
    .checked_add(top_up_amount)
    .expect("overflow");

// ✗ WRONG
let new_amount = bond.bonded_amount + top_up_amount;  // Can overflow
```

### 7. Event Emission (Audit Trail)

**Comprehensive Events**:
```rust
// All state changes emit events
e.events().publish(
    (Symbol::new(&e, "bond_created"),),
    (identity.clone(), amount, duration),
);

e.events().publish(
    (Symbol::new(&e, "slash_executed"),),
    (proposal_id, amount, executor.clone()),
);
```

## Integration Patterns

### 1. Cross-Contract Calls

**Registry Lookup Pattern**:
```rust
// In Bond Contract
let registry_addr = get_registry_address(&e);
let registry = RegistryClient::new(&e, &registry_addr);
registry.register_identity(&identity, &e.current_contract_address());
```

### 2. Token Integration

**ERC-20 Token Flow**:
```rust
// Setup
let token = TokenClient::new(&e, &token_address);

// Approve (user does this beforehand)
token.approve(&identity, &contract, &amount, &expiration);

// Transfer in bond creation
token.transfer_from(&contract, &identity, &contract, &amount);

// Transfer for withdrawal
token.transfer(&contract, &identity, &amount);
```

### 3. Event-Driven Architecture

**Event Emission**:
```rust
// Contract emits
e.events().publish(
    (Symbol::new(&e, "event_name"), param1),
    data,
);
```

**Off-Chain Indexer**:
```
1. Indexer subscribes to contract events
2. Events are emitted on state changes
3. Indexer processes and stores in database
4. DApp queries indexer for historical data
```

### 4. Upgrade Pattern

**Admin-Controlled Upgrade**:
```rust
pub fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
    let admin = get_admin(&e);
    admin.require_auth();

    e.deployer().update_current_contract_wasm(new_wasm_hash);

    e.events().publish(
        (Symbol::new(&e, "contract_upgraded"),),
        new_wasm_hash,
    );
}
```

### 5. Modular Design

**Module Structure** (Bond Contract):
```rust
// lib.rs - Main contract
mod batch;              // Batch operations
mod fees;               // Fee calculation
mod slashing;           // Slashing logic
mod governance_approval; // Governance voting
mod evidence;           // Evidence storage
mod access_control;     // Role management
mod tiered_bond;        // Tier system
mod rolling_bond;       // Rolling bonds

// Each module exports functions used by main contract
pub use batch::create_batch_bonds;
pub use fees::calculate_fee;
```

### 6. Gas Optimization Patterns

**Batch Operations**:
```rust
// Instead of N transactions:
for identity in identities {
    create_bond(identity, amount, duration);  // N gas costs
}

// Single batch transaction:
create_batch_bonds(params_list);  // ~1.5x gas cost for N bonds
```

**Storage Minimization**:
```rust
// ✓ CORRECT - store IDs, fetch details on demand
SubjectAttestations(Address) → Vec<u64>

// ✗ WRONG - store full objects
SubjectAttestations(Address) → Vec<Attestation>  // Expensive!
```

## Summary

The Credence protocol implements a robust, modular architecture with:

- **7 specialized contracts** working in concert
- **Clear separation of concerns** (bond mgmt, registry, treasury, delegation, arbitration, admin, errors)
- **Multiple data flow patterns** (creation, attestation, slashing, delegation)
- **Comprehensive security** (reentrancy guards, access control, overflow protection)
- **Efficient storage** (instance/persistent/temporary, optimized key structures)
- **Gas-optimized operations** (batch processing, minimal storage)
- **Auditability** (event emission on all state changes)
- **Upgradability** (admin-controlled with governance)

This architecture enables a trust and reputation system that is secure, efficient, and maintainable while supporting complex workflows like governance-approved slashing, weighted attestations, and multi-signature treasury management.
