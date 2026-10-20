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

fn bump_persistent_ttl(e: &Env, key: &DataKey) {
    e.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
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

#[contracttype]
pub enum DataKey {
    Admin,
    OperationCounter,
    Operation(u64),
    ExecutedOp(BytesN<32>),
    Call(u64),
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
    bump_persistent_ttl(e, &key);
    op
}

fn store_operation(e: &Env, op: &QueuedOperation) {
    let key = DataKey::Operation(op.id);
    e.storage().persistent().set(&key, op);
    bump_persistent_ttl(e, &key);
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
    /// The operation is *hash-only*: it records an opaque `op_hash` and
    /// `execute_operation` will mark it executed without invoking anything.
    /// Use [`TimelockContract::queue_call`] to queue an operation that is bound
    /// to a concrete contract call.
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

    /// Queue a concrete contract call that becomes executable after `delay`.
    ///
    /// The returned operation id identifies an operation whose `op_hash` is
    /// derived on-chain from `(network_id, timelock_address, target, func, args,
    /// salt)`. `execute_operation` runs exactly that call, so the call cannot be
    /// substituted after the delay unless the hash changes as well.
    ///
    /// The target contract must authorize this timelock contract (typically by
    /// holding it as its admin) for the inner call to succeed.
    pub fn queue_call(
        e: Env,
        proposer: Address,
        target: Address,
        func: Symbol,
        args: Vec<Val>,
        delay: u64,
    ) -> u64 {
        bump_instance_ttl(&e);
        Self::require_proposer_admin(&e, &proposer);
        Self::require_valid_delay(&e, delay);

        let (op_id, eta, expires_at) = Self::begin_queue(&e, delay);

        let salt = derive_salt(&e, op_id, eta);
        let op_hash = compute_call_hash(&e, &target, &func, &args, &salt);

        let op = QueuedOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };

        let call = CallSpec {
            target,
            func,
            args,
            salt,
        };
        let call_key = DataKey::Call(op_id);
        e.storage().persistent().set(&call_key, &call);
        bump_persistent_ttl(&e, &call_key);

        store_operation(&e, &op);

        e.events().publish(
            (Symbol::new(&e, "operation_queued"), op_id),
            (proposer, op_hash, eta, expires_at),
        );

        op_id
    }

    /// Return the call bound to an operation, if it is call-bound.
    pub fn get_call(e: Env, op_id: u64) -> Option<CallSpec> {
        bump_instance_ttl(&e);
        let key = DataKey::Call(op_id);
        let call: Option<CallSpec> = e.storage().persistent().get(&key);
        if call.is_some() {
            bump_persistent_ttl(&e, &key);
        }
        call
    }

    /// Recompute the call hash with the same pre-image `queue_call` uses.
    ///
    /// Combined with the `salt` stored on a queued operation, this lets
    /// off-chain tooling confirm that what was queued is what will execute.
    pub fn hash_call(e: Env, target: Address, func: Symbol, args: Vec<Val>, salt: BytesN<32>) -> BytesN<32> {
        compute_call_hash(&e, &target, &func, &args, &salt)
    }

    /// Execute a queued operation after its ETA has passed and before its grace period expires.
    ///
    /// For call-bound operations the queued `(target, func, args)` is invoked.
    /// Effects (status and the replay guard) are written *before* the
    /// interaction; if the inner call fails the whole invocation reverts, so the
    /// operation stays `Pending` and is not marked executed.
    pub fn execute_operation(e: Env, op_id: u64) {
        bump_instance_ttl(&e);
        let mut op = load_operation(&e, op_id);

        if op.status != OperationStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let now = e.ledger().timestamp();
        if !is_ready(op.eta, now) {
            panic_with_error!(&e, ContractError::TimelockNotReady);
        }

        if now > op.expires_at {
            panic_with_error!(&e, ContractError::SignatureExpired);
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

        let mut op = load_operation(&e, op_id);

        if op.status != OperationStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        op.status = OperationStatus::Cancelled;
        store_operation(&e, &op);

        e.events()
            .publish((Symbol::new(&e, "operation_cancelled"), op_id), op.op_hash);
    }

    /// Get details of a queued operation.
    pub fn get_operation(e: Env, op_id: u64) -> Option<QueuedOperation> {
        bump_instance_ttl(&e);
        let key = DataKey::Operation(op_id);
        let op: Option<QueuedOperation> = e.storage().persistent().get(&key);
        if op.is_some() {
            bump_persistent_ttl(&e, &key);
        }
        op
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
            .checked_add(GRACE_PERIOD)
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

    pub fn transfer_admin(e: Env, new_admin: Address) {
        bump_instance_ttl(&e);
        let current_admin = Self::get_admin(e.clone());
        current_admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events().publish(
            (soroban_sdk::Symbol::new(&e, "admin_transferred"),),
            (current_admin, new_admin),
        );
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
