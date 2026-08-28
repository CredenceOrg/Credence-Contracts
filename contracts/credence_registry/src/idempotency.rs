use soroban_sdk::{contracterror, contracttype, Address, Bytes, BytesN, Env};

/// Storage key namespace for idempotent transactions.
///
/// Currently unreferenced by any production entry point in `lib.rs`
/// (`Idempotency::handle` is exercised only by this module's own tests).
/// Its instance-storage variant name doesn't overlap with `storage::DataKey`,
/// so wiring it in later introduces no collision — see
/// `docs/STORAGE_KEY_LAYOUT.md` for the contract's full key catalog.
#[contracttype]
pub enum StorageKey {
    Idempotent(BytesN<32>),
}

/// Storage transaction result
#[contracttype]
#[derive(Clone)]
pub struct StorageResult {
    pub caller: Address,
    pub result: Bytes,
    pub timestamp: u64,
}

/// Idempotency errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
        if let Some(existing) = env.storage().instance().get::<_, StorageResult>(&key) {
            if existing.caller != caller {
                return Err(IdempotencyError::DuplicateDifferentCaller);
            }
            return Ok(existing.result);
        }

        // Execute transaction logic
        let result = execute();

        let record = StorageResult {
            caller: caller.clone(),
            result: result.clone(),
            timestamp: env.ledger().timestamp(),
        };

        // Store result to prevent re-execution
        env.storage().instance().set(&key, &record);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    /// Build a fixed 32-byte tx_id from a single seed byte.
    fn tx_id(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    /// Build a short Bytes payload from a slice.
    fn payload(env: &Env, data: &[u8]) -> Bytes {
        Bytes::from_slice(env, data)
    }

    // ── unset ─────────────────────────────────────────────────────────────────

    /// First call with a never-seen tx_id must execute the closure and return
    /// its result.
    #[test]
    fn unset_executes_closure_and_returns_result() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let id = tx_id(&env, 0x01);
        let expected = payload(&env, b"hello");

        let result = Idempotency::handle(&env, id, caller, || payload(&env, b"hello"));

        assert_eq!(result, Ok(expected));
    }

    /// The closure must be called exactly once on the first invocation (no
    /// short-circuit despite unset state).
    #[test]
    fn unset_closure_is_called_exactly_once() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let id = tx_id(&env, 0x02);

        let call_count = std::cell::Cell::new(0u32);
        let _ = Idempotency::handle(&env, id, caller, || {
            call_count.set(call_count.get() + 1);
            payload(&env, b"once")
        });

        assert_eq!(call_count.get(), 1, "closure must be called exactly once");
    }

    // ── match ─────────────────────────────────────────────────────────────────

    /// Second call with the same tx_id and same caller must return the cached
    /// result without executing the closure again.
    #[test]
    fn match_same_caller_returns_cached_result_without_re_executing() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let id = tx_id(&env, 0x10);

        // First call — stores "original".
        let first = Idempotency::handle(&env, id.clone(), caller.clone(), || {
            payload(&env, b"original")
        });
        assert_eq!(first, Ok(payload(&env, b"original")));

        // Second call — must return "original" even though the closure would
        // produce "different".
        let second = Idempotency::handle(&env, id, caller, || payload(&env, b"different"));
        assert_eq!(
            second,
            Ok(payload(&env, b"original")),
            "cached result must be returned on match"
        );
    }

    /// The closure must NOT be invoked on a matching replay.
    #[test]
    fn match_closure_is_not_called_on_replay() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let id = tx_id(&env, 0x11);

        // Prime the cache.
        let _ = Idempotency::handle(&env, id.clone(), caller.clone(), || {
            payload(&env, b"primed")
        });

        let call_count = std::cell::Cell::new(0u32);
        let _ = Idempotency::handle(&env, id, caller, || {
            call_count.set(call_count.get() + 1);
            payload(&env, b"should_not_run")
        });

        assert_eq!(call_count.get(), 0, "closure must NOT be called on a match");
    }

    // ── mismatch ──────────────────────────────────────────────────────────────

    /// Second call with the same tx_id but a *different* caller must return
    /// `Err(IdempotencyError::DuplicateDifferentCaller)`.
    #[test]
    fn mismatch_different_caller_returns_error() {
        let env = Env::default();
        let original_caller = Address::generate(&env);
        let intruder = Address::generate(&env);
        let id = tx_id(&env, 0x20);

        // First call succeeds.
        let _ = Idempotency::handle(&env, id.clone(), original_caller, || payload(&env, b"data"));

        // Second call with a different caller must be rejected.
        let result =
            Idempotency::handle(&env, id, intruder, || payload(&env, b"should_not_matter"));

        assert_eq!(
            result,
            Err(IdempotencyError::DuplicateDifferentCaller),
            "different caller must receive DuplicateDifferentCaller"
        );
    }

    /// On a mismatch the closure must NOT be invoked.
    #[test]
    fn mismatch_closure_is_not_called() {
        let env = Env::default();
        let original_caller = Address::generate(&env);
        let intruder = Address::generate(&env);
        let id = tx_id(&env, 0x21);

        let _ = Idempotency::handle(&env, id.clone(), original_caller, || payload(&env, b"ok"));

        let call_count = std::cell::Cell::new(0u32);
        let _ = Idempotency::handle(&env, id, intruder, || {
            call_count.set(call_count.get() + 1);
            payload(&env, b"nope")
        });

        assert_eq!(
            call_count.get(),
            0,
            "closure must NOT be called on a mismatch"
        );
    }

    // ── isolation ─────────────────────────────────────────────────────────────

    /// Distinct tx_ids are independent: each gets its own storage slot.
    #[test]
    fn distinct_tx_ids_are_independent() {
        let env = Env::default();
        let caller = Address::generate(&env);

        let id_a = tx_id(&env, 0xAA);
        let id_b = tx_id(&env, 0xBB);

        let a = Idempotency::handle(&env, id_a, caller.clone(), || payload(&env, b"alpha"));
        let b = Idempotency::handle(&env, id_b, caller, || payload(&env, b"beta"));

        assert_eq!(a, Ok(payload(&env, b"alpha")));
        assert_eq!(b, Ok(payload(&env, b"beta")));
    }
}
