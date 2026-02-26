//! # Credence Timelock Contract
//!
//! Enforces a mandatory delay before protocol parameter changes take effect.
//! Changes must be proposed by the admin, wait for a minimum delay period,
//! and can be cancelled by governance during the waiting period.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// A pending parameter change. Created when admin proposes a new value for a
/// protocol parameter. The change can only be executed after the ETA (estimated
/// time of arrival) has passed.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ParameterChange {
    /// Identifier for the parameter being changed.
    pub parameter_key: Symbol,
    /// The proposed new value.
    pub new_value: i128,
    /// Ledger timestamp when the change was proposed.
    pub proposed_at: u64,
    /// Earliest timestamp at which the change can be executed.
    pub eta: u64,
    /// True once the change has been executed.
    pub executed: bool,
    /// True if the change was cancelled by governance.
    pub cancelled: bool,
}

#[contracttype]
pub enum DataKey {
    /// Contract administrator who can propose and execute changes.
    Admin,
    /// Governance address that can cancel pending changes.
    GovernanceAddress,
    /// Minimum delay in seconds between proposal and execution.
    MinDelay,
    /// A pending or completed parameter change, indexed by ID.
    PendingChange(u64),
    /// Counter for generating unique change IDs.
    ChangeCounter,
}

#[contract]
pub struct Timelock;

#[contractimpl]
impl Timelock {
    /// Initialize the timelock contract.
    ///
    /// @param e          The contract environment
    /// @param admin      Address that can propose and execute parameter changes
    /// @param governance Address that can cancel pending changes
    /// @param min_delay  Minimum delay in seconds before a change can be executed
    pub fn initialize(e: Env, admin: Address, governance: Address, min_delay: u64) {
        admin.require_auth();
        if min_delay == 0 {
            panic!("min_delay must be greater than zero");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::GovernanceAddress, &governance);
        e.storage().instance().set(&DataKey::MinDelay, &min_delay);
        e.storage().instance().set(&DataKey::ChangeCounter, &0_u64);
        e.events().publish(
            (Symbol::new(&e, "timelock_initialized"),),
            (admin, governance, min_delay),
        );
    }

    /// Propose a parameter change. Only the admin can propose. The change is
    /// queued with an ETA of `now + min_delay`.
    ///
    /// @param e             The contract environment
    /// @param proposer      Must be the admin
    /// @param parameter_key Identifier for the parameter to change
    /// @param new_value     The proposed new value
    /// @return change_id    Unique ID for this pending change
    pub fn propose_change(
        e: Env,
        proposer: Address,
        parameter_key: Symbol,
        new_value: i128,
    ) -> u64 {
        proposer.require_auth();
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        if proposer != admin {
            panic!("only admin can propose changes");
        }

        let min_delay: u64 = e
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or_else(|| panic!("not initialized"));

        let now = e.ledger().timestamp();
        let eta = now.checked_add(min_delay).expect("eta overflow");

        let id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ChangeCounter)
            .unwrap_or(0);
        let next_id = id.checked_add(1).expect("change counter overflow");
        e.storage()
            .instance()
            .set(&DataKey::ChangeCounter, &next_id);

        let change = ParameterChange {
            parameter_key: parameter_key.clone(),
            new_value,
            proposed_at: now,
            eta,
            executed: false,
            cancelled: false,
        };

        e.storage()
            .instance()
            .set(&DataKey::PendingChange(id), &change);

        e.events().publish(
            (Symbol::new(&e, "change_proposed"), id),
            (parameter_key, new_value, eta),
        );
        id
    }

    /// Execute a pending parameter change. Only the admin can execute.
    /// The current ledger timestamp must be at or past the change's ETA.
    ///
    /// @param e         The contract environment
    /// @param change_id ID of the change to execute
    pub fn execute_change(e: Env, change_id: u64) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();

        let mut change: ParameterChange = e
            .storage()
            .instance()
            .get(&DataKey::PendingChange(change_id))
            .unwrap_or_else(|| panic!("change not found"));

        if change.cancelled {
            panic!("change has been cancelled");
        }
        if change.executed {
            panic!("change already executed");
        }

        let now = e.ledger().timestamp();
        if now < change.eta {
            panic!("timelock delay has not elapsed");
        }

        change.executed = true;
        e.storage()
            .instance()
            .set(&DataKey::PendingChange(change_id), &change);

        e.events().publish(
            (Symbol::new(&e, "change_executed"), change_id),
            (change.parameter_key.clone(), change.new_value),
        );
    }

    /// Cancel a pending parameter change. Only the governance address can cancel.
    ///
    /// @param e         The contract environment
    /// @param canceller Must be the governance address
    /// @param change_id ID of the change to cancel
    pub fn cancel_change(e: Env, canceller: Address, change_id: u64) {
        canceller.require_auth();
        let governance: Address = e
            .storage()
            .instance()
            .get(&DataKey::GovernanceAddress)
            .unwrap_or_else(|| panic!("not initialized"));
        if canceller != governance {
            panic!("only governance can cancel changes");
        }

        let mut change: ParameterChange = e
            .storage()
            .instance()
            .get(&DataKey::PendingChange(change_id))
            .unwrap_or_else(|| panic!("change not found"));

        if change.executed {
            panic!("change already executed");
        }
        if change.cancelled {
            panic!("change already cancelled");
        }

        change.cancelled = true;
        e.storage()
            .instance()
            .set(&DataKey::PendingChange(change_id), &change);

        e.events().publish(
            (Symbol::new(&e, "change_cancelled"), change_id),
            change.parameter_key.clone(),
        );
    }

    /// Update the minimum delay. Only the admin can call this.
    /// The new delay must be greater than zero.
    ///
    /// @param e         The contract environment
    /// @param new_delay New minimum delay in seconds
    pub fn update_min_delay(e: Env, new_delay: u64) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();

        if new_delay == 0 {
            panic!("min_delay must be greater than zero");
        }

        let old_delay: u64 = e.storage().instance().get(&DataKey::MinDelay).unwrap_or(0);

        e.storage().instance().set(&DataKey::MinDelay, &new_delay);

        e.events()
            .publish((Symbol::new(&e, "delay_updated"),), (old_delay, new_delay));
    }

    /// Get a parameter change by ID.
    pub fn get_change(e: Env, change_id: u64) -> ParameterChange {
        e.storage()
            .instance()
            .get(&DataKey::PendingChange(change_id))
            .unwrap_or_else(|| panic!("change not found"))
    }

    /// Get the current minimum delay.
    pub fn get_min_delay(e: Env) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    /// Get the admin address.
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    /// Get the governance address.
    pub fn get_governance(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::GovernanceAddress)
            .unwrap_or_else(|| panic!("not initialized"))
    }
}
