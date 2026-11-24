#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Bytes, BytesN, Env, Symbol,
    Val, Vec,
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
#[allow(dead_code, reason = "reserved for future TTL-extension wiring")]
const PERSISTENT_TTL_THRESHOLD: u32 = 518_400; // ~30 days
#[allow(dead_code, reason = "reserved for future TTL-extension wiring")]
const PERSISTENT_TTL_EXTEND_TO: u32 = 3_110_400; // ~180 days

/// Hard cap on the page size accepted by
/// [`TimelockContract::get_operations_page`].
///
/// A caller requesting a larger `limit` is silently clamped to this value so a
/// single read can never exceed the gas/read budget regardless of the argument.
/// `limit == 0` selects this maximum, matching the admin contract's pagination.
const MAX_PAGE_LIMIT: u32 = 200;
pub const MAX_BATCH_SIZE: u32 = 20;

fn effective_page_limit(limit: u32) -> u32 {
    if limit == 0 {
        MAX_PAGE_LIMIT
    } else {
        limit.min(MAX_PAGE_LIMIT)
    }
}

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
pub struct CallRequest {
    pub target: Address,
    pub func: Symbol,
    pub args: Vec<Val>,
    pub salt: BytesN<32>,
    pub delay: u64,
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

/// Call-bound operation that stores its target, function, arguments, and hash salt.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallBoundOperation {
    pub id: u64,
    pub op_hash: BytesN<32>,
    pub target: Address,
    pub func: Symbol,
    pub args: Vec<soroban_sdk::Val>,
    pub salt: BytesN<32>,
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
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PendingAdmin,
    OperationCounter,
    Operation(u64),
    ExecutedOp(BytesN<32>),
    /// Persistent storage key for call-bound operations.
    CallBoundOperation(u64),
    /// Optional configured grace period; absent entries use `GRACE_PERIOD`.
    GracePeriod,
}

#[contract]
pub struct TimelockContract;

/// Derive the per-operation salt from values that are only known once the
/// operation id and its execution window have been assigned.
#[allow(dead_code, reason = "prepared for call-bound operation hashing; not yet wired to an entrypoint")]
fn derive_salt(e: &Env, op_id: u64, eta: u64) -> BytesN<32> {
    let mut preimage = Bytes::new(e);
    preimage.append(&op_id.to_xdr(e));
    preimage.append(&eta.to_xdr(e));
    e.crypto().sha256(&preimage).to_bytes()
}

/// `op_hash = sha256(xdr(network_id ‖ timelock_address ‖ target ‖ func ‖ args ‖ salt))`
///
/// Every field of the pre-image is length-delimited by its XDR encoding, so
/// changing any byte of `target`, `func`, `args` or `salt` yields a different
/// hash and therefore a different queued operation.
#[allow(dead_code, reason = "prepared for call-bound operation hashing; not yet wired to an entrypoint")]
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
    e.crypto().sha256(&preimage).to_bytes()
}

/// Load a queued operation from persistent storage, bumping its TTL.
#[allow(dead_code, reason = "helper used by test helpers and future entrypoints")]
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

    /// Set the grace period applied to newly queued operations.
    pub fn set_grace_period(e: Env, admin: Address, grace_period: u64) {
        bump_instance_ttl(&e);
        Self::require_proposer_admin(&e, &admin);
        let previous = Self::get_grace_period(e.clone());
        e.storage()
            .instance()
            .set(&DataKey::GracePeriod, &grace_period);
        e.events().publish(
            (Symbol::new(&e, "grace_period_updated"),),
            (previous, grace_period),
        );
    }

    /// Get the grace period used for newly queued operations.
    pub fn get_grace_period(e: Env) -> u64 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::GracePeriod)
            .unwrap_or(GRACE_PERIOD)
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
        args: Vec<soroban_sdk::Val>,
        salt: BytesN<32>,
        delay: u64,
    ) -> u64 {
        bump_instance_ttl(&e);
        Self::require_proposer_admin(&e, &proposer);
        Self::queue_call_internal(
            &e,
            &proposer,
            CallRequest {
                target,
                func,
                args,
                salt,
                delay,
            },
        )
    }

    /// Atomically queue a bounded batch of call-bound operations.
    ///
    /// Each request supplies its own target, payload, salt, and delay. Any
    /// invalid request aborts the transaction, including all earlier requests.
    pub fn queue_calls(e: Env, proposer: Address, calls: Vec<CallRequest>) -> Vec<u64> {
        bump_instance_ttl(&e);
        Self::require_proposer_admin(&e, &proposer);
        if calls.is_empty() {
            panic_with_error!(&e, ContractError::EmptyBatch);
        }
        if calls.len() > MAX_BATCH_SIZE {
            panic_with_error!(&e, ContractError::BatchTooLarge);
        }

        let mut op_ids = Vec::new(&e);
        for index in 0..calls.len() {
            let call = calls.get(index).unwrap();
            op_ids.push_back(Self::queue_call_internal(&e, &proposer, call));
        }
        op_ids
    }

    /// Compute the hash for a call-bound operation.
    ///
    /// The hash is `sha256(timelock_address || target || func || args || salt)`.
    /// This is a public view function so off-chain tools can precompute the hash.
    pub fn hash_call(
        e: Env,
        target: Address,
        func: Symbol,
        args: Vec<soroban_sdk::Val>,
        salt: BytesN<32>,
    ) -> BytesN<32> {
        let timelock_address = e.current_contract_address();

        // Build the pre-image using Bytes and append (following the pattern in credence_bond)
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&timelock_address.to_xdr(&e));
        preimage.append(&target.to_xdr(&e));
        preimage.append(&func.to_xdr(&e));
        preimage.append(&args.to_xdr(&e));
        preimage.append(&salt.to_xdr(&e));

        // Compute SHA256 hash using e.crypto().sha256()
        let hash: soroban_sdk::BytesN<32> = e.crypto().sha256(&preimage).into();
        hash
    }

    /// Execute a queued operation after its ETA has passed and before its grace period expires.
    ///
    /// For call-bound operations, this will invoke the target contract's function with the
    /// stored arguments. The status is marked as Executed BEFORE the call, so if the call
    /// fails, the whole transaction reverts and the status stays Pending.
    pub fn execute_operation(e: Env, op_id: u64) {
        bump_instance_ttl(&e);

        // First check if this is a call-bound operation in persistent storage
        let call_bound_key = DataKey::CallBoundOperation(op_id);
        if e.storage().persistent().has(&call_bound_key) {
            Self::execute_call_bound(&e, op_id);
            return;
        }

        // Otherwise, execute as hash-only operation (backward compatibility)
        let mut op: QueuedOperation = e
            .storage()
            .instance()
            .get(&DataKey::Operation(op_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

        if op.status != OperationStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let now = e.ledger().timestamp();
        if !is_ready(op.eta, now) {
            panic_with_error!(&e, ContractError::TimelockNotReady);
        }

        if now > op.expires_at {
            panic_with_error!(&e, ContractError::OperationExpired);
        }

        // Replay guard check
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op.op_hash.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        // Effects before the interaction: mark executed, then invoke.
        e.storage()
            .instance()
            .set(&DataKey::ExecutedOp(op.op_hash.clone()), &true);

        op.status = OperationStatus::Executed;
        store_operation(&e, &op);

        match Self::get_call(e.clone(), op_id) {
            Some(call) => {
                e.invoke_contract::<Val>(&call.target, &call.func, call.args);
                e.events().publish(
                    (Symbol::new(&e, "operation_executed"), op_id),
                    (op.op_hash, call.target, call.func),
                );
            }
            None => {
                e.events()
                    .publish((Symbol::new(&e, "operation_executed"), op_id), op.op_hash);
            }
        }
    }

    /// Internal: execute a call-bound operation.
    ///
    /// Effects are applied before the interaction (marking as Executed), so if the
    /// inner call fails, the whole invocation reverts and the status stays Pending.
    fn execute_call_bound(e: &Env, op_id: u64) {
        let key = DataKey::CallBoundOperation(op_id);
        let mut op: CallBoundOperation = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

        if op.status != OperationStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let now = e.ledger().timestamp();
        if !is_ready(op.eta, now) {
            panic_with_error!(&e, ContractError::TimelockNotReady);
        }

        if now > op.expires_at {
            panic_with_error!(&e, ContractError::OperationExpired);
        }

        // Replay guard check
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op.op_hash.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        // Mark executed BEFORE the call (effects before interaction)
        e.storage()
            .instance()
            .set(&DataKey::ExecutedOp(op.op_hash.clone()), &true);

        op.status = OperationStatus::Executed;
        e.storage().persistent().set(&key, &op);
        bump_persistent_ttl(e, key);

        // Also update the instance storage wrapper
        let op_wrapper = QueuedOperation {
            id: op.id,
            op_hash: op.op_hash.clone(),
            eta: op.eta,
            expires_at: op.expires_at,
            status: op.status,
        };
        e.storage()
            .instance()
            .set(&DataKey::Operation(op_id), &op_wrapper);

        // Emit event BEFORE the call
        e.events()
            .publish(("operation_executed", op_id), op.op_hash.clone());

        // Invoke the target contract
        e.invoke_contract::<soroban_sdk::Val>(&op.target, &op.func, op.args.clone());

        // Emit post-execution event
        e.events()
            .publish(("call_executed", op_id), (op.target, op.func));
    }

    /// Cancel a pending operation in the queue. Only callable by admin.
    pub fn cancel_operation(e: Env, admin: Address, op_id: u64) {
        bump_instance_ttl(&e);
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Check if this is a call-bound operation
        let call_bound_key = DataKey::CallBoundOperation(op_id);
        let is_call_bound = e.storage().persistent().has(&call_bound_key);

        if is_call_bound {
            let mut op: CallBoundOperation = e
                .storage()
                .persistent()
                .get(&call_bound_key)
                .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

            if op.status != OperationStatus::Pending {
                panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
            }

            op.status = OperationStatus::Cancelled;
            e.storage().persistent().set(&call_bound_key, &op);
            bump_persistent_ttl(&e, call_bound_key);

            // Update instance storage wrapper
            let op_wrapper = QueuedOperation {
                id: op.id,
                op_hash: op.op_hash.clone(),
                eta: op.eta,
                expires_at: op.expires_at,
                status: op.status,
            };
            e.storage()
                .instance()
                .set(&DataKey::Operation(op_id), &op_wrapper);

            e.events()
                .publish(("operation_cancelled", op_id), op.op_hash);
        } else {
            // Hash-only operation
            let mut op: QueuedOperation = e
                .storage()
                .instance()
                .get(&DataKey::Operation(op_id))
                .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

            if op.status != OperationStatus::Pending {
                panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
            }

            op.status = OperationStatus::Cancelled;
            e.storage().instance().set(&DataKey::Operation(op_id), &op);

            e.events()
                .publish(("operation_cancelled", op_id), op.op_hash);
        }
    }

    /// Get details of a queued operation.
    pub fn get_operation(e: Env, op_id: u64) -> Option<QueuedOperation> {
        bump_instance_ttl(&e);
        let key = DataKey::Operation(op_id);
        let op: Option<QueuedOperation> = e.storage().persistent().get(&key);
        if op.is_some() {
            bump_persistent_ttl(&e, key);
            bump_persistent_ttl(&e, key.clone());
            return op;
        }
        e.storage().instance().get(&key)
    }

    /// Get the concrete call associated with a call-bound operation.
    pub fn get_call(e: Env, op_id: u64) -> Option<CallSpec> {
        let key = DataKey::CallBoundOperation(op_id);
        let op: Option<CallBoundOperation> = e.storage().persistent().get(&key);
        if let Some(op) = op {
            bump_persistent_ttl(&e, key);
            Some(CallSpec {
                target: op.target,
                func: op.func,
                args: op.args,
                salt: op.salt,
            })
        } else {
            None
        }
    }

    /// Number of operation ids allocated so far.
    ///
    /// Ids are assigned from `0` and increase by one per queued operation, so
    /// the valid id space is `0..get_operation_count()`.
    pub fn get_operation_count(e: Env) -> u64 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::OperationCounter)
            .unwrap_or(0)
    }

    /// Read a page of queued operations in ascending id order.
    ///
    /// `offset` is an id-space index, not an index into the returned vector:
    /// the examined slice is `offset..min(offset + effective_limit, count)`.
    /// Missing entries (never stored, or expired from persistent storage) are
    /// skipped, so a page may be shorter than `limit` even when later ids
    /// exist; nothing is fabricated in their place.
    ///
    /// `limit == 0` means [`MAX_PAGE_LIMIT`]; any larger `limit` is clamped to
    /// it. Each returned entry has its persistent TTL bumped, exactly like
    /// [`Self::get_operation`].
    pub fn get_operations_page(e: Env, offset: u64, limit: u32) -> Vec<QueuedOperation> {
        bump_instance_ttl(&e);
        let count: u64 = e
            .storage()
            .instance()
            .get(&DataKey::OperationCounter)
            .unwrap_or(0);
        let mut page = Vec::new(&e);
        if offset >= count {
            return page;
        }
        let end = offset
            .saturating_add(u64::from(effective_page_limit(limit)))
            .min(count);
        for op_id in offset..end {
            let key = DataKey::Operation(op_id);
            if let Some(op) = e
                .storage()
                .persistent()
                .get::<DataKey, QueuedOperation>(&key)
            {
                bump_persistent_ttl(&e, key);
                page.push_back(op);
            }
        }
        page
        }
    }

    fn require_proposer_admin(e: &Env, proposer: &Address) {
        proposer.require_auth();
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if proposer != &admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
    }

    fn require_valid_delay(e: &Env, delay: u64) {
        if delay < min_delay_seconds() {
            panic_with_error!(e, ContractError::TimelockNotReady);
        }
    }

    fn queue_call_internal(e: &Env, proposer: &Address, call: CallRequest) -> u64 {
        Self::require_valid_delay(e, call.delay);
        let op_hash = Self::hash_call(
            e.clone(),
            call.target.clone(),
            call.func.clone(),
            call.args.clone(),
            call.salt.clone(),
        );
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(e, ContractError::ProposalAlreadyExecuted);
        }

        let (op_id, eta, expires_at) = Self::begin_queue(e, call.delay);
        let op = CallBoundOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            target: call.target.clone(),
            func: call.func.clone(),
            args: call.args,
            salt: call.salt,
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };
        let op_key = DataKey::CallBoundOperation(op_id);
        e.storage().persistent().set(&op_key, &op);
        bump_persistent_ttl(e, op_key);

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
            (Symbol::new(e, "operation_queued"), op_id),
            (proposer.clone(), op_hash, eta, expires_at),
        );
        op_id
    }

    /// Allocate the next operation id and compute its execution window.
    fn begin_queue(e: &Env, delay: u64) -> (u64, u64, u64) {
        let op_id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::OperationCounter)
            .unwrap_or(0);
        let next_op_id = op_id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::OperationCounter, &next_op_id);

        let now = e.ledger().timestamp();
        let eta = now
            .checked_add(delay)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        let expires_at = eta
            .checked_add(Self::get_grace_period(e.clone()))
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        (op_id, eta, expires_at)
    }

    /// Check if an operation hash has already been executed.
    pub fn is_operation_executed(e: Env, op_hash: BytesN<32>) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash))
            .unwrap_or(false)
    }

    /// Get the current admin address.
    pub fn get_admin(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }

    /// Propose a new admin. The current admin remains in control until the
    /// proposed admin accepts after the minimum delay.
    pub fn propose_admin(e: Env, new_admin: Address) {
        bump_instance_ttl(&e);
        let current_admin = Self::get_admin(e.clone());
        current_admin.require_auth();

        if new_admin == current_admin {
            panic_with_error!(&e, ContractError::AdminUnchanged);
        }
        if new_admin == e.current_contract_address() {
            panic_with_error!(&e, ContractError::InvalidAdminAddress);
        }

        let now = e.ledger().timestamp();
        let eta = now
            .checked_add(min_delay_seconds())
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let expires_at = eta
            .checked_add(Self::get_grace_period(e.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let pending = PendingAdmin {
            address: new_admin.clone(),
            eta,
            expires_at,
        };
        e.storage().instance().set(&DataKey::PendingAdmin, &pending);

        e.events().publish(
            (Symbol::new(&e, "admin_proposed"),),
            (current_admin, new_admin, eta, expires_at),
        );
    }

    /// Accept a pending admin proposal once its delay has elapsed.
    pub fn accept_admin(e: Env) {
        bump_instance_ttl(&e);
        let pending: PendingAdmin = e
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NoPendingAdmin));
        pending.address.require_auth();

        let now = e.ledger().timestamp();
        if !is_ready(pending.eta, now) {
            panic_with_error!(&e, ContractError::TimelockNotReady);
        }
        if now > pending.expires_at {
            panic_with_error!(&e, ContractError::SignatureExpired);
        }

        let current_admin = Self::get_admin(e.clone());
        e.storage()
            .instance()
            .set(&DataKey::Admin, &pending.address);
        e.storage().instance().remove(&DataKey::PendingAdmin);
        e.events().publish(
            (Symbol::new(&e, "admin_transferred"),),
            (current_admin, pending.address),
        );
    }

    /// Cancel the pending admin proposal. Only the current admin may cancel.
    pub fn cancel_admin_proposal(e: Env) {
        bump_instance_ttl(&e);
        let current_admin = Self::get_admin(e.clone());
        current_admin.require_auth();
        let pending: PendingAdmin = e
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NoPendingAdmin));
        e.storage().instance().remove(&DataKey::PendingAdmin);
        e.events().publish(
            (Symbol::new(&e, "admin_proposal_cancelled"),),
            (current_admin, pending.address),
        );
    }

    /// Get the pending admin proposal, if one exists.
    pub fn get_pending_admin(e: Env) -> Option<PendingAdmin> {
        bump_instance_ttl(&e);
        e.storage().instance().get(&DataKey::PendingAdmin)
    }
}

#[cfg(test)]
mod test_timelock;

#[cfg(test)]
mod test_call_binding;

#[cfg(test)]
mod test_events_schema;

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env,
    };

    fn setup_env() -> (Env, TimelockContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(TimelockContract, ());
        let client = TimelockContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, client, admin)
    }

    #[test]
    fn min_delay_is_one_day() {
        assert_eq!(min_delay_seconds(), 86_400);
    }

    #[test]
    fn ready_when_now_meets_eta() {
        assert!(is_ready(100, 100));
        assert!(is_ready(100, 200));
    }

    #[test]
    fn not_ready_when_before_eta() {
        assert!(!is_ready(100, 50));
    }

    #[test]
    fn test_initialize() {
        let (_, client, admin) = setup_env();
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_queue_and_execute() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[0; 32]);
        let delay = 86_400; // minimum delay

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        assert_eq!(op_id, 0);

        let op = client.get_operation(&op_id).unwrap();
        assert_eq!(op.op_hash, op_hash);
        assert_eq!(op.status, OperationStatus::Pending);

        // Before ETA: must fail
        env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());

        // At ETA: must succeed
        env.ledger().with_mut(|li| li.timestamp = op.eta);
        client.execute_operation(&op_id);

        let op = client.get_operation(&op_id).unwrap();
        assert_eq!(op.status, OperationStatus::Executed);
        assert!(client.is_operation_executed(&op_hash));
    }

    #[test]
    fn execute_rejects_already_executed_operation_with_typed_error() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[7; 32]);
        let delay = min_delay_seconds();

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();
        env.ledger().with_mut(|li| li.timestamp = op.eta);
        client.execute_operation(&op_id);

        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn cancel_rejects_executed_operation_with_typed_error() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[8; 32]);
        let delay = min_delay_seconds();

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();
        env.ledger().with_mut(|li| li.timestamp = op.eta);
        client.execute_operation(&op_id);

        let err = client.try_cancel_operation(&admin, &op_id);
        assert!(err.is_err());
    }

    #[test]
    fn execute_rejects_cancelled_operation_with_typed_error() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[9; 32]);
        let delay = min_delay_seconds();

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        client.cancel_operation(&admin, &op_id);
        let op = client.get_operation(&op_id).unwrap();
        env.ledger().with_mut(|li| li.timestamp = op.eta);

        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn test_execute_succeeds_at_expiry_boundary() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[1; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        // At expires_at + 1: must fail because past the grace period.
        env.ledger().with_mut(|li| li.timestamp = op.expires_at + 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn test_execute_expired() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[2; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        // At expires_at + 1: must fail
        env.ledger().with_mut(|li| li.timestamp = op.expires_at + 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn test_execute_operation_times_out_after_grace_period() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[6; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        env.ledger().with_mut(|li| li.timestamp = op.expires_at + 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());

        let op = client.get_operation(&op_id).unwrap();
        assert_eq!(op.status, OperationStatus::Pending);
        assert!(!client.is_operation_executed(&op_hash));
    }

    #[test]
    fn test_replay_guard() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[3; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        env.ledger().with_mut(|li| li.timestamp = op.eta);
        client.execute_operation(&op_id);

        // Try to queue same hash again: must fail
        let res = client.try_queue_operation(&admin, &op_hash, &delay);
        assert!(res.is_err());
    }

    #[test]
    fn test_cancel() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[4; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        client.cancel_operation(&admin, &op_id);

        let op = client.get_operation(&op_id).unwrap();
        assert_eq!(op.status, OperationStatus::Cancelled);

        // Try to execute cancelled: must fail
        env.ledger().with_mut(|li| li.timestamp = op.eta);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn test_delay_too_short() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[5; 32]);
        let delay = 86_399; // 1 second below min

        let res = client.try_queue_operation(&admin, &op_hash, &delay);
        assert!(res.is_err());
    }

    // --- operation count and paginated view (#1754) ---

    fn queue_hash_only(
        env: &Env,
        client: &TimelockContractClient<'_>,
        admin: &Address,
        tag: u8,
    ) -> u64 {
        client.queue_operation(
            admin,
            &BytesN::from_array(env, &[tag; 32]),
            &min_delay_seconds(),
        )
    }

    #[test]
    fn operations_page_is_empty_before_any_queue() {
        let (_env, client, _admin) = setup_env();

        assert_eq!(client.get_operation_count(), 0);
        assert_eq!(client.get_operations_page(&0, &10).len(), 0);
        assert_eq!(client.get_operations_page(&0, &0).len(), 0);
        assert_eq!(client.get_operations_page(&99, &10).len(), 0);
    }

    #[test]
    fn operations_page_lists_ids_in_ascending_order() {
        let (env, client, admin) = setup_env();
        let first = queue_hash_only(&env, &client, &admin, 1);
        let second = queue_hash_only(&env, &client, &admin, 2);
        let third = queue_hash_only(&env, &client, &admin, 3);

        assert_eq!(client.get_operation_count(), 3);

        let page = client.get_operations_page(&0, &10);
        assert_eq!(page.len(), 3);
        assert_eq!(page.get(0).unwrap().id, first);
        assert_eq!(page.get(1).unwrap().id, second);
        assert_eq!(page.get(2).unwrap().id, third);
        for op in page.iter() {
            assert_eq!(op.status, OperationStatus::Pending);
        }
    }

    #[test]
    fn operations_page_supports_a_page_smaller_than_the_range() {
        let (env, client, admin) = setup_env();
        for tag in 1..=4u8 {
            queue_hash_only(&env, &client, &admin, tag);
        }

        let page = client.get_operations_page(&1, &2);
        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0).unwrap().id, 1);
        assert_eq!(page.get(1).unwrap().id, 2);
    }

    #[test]
    fn operations_page_offset_past_the_end_is_empty() {
        let (env, client, admin) = setup_env();
        queue_hash_only(&env, &client, &admin, 1);

        assert_eq!(client.get_operations_page(&1, &10).len(), 0);
        assert_eq!(client.get_operations_page(&u64::MAX, &10).len(), 0);
    }

    #[test]
    fn operations_page_clamps_limit_and_zero_means_max() {
        let (env, client, admin) = setup_env();
        for _ in 0..=MAX_PAGE_LIMIT {
            queue_hash_only(&env, &client, &admin, 0);
        }
        assert_eq!(client.get_operation_count(), u64::from(MAX_PAGE_LIMIT + 1));

        // limit 0 selects MAX_PAGE_LIMIT.
        assert_eq!(client.get_operations_page(&0, &0).len(), MAX_PAGE_LIMIT);
        // A larger limit is clamped, never honoured.
        assert_eq!(
            client.get_operations_page(&0, &u32::MAX).len(),
            MAX_PAGE_LIMIT
        );
        // The last page is bounded by the id space, not the limit.
        let last = client.get_operations_page(&u64::from(MAX_PAGE_LIMIT), &u32::MAX);
        assert_eq!(last.len(), 1);
        assert_eq!(last.get(0).unwrap().id, u64::from(MAX_PAGE_LIMIT));
    }

    #[test]
    fn operations_page_keeps_executed_and_cancelled_entries() {
        let (env, client, _admin) = setup_env();

        // Seed the persistent entries directly: this test covers the view, not
        // the queue/execute/cancel lifecycle.
        env.as_contract(&client.address, || {
            let entries = [
                (0u64, 31u8, OperationStatus::Executed),
                (1u64, 32u8, OperationStatus::Pending),
                (2u64, 33u8, OperationStatus::Cancelled),
            ];
            for (id, tag, status) in entries {
                store_operation(
                    &env,
                    &QueuedOperation {
                        id,
                        op_hash: BytesN::from_array(&env, &[tag; 32]),
                        eta: 1_000,
                        expires_at: 87_400,
                        status,
                    },
                );
            }
            env.storage()
                .instance()
                .set(&DataKey::OperationCounter, &3_u64);
        });

        assert_eq!(client.get_operation_count(), 3);

        // Ascending id order, status intact, entry contents not dropped.
        let page = client.get_operations_page(&0, &10);
        assert_eq!(page.len(), 3);
        assert_eq!(page.get(0).unwrap().id, 0);
        assert_eq!(
            page.get(0).unwrap().op_hash,
            BytesN::from_array(&env, &[31; 32])
        );
        assert_eq!(page.get(0).unwrap().status, OperationStatus::Executed);
        assert_eq!(page.get(1).unwrap().id, 1);
        assert_eq!(
            page.get(1).unwrap().op_hash,
            BytesN::from_array(&env, &[32; 32])
        );
        assert_eq!(page.get(1).unwrap().status, OperationStatus::Pending);
        assert_eq!(page.get(2).unwrap().id, 2);
        assert_eq!(
            page.get(2).unwrap().op_hash,
            BytesN::from_array(&env, &[33; 32])
        );
        assert_eq!(page.get(2).unwrap().status, OperationStatus::Cancelled);
    }

    #[test]
    fn operations_page_skips_missing_ids_without_fabricating() {
        let (env, client, admin) = setup_env();
        queue_hash_only(&env, &client, &admin, 21);
        queue_hash_only(&env, &client, &admin, 22);
        queue_hash_only(&env, &client, &admin, 23);

        // Drop a stored entry the way an expired persistent key would vanish.
        env.as_contract(&client.address, || {
            env.storage().persistent().remove(&DataKey::Operation(1));
        });

        // The count still reflects allocated ids; the gap is only skipped.
        assert_eq!(client.get_operation_count(), 3);

        let page = client.get_operations_page(&0, &10);
        assert_eq!(page.len(), 2);
        assert_eq!(page.get(0).unwrap().id, 0);
        assert_eq!(page.get(1).unwrap().id, 2);

        // `offset` indexes the id space, so a page starting inside a gap
        // simply starts after it.
        let after_gap = client.get_operations_page(&1, &10);
        assert_eq!(after_gap.len(), 1);
        assert_eq!(after_gap.get(0).unwrap().id, 2);
    }

    // --- is_ready boundary regression suite ---
    //
    // `is_ready(eta, now)` returns `true` iff `now >= eta`.
    // `min_delay_seconds()` returns 86_400, interpreted as 1 second per ledger
    // (i.e. the minimum queue delay is 86 400 ledgers / seconds = 24 hours).
    //
    // The table below covers every interesting u64 boundary:
    //   eta == 0, now == 0, eta == u64::MAX, now == u64::MAX,
    //   eta == now (tie-break), eta == now + 1 (one ledger early),
    //   now == eta + 1 (one ledger late).
    // No combination should panic; the comparison is total over u64.

    #[test]
    fn is_ready_boundary_eta_zero_now_zero() {
        // eta == 0, now == 0  →  0 >= 0  →  true
        assert!(is_ready(0, 0));
    }

    #[test]
    fn is_ready_boundary_eta_zero_now_nonzero() {
        // eta == 0, now > 0  →  always ready
        assert!(is_ready(0, 1));
        assert!(is_ready(0, u64::MAX));
    }

    #[test]
    fn is_ready_boundary_now_zero_eta_nonzero() {
        // now == 0, eta > 0  →  never ready
        assert!(!is_ready(1, 0));
        assert!(!is_ready(u64::MAX, 0));
    }

    #[test]
    fn is_ready_boundary_eta_equals_now_exact_tie() {
        // eta == now: the tie-break that decides whether an action executes
        // on the exact ledger it becomes valid.  Must return true.
        let t: u64 = 1_000_000;
        assert!(is_ready(t, t));
    }

    #[test]
    fn is_ready_boundary_now_one_before_eta() {
        // now == eta - 1: one ledger before the ETA — must be false.
        let eta: u64 = 500;
        assert!(!is_ready(eta, eta - 1));
    }

    #[test]
    fn is_ready_boundary_now_one_after_eta() {
        // now == eta + 1: one ledger after the ETA — must be true.
        let eta: u64 = 500;
        assert!(is_ready(eta, eta + 1));
    }

    #[test]
    fn is_ready_boundary_eta_max_now_max_minus_one() {
        // Saturating upper bound: eta == u64::MAX, now == u64::MAX - 1  →  false
        assert!(!is_ready(u64::MAX, u64::MAX - 1));
    }

    #[test]
    fn is_ready_boundary_eta_max_now_max() {
        // eta == u64::MAX, now == u64::MAX  →  true (tie-break at maximum value)
        assert!(is_ready(u64::MAX, u64::MAX));
    }

    #[test]
    fn is_ready_boundary_eta_max_minus_one_now_max() {
        // now overflows past eta: now == u64::MAX > eta == u64::MAX - 1  →  true
        assert!(is_ready(u64::MAX - 1, u64::MAX));
    }

    #[test]
    fn is_ready_boundary_no_panic_exhaustive_pairs() {
        // Spot-check a selection of extreme pairs to confirm no overflow/panic.
        let pairs: &[(u64, u64)] = &[
            (0, 0),
            (0, u64::MAX),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
            (u64::MAX - 1, u64::MAX),
            (u64::MAX, u64::MAX - 1),
            (1, 0),
            (0, 1),
        ];
        for &(eta, now) in pairs {
            let _ = is_ready(eta, now); // must not panic
        }
    }

    #[test]
    fn min_delay_seconds_is_86400_one_second_per_ledger() {
        // min_delay_seconds() must equal 86_400.
        // Under the assumed 1-second-per-ledger cadence this corresponds to
        // exactly 24 hours, the minimum window before a queued action may execute.
        assert_eq!(min_delay_seconds(), 86_400);
        // Verify the constant fits comfortably in u64 (no truncation).
        let delay: u64 = min_delay_seconds();
        assert_eq!(delay, 86_400_u64);
    }
}
