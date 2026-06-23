#![no_std]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, BytesN, Env, Symbol,
};

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
}

#[contract]
pub struct TimelockContract;

#[contractimpl]
impl TimelockContract {
    /// Initialize the timelock contract with the admin address.
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, ContractError::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::OperationCounter, &0_u64);
    }

    /// Queue a new administrative operation to be executed after the delay.
    pub fn queue_operation(e: Env, proposer: Address, op_hash: BytesN<32>, delay: u64) -> u64 {
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

        // Replay guard: cannot queue an operation that was already executed
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash.clone()))
            .unwrap_or(false)
        {
            panic!("operation already executed");
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

        let op = QueuedOperation {
            id: op_id,
            op_hash: op_hash.clone(),
            eta,
            expires_at,
            status: OperationStatus::Pending,
        };

        e.storage().instance().set(&DataKey::Operation(op_id), &op);

        e.events().publish(
            (Symbol::new(&e, "operation_queued"), op_id),
            (proposer, op_hash, eta, expires_at),
        );

        op_id
    }

    /// Execute a queued operation after its ETA has passed and before its grace period expires.
    pub fn execute_operation(e: Env, op_id: u64) {
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
            panic_with_error!(&e, ContractError::SignatureExpired);
        }

        // Replay guard check
        if e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op.op_hash.clone()))
            .unwrap_or(false)
        {
            panic!("operation already executed");
        }

        // Mark executed
        e.storage()
            .instance()
            .set(&DataKey::ExecutedOp(op.op_hash.clone()), &true);

        op.status = OperationStatus::Executed;
        e.storage().instance().set(&DataKey::Operation(op_id), &op);

        e.events()
            .publish((Symbol::new(&e, "operation_executed"), op_id), op.op_hash);
    }

    /// Cancel a pending operation in the queue. Only callable by admin.
    pub fn cancel_operation(e: Env, admin: Address, op_id: u64) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

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
            .publish((Symbol::new(&e, "operation_cancelled"), op_id), op.op_hash);
    }

    /// Get details of a queued operation.
    pub fn get_operation(e: Env, op_id: u64) -> Option<QueuedOperation> {
        e.storage().instance().get(&DataKey::Operation(op_id))
    }

    /// Check if an operation hash has already been executed.
    pub fn is_operation_executed(e: Env, op_hash: BytesN<32>) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash))
            .unwrap_or(false)
    }

    /// Get the current admin address.
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }
}

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
        env.ledger().set_timestamp(op.eta - 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());

        // At ETA: must succeed
        env.ledger().set_timestamp(op.eta);
        client.execute_operation(&op_id);

        let op = client.get_operation(&op_id).unwrap();
        assert_eq!(op.status, OperationStatus::Executed);
        assert!(client.is_operation_executed(&op_hash));
    }

    #[test]
    fn test_execute_boundaries() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[1; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        // At expires_at: must succeed
        env.ledger().set_timestamp(op.expires_at);
        client.execute_operation(&op_id);
    }

    #[test]
    fn test_execute_expired() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[2; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        // At expires_at + 1: must fail
        env.ledger().set_timestamp(op.expires_at + 1);
        let res = client.try_execute_operation(&op_id);
        assert!(res.is_err());
    }

    #[test]
    fn test_replay_guard() {
        let (env, client, admin) = setup_env();
        let op_hash = BytesN::from_array(&env, &[3; 32]);
        let delay = 86_400;

        let op_id = client.queue_operation(&admin, &op_hash, &delay);
        let op = client.get_operation(&op_id).unwrap();

        env.ledger().set_timestamp(op.eta);
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
        env.ledger().set_timestamp(op.eta);
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
}
