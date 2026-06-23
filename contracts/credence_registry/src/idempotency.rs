use soroban_sdk::{contracterror, contracttype, Address, Bytes, BytesN, Env};

/// Storage key namespace for idempotent transactions
#[contracttype]
pub enum StorageKey {
    Idempotent(BytesN<32>),
}

/// Stored transaction result
#[contracttype]
#[derive(Clone)]
pub struct StoredResult {
    pub caller: Address,
    pub result: Bytes,
    pub timestamp: u64,
}

/// Idempotency errors
#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum IdempotencyError {
    /// Transaction ID already used by a different caller
    DuplicateDifferentCaller = 1,
}

/// Idempotent transaction handler
pub struct Idempotency;

impl Idempotency {
    /// Executes a transaction in an idempotent manner.
    ///
    /// If the `tx_id` already exists:
    /// - Returns the stored result if the caller matches.
    /// - Returns an error if the caller differs.
    ///
    /// Otherwise:
    /// - Executes the provided logic.
    /// - Stores the result.
    /// - Returns the result.
    pub fn handle<F>(
        env: &Env,
        tx_id: BytesN<32>,
        caller: Address,
        execute: F,
    ) -> Result<Bytes, IdempotencyError>
    where
        F: FnOnce() -> Bytes,
    {
        let key = StorageKey::Idempotent(tx_id.clone());

        // Check if transaction already exists
        if let Some(existing) = env.storage().instance().get::<_, StoredResult>(&key) {
            if existing.caller != caller {
                return Err(IdempotencyError::DuplicateDifferentCaller);
            }
            return Ok(existing.result);
        }

        // Execute transaction logic
        let result = execute();

        let record = StoredResult {
            caller: caller.clone(),
            result: result.clone(),
            timestamp: env.ledger().timestamp(),
        };

        // Store result to prevent re-execution
        env.storage().instance().set(&key, &record);

        Ok(result)
    }
}
