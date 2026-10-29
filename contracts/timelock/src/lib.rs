#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Bytes, BytesN, Env, Symbol,
    Val, Vec,
};
use soroban_sdk::xdr::ToXdr;

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
        PERSISTENT_TTL_THRESHOLD,
        PERSISTENT_TTL_EXTEND_TO,
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
    pub args: Vec<soroban_sdk::Val>,
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
    e.crypto().sha256(&preimage).to_bytes()
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
    e.crypto().sha256(&preimage).to_bytes()
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
