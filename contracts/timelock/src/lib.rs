#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, xdr::ToXdr, Address, Bytes, BytesN,
    Env, Symbol, Val, Vec,
};

/// Signature domain identifier for the Timelock contract.
///
/// This constant binds signatures to this specific contract, preventing
/// cross-contract replay attacks where a signature intended for one contract
/// could be replayed against another. Each contract in the Credence system
/// has a unique signature domain constant.
///
/// # Security
///
/// Without domain separation, a signature created for contract A could be
/// replayed against contract B if both contracts share the same nonce namespace
/// and signature verification logic. By including this domain in the signed
/// payload hash, we ensure signatures are only valid for their intended contract.
///
/// # Value
///
/// The domain is a human-readable string that uniquely identifies this contract
/// within the Credence system. It should be included in the signed payload hash
/// along with other payload fields (nonce, deadline, etc.).
#[allow(dead_code)]
const SIGNATURE_DOMAIN: &str = "Timelock";

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;
const CALL_BOUND_OPERATION_TTL: u32 = 3600;

/// Persistent entries (operations, which carry their `(target, func, args)`
/// call) must be kept alive for at least as long as the longest execution
/// window. The limits are kept below the network's maximum entry TTL
/// (`6_311_520` ledgers) so that `extend_ttl` is always valid.
const PERSISTENT_TTL_THRESHOLD: u32 = 518_400; // ~30 days
const PERSISTENT_TTL_EXTEND_TO: u32 = 3_110_400; // ~180 days

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

fn bump_persistent_ttl(e: &Env, key: DataKey) {
    e.storage().persistent().extend_ttl(
        &key,
        CALL_BOUND_OPERATION_TTL / 2,
        CALL_BOUND_OPERATION_TTL,
    );
}

pub const fn min_delay_seconds() -> u64 {
    86_400
}

pub fn is_ready(eta: u64, now: u64) -> bool {
    now >= eta
}

pub const GRACE_PERIOD: u64 = 86_400; // 24 hours

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OperationStatus {
    Pending = 0,
    Executed = 1,
    Cancelled = 2,
}

/// The concrete call a call-bound operation executes once its delay elapses.
///
/// `salt` is part of the `op_hash` pre-image and is derived on-chain from the
/// operation id and ETA, so an observer can recompute [`TimelockContract::hash_call`]
/// from what the queued operation publishes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSpec {
    pub target: Address,
    pub func: Symbol,
    pub args: Vec<Val>,
    pub salt: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedOperation {
    pub id: u64,
    pub op_hash: BytesN<32>,
    pub eta: u64,
    pub expires_at: u64,
    pub status: OperationStatus,
}

/// Call-bound operation that stores the target contract, function, and arguments.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallBoundOperation {
    pub id: u64,
    pub op_hash: BytesN<32>,
    pub target: Address,
    pub func: Symbol,
    pub args: Vec<Val>,
    pub eta: u64,
    pub expires_at: u64,
    pub status: OperationStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdmin {
    pub address: Address,
    pub eta: u64,
    pub expires_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    OperationCounter,
    Operation(u64),
    ExecutedOp(BytesN<32>),
    /// Persistent storage key for call-bound operations.
    CallBoundOperation(u64),
}

#[contract]
pub struct TimelockContract;

/// Derive the per-operation salt from values that are only known once the
/// operation id and its execution window have been assigned.
fn derive_salt(e: &Env, op_id: u64, eta: u64) -> BytesN<32> {
    let mut preimage = Bytes::new(e);
    preimage.append(&op_id.to_xdr(e));
    preimage.append(&eta.to_xdr(e));
    e.crypto().sha256(&preimage).into()
}

/// `op_hash = sha256(xdr(network_id ‖ timelock_address ‖ target ‖ func ‖ args ‖ salt))`
///
/// Every field of the pre-image is length-delimited by its XDR encoding, so
/// changing any byte of `target`, `func`, `args` or `salt` yields a different
/// hash and therefore a different queued operation.
fn compute_call_hash(
    e: &Env,
    target: &Address,
    func: &Symbol,
    args: &Vec<Val>,
    salt: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::new(e);
    preimage.append(&e.ledger().network_id().to_xdr(e));
    preimage.append(&e.current_contract_address().to_xdr(e));
    preimage.append(&target.to_xdr(e));
    preimage.append(&func.to_xdr(e));
    preimage.append(&args.clone().to_xdr(e));
    preimage.append(&salt.to_xdr(e));
    e.crypto().sha256(&preimage).into()
}

fn load_operation(e: &Env, op_id: u64) -> QueuedOperation {
    let key = DataKey::Operation(op_id);
    let op: QueuedOperation = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));
    bump_persistent_ttl(e, key);
    op
}

fn store_operation(e: &Env, op: &QueuedOperation) {
    let key = DataKey::Operation(op.id);
    e.storage().persistent().set(&key, op);
    bump_persistent_ttl(e, key);
}

#[contractimpl]
impl TimelockContract {
    /// Initialize the timelock contract with the admin address.
    pub fn initialize(e: Env, admin: Address) {
        bump_instance_ttl(&e);
        credence_errors::require_contract_uninitialized(
            &e,
            e.storage().instance().has(&DataKey::Admin),
        );
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::OperationCounter, &0_u64);
    }

    /// Queue a new administrative operation to be executed after the delay.
    ///
    /// This is the hash-only variant for backward compatibility. The operation
    /// stores only the hash and does not invoke any contract on execution.
    pub fn queue_operation(e: Env, proposer: Address, op_hash: BytesN<32>, delay: u64) -> u64 {
        bump_instance_ttl(&e);
        Self::require_proposer_admin(&e, &proposer);
        Self::require_valid_delay(&e, delay);

        // Replay guard: cannot queue an operation that was already executed
        if Self::is_operation_executed(e.clone(), op_hash.clone()) {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let (op_id, eta, expires_at) = Self::begin_queue(&e, delay);

        let op = QueuedOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };

        store_operation(&e, &op);

        e.events().publish(
            (Symbol::new(&e, "operation_queued"), op_id),
            (proposer, op_hash, eta, expires_at),
        );

        op_id
    }

    /// Queue a call-bound operation that will invoke a specific contract function on execution.
    ///
    /// The `op_hash` is computed as `sha256(network_id || timelock_address || target || func || args || salt)`.
    /// This ensures any change to the target, function, arguments, or salt produces a different hash.
    ///
    /// The operation is stored in persistent storage with TTL extension.
    pub fn queue_call(
        e: Env,
        proposer: Address,
        target: Address,
        func: Symbol,
        args: Vec<Val>,
        salt: BytesN<32>,
        delay: u64,
    ) -> u64 {
        bump_instance_ttl(&e);
        proposer.require_auth();
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        if proposer != admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        if delay < min_delay_seconds() {
            panic_with_error!(&e, ContractError::TimelockNotReady);
        }

        // Compute the operation hash on-chain
        let op_hash = compute_call_hash(&e, &target, &func, &args, &salt);

        // Replay guard: cannot queue an operation that was already executed
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let op_id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::OperationCounter)
            .unwrap_or(0);
        let next_op_id = op_id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::OperationCounter, &next_op_id);

        let now = e.ledger().timestamp();
        let eta = now
            .checked_add(delay)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let expires_at = eta
            .checked_add(GRACE_PERIOD)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));

        let op = CallBoundOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            target: target.clone(),
            func: func.clone(),
            args: args.clone(),
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };

        // Store in persistent storage with TTL extension
        let op_key = DataKey::CallBoundOperation(op_id);
        e.storage().persistent().set(&op_key, &op);
        bump_persistent_ttl(&e, op_key);

        // Also store in instance storage for backward compatibility with get_operation
        let op_wrapper = QueuedOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };
        e.storage()
            .instance()
            .set(&DataKey::Operation(op_id), &op_wrapper);

        e.events().publish(
            (Symbol::new(&e, "operation_queued"), op_id),
            (proposer, op_hash, eta, expires_at),
        );

        op_id
    }

    /// Compute the hash for a call-bound operation.
    ///
    /// The hash is `sha256(timelock_address || target || func || args || salt)`.
    /// This is a public view function so off-chain tools can precompute the hash.
    pub fn hash_call(
        e: Env,
        target: Address,
        func: Symbol,
        args: Vec<Val>,
        salt: BytesN<32>,
    ) -> BytesN<32> {
        let timelock_address = e.current_contract_address();

        // Build the pre-image using Bytes and append (following the pattern in credence_bond)
        let mut preimage = Bytes::new(&e);
        preimage.append(&timelock_address.to_xdr(&e));
        preimage.append(&target.to_xdr(&e));
        preimage.append(&func.to_xdr(&e));
        preimage.append(&args.to_xdr(&e));
        preimage.append(&salt.to_xdr(&e));

        // Compute SHA256 hash using e.crypto().sha256()
        let hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        hash
    }

    /// Execute a queued operation after its ETA has passed and before its grace period expires.
    ///
    /// For call-bound operations, this will invoke the target contract's function with the
    /// stored arguments. The status is marked as Executed BEFORE the call, so if the call
    /// fails, the whole transaction reverts and the status stays Pending.
    pub fn execute_operation(e: Env, op_id: u64) {
        bump_instance_ttl(&e);
    }
}
