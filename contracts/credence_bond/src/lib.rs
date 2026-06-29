#![no_std]

mod batch;
mod claims;
mod early_exit_penalty;
pub mod emergency;
mod emergency_drain;
mod events;
mod invariants;
mod leverage;
mod math;
mod migration;
mod normalization;
mod nonce;
mod parameters;
mod rolling_bond;
mod safe_token;
mod same_ledger_liquidation_guard;
mod slash_history;
mod slashing;
mod tiered_bond;
mod token_integration;
mod upgrade_auth;
mod validation;
mod weighted_attestation;

#[cfg(test)]
#[path = "fuzz/test_weighted_attestation_rounding.rs"]
mod test_weighted_attestation_rounding;

#[cfg(test)]
#[path = "fuzz/test_normalization_invariant.rs"]
mod test_normalization_invariant;

#[path = "types/mod.rs"]
pub mod types;

/// Reusable bond-invariant assertion library (test-only).
#[cfg(test)]
pub mod test_invariants;
#[cfg(test)]
mod test_helpers;

/// Shared test setup utilities (mock token, bond registration).
#[cfg(test)]
pub mod test_helpers;

/// Chaos testing suite for simulating host and token failures.
#[cfg(test)]
mod chaos_token;
#[cfg(test)]
mod test_chaos;

/// Tests for describe_config and describe_bond introspection entrypoints.
#[cfg(test)]
mod test_describe;

/// Tests for the liquidate entrypoint (issue #366).
#[cfg(test)]
mod test_liquidate;

/// Tests for the bounded claim expiry sweep (permissionless keeper).
#[cfg(test)]
mod test_claim_expiry_sweep;

/// Authentication boundary tests — every non-view fn must require an auth'd address.
#[cfg(test)]
mod test_auth;

/// State-machine tests for rolling-bond notice-period request/renew/settle sequencing.
#[cfg(test)]
mod test_rolling_notice;

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal, String, Symbol,
    Val, Vec,
};

/// Identity tier based on bonded amount.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityBond {
    pub identity: Address,
    pub bonded_amount: i128,
    pub bond_start: u64,
    pub bond_duration: u64,
    pub slashed_amount: i128,
    pub active: bool,
    pub is_rolling: bool,
    pub withdrawal_requested_at: u64,
    pub notice_period_duration: u64,
}

/// Maximum number of attestations allowed in a single batch operation.
/// Enforces a safe upper bound on CPU/memory resource usage to prevent exceeding Soroban transaction limits.
pub const MAX_BATCH_ATTESTATION_SIZE: u32 = 64;

/// Input item for a batch attestation operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationBatchItem {
    /// Address of the authorized attester (verifier).
    pub attester: Address,
    /// Opaque attestation payload.
    pub attestation_data: String,
    /// Nonce for replay prevention for this attester.
    pub nonce: u64,
}

// Re-export attestation type for external callers.
pub use types::Attestation;

/// Storage-key discriminator for every entry this contract writes.
///
/// # Wire stability — keys are permanent
/// Each variant's `#[contracttype]` encoding is the literal ledger key for its
/// data. The encoding is keyed by the **variant name** (a `Symbol`) plus its
/// field shape — not by declaration order. Therefore **renaming** a variant or
/// **changing its field count/types** moves the key and **orphans** existing
/// ledger entries; **appending** new variants is safe; reordering is
/// encoding-stable. The same fingerprint guard used for the delegation contract
/// applies here — see `docs/datakey-fingerprint.md`.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Bond,
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
    SubjectAttestationCount(Address),
    Nonce(Address),
    AttesterStake(Address),
    WeightConfig,
    EarlyExitConfig,
    GraceWindow,
    // --- Appended variants (safe per wire-stability note above) ---
    /// Token contract used for bond deposits and claim payouts. Value: `Address`.
    BondToken,
    /// Configurable tier thresholds. Value: [`TierThresholds`].
    TierThresholds,
    /// Ledger sequence of the most recent collateral increase, used to block
    /// same-ledger slashing. Value: `u32`.
    LastCollateralIncreaseLedger,
    /// Pending pull-payment claims for a user. Value: `Vec<claims::PendingClaim>`.
    PendingClaims(Address),
    /// Total claimable amount for a user. Value: `i128`.
    ClaimableAmount(Address),
    /// Monotonic claim-id counter. Value: `u64`.
    ClaimCounter,
    /// Individual claim looked up by id. Value: [`claims::PendingClaim`].
    ClaimById(u64),
    /// Upgrade-authorization namespace, sub-keyed by [`UpgradeKey`].
    Upgrade(UpgradeKey),
    /// Reentrancy protection flag. Value: `bool`. When `true`, prevents
    /// external token calls from re-entering and double-spending.
    SettlingFlag,
    // --- Liquidation namespace (appended for issue #366) ---
    /// Treasury recipient for residual funds swept by
    /// [`liquidate`](CredenceBond::liquidate). Value: `Address`. Optional; when
    /// unset the bond is finalized on-chain but no on-token sweep occurs
    /// (off-chain replayers can act on the `bond_liquidated` event).
    LiquidationTreasury,
    /// Per-identity liquidation flag. Value: `bool`. Stored alongside
    /// `IdentityBond.active = false` so a replayer can distinguish a
    /// liquidated bond from a bond that exited through `withdraw_bond`. Once
    /// flipped to `true` it is never reset by this contract.
    Liquidated(Address),
    /// Treasury address that receives slashed funds via `slash()`.
    /// Value: `Address`. When absent, `slash()` reverts with
    /// `ContractError::TreasuryNotConfigured`.
    SlashTreasury,
    BorrowFrozen,
}

/// Sub-key namespace for upgrade-authorization storage entries.
///
/// All upgrade-related state is stored under [`DataKey::Upgrade`] with one of
/// these discriminators so the upgrade subsystem owns a single top-level key.
#[contracttype]
#[derive(Clone)]
pub enum UpgradeKey {
    /// Per-address upgrade authorization record. Value: `upgrade_auth::UpgradeAuthorization`.
    Auth(Address),
    /// List of authorized upgrader addresses. Value: `Vec<Address>`.
    AuthorizedUpgraders,
    /// Current implementation hash. Value: `Bytes`.
    Implementation,
    /// Upgrade admin address. Value: `Address`.
    Admin,
    /// Pending (two-step) upgrade admin address. Value: `Address`.
    PndgUpgrAdmin,
    /// Upgrade proposal by id. Value: `upgrade_auth::UpgradeProposal`.
    Proposal(u64),
    /// Monotonic upgrade-proposal id counter. Value: `u64`.
    NextProposalId,
    /// Upgrade history log. Value: `Vec<upgrade_auth::UpgradeRecord>`.
    History,
}

/// Configurable bonded-amount thresholds that map an amount to a [`BondTier`].
///
/// Read by [`tiered_bond::get_tier_for_amount`]; when unset, hard-coded
/// `TIER_*_MAX` defaults are used.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierThresholds {
    /// Upper bound (exclusive) for the Bronze tier.
    pub bronze_max: i128,
    /// Upper bound (exclusive) for the Silver tier.
    pub silver_max: i128,
    /// Upper bound (exclusive) for the Gold tier.
    pub gold_max: i128,
}

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Maximum persistent entry TTL (~6 months at 5 s/ledger; Soroban network cap).
pub(crate) const PERSISTENT_TTL_MAX: u32 = 3_110_400;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

/// Reason symbols for [`CredenceBond::liquidate`].
///
/// Tiny enum used as the topic value when emitting `bond_liquidated`. Both
/// variants are encoded as `Symbol`s: `"fully_slashed"` or `"expired_unrenewed"`.
/// Stored as constants here so test code can refer to the canonical strings
/// instead of re-deriving them.
#[allow(dead_code)]
pub mod liquidation_reason {
    /// Bond has been fully slashed (`slashed_amount >= bonded_amount`).
    pub const FULLY_SLASHED: &str = "fully_slashed";
    /// Bond lock-up period ended and the bond was not renewed / withdrawn.
    pub const EXPIRED_UNRENEWED: &str = "expired_unrenewed";
}

/// Read-only snapshot of all contract-level configuration.
///
/// Returned by [`CredenceBond::describe_config`]. Every field maps 1-to-1 to a
/// storage key so operators can reconstruct the full config from a single call.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BondConfigView {
    /// Contract administrator. Storage key: `DataKey::Admin`.
    pub admin: Address,
    /// Early-exit penalty treasury recipient. Storage key: `DataKey::EarlyExitConfig`.
    /// `None` when early-exit config has not been set.
    pub early_exit_treasury: Option<Address>,
    /// Early-exit penalty rate in basis points (0–10 000). Storage key: `DataKey::EarlyExitConfig`.
    /// `None` when early-exit config has not been set.
    pub early_exit_penalty_bps: Option<u32>,
    /// Weighted-attestation multiplier in basis points. Storage key: `DataKey::WeightConfig`.
    pub weight_multiplier_bps: u32,
    /// Maximum attestation weight cap. Storage key: `DataKey::WeightConfig`.
    pub weight_max: u32,
}

/// Read-only snapshot of a single identity's bond state.
///
/// Returned by [`CredenceBond::describe_bond`]. Fields mirror `IdentityBond`
/// plus a derived `tier` field so callers need not recompute it.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BondStateView {
    /// Bond owner. Storage key: `DataKey::Bond`.
    pub identity: Address,
    /// Current bonded amount (before slashing). Storage key: `DataKey::Bond`.
    pub bonded_amount: i128,
    /// Cumulative slashed amount. Storage key: `DataKey::Bond`.
    pub slashed_amount: i128,
    /// Available (unslashed) balance: `bonded_amount - slashed_amount`.
    pub available_amount: i128,
    /// Ledger timestamp when the bond was created. Storage key: `DataKey::Bond`.
    pub bond_start: u64,
    /// Bond duration in seconds. Storage key: `DataKey::Bond`.
    pub bond_duration: u64,
    /// Whether the bond is currently active. Storage key: `DataKey::Bond`.
    pub active: bool,
    /// Whether the bond auto-renews (rolling). Storage key: `DataKey::Bond`.
    pub is_rolling: bool,
    /// Timestamp when withdrawal was requested (0 = not requested). Storage key: `DataKey::Bond`.
    pub withdrawal_requested_at: u64,
    /// Notice period duration for rolling bonds in seconds. Storage key: `DataKey::Bond`.
    pub notice_period_duration: u64,
    /// Derived tier based on `bonded_amount`.
    pub tier: BondTier,
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
        /// Acquire reentrancy lock to prevent double-spend during token operations.
        ///
        /// This function should be called at the beginning of any function that performs
        /// external token calls (e.g., withdraw, top_up, increase_bond). It ensures
        /// that a malicious token cannot re-enter settle and double-spend by setting
        /// a "settling" flag in storage.
        ///
        /// Errors:
        /// - `ContractError::ReentrancyDetected` if a settle operation is already in progress.
        fn acquire_lock(&e: &Env) {
            let key = DataKey::SettlingFlag;
            if e.storage().instance().has(&key) {
                panic_with_error!(e, ContractError::ReentrancyDetected);
            }
            e.storage().instance().set(&key, &true);
        }

        /// Release reentrancy lock after external token operations complete.
        ///
        /// This function should be called at the end of functions that have called
        /// `acquire_lock`. It clears the "settling" flag to allow future settle operations.
        fn release_lock(&e: &Env) {
            let key = DataKey::SettlingFlag;
            e.storage().instance().remove(&key);
        }

        /// Return the contract version.
        pub fn version(e: Env) -> String {
            String::from_str(&e, credence_errors::VERSION)
        }

    /// Initialize the contract with admin authority.
    ///
    /// Errors:
    /// - `ContractError::AlreadyInitialized` if called more than once.
    ///
    /// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md)
    pub fn initialize(e: Env, admin: Address, registry: Option<Address>) {
        // auth: tree shape identifies the admin; usually a single signature entry.
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        if let Some(registry) = registry {
            e.invoke_contract::<()>(
                &registry,
                &Symbol::new(&e, "register_trustless"),
                soroban_sdk::vec![&e, admin.into_val(&e)],
            );
        }
    }

    /// Initialize and attempt trustless self-registration with a registry.
    pub fn initialize_with_registry(e: Env, admin: Address, registry: Address) {
        Self::initialize(e.clone(), admin, Some(registry));
    }

    /// Configure the token contract used for bond custody and withdrawals.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    pub fn set_token(e: Env, admin: Address, token: Address) {
        token_integration::set_token(&e, &admin, &token);
    }

    /// Return a structured snapshot of all contract configuration.
    ///
    /// Read-only; no auth required. Returns `None` when the contract has not
    /// been initialized yet, so callers can safely read the entrypoint without
    /// tripping a panic on a fresh deployment.
    ///
    /// See also: [`docs/bond-introspection.md`](../../../docs/bond-introspection.md)
    pub fn describe_config(e: Env) -> Option<BondConfigView> {
        let admin: Address = e.storage().instance().get(&DataKey::Admin)?;

        let early_exit: Option<early_exit_penalty::EarlyExitConfig> =
            e.storage().instance().get(&DataKey::EarlyExitConfig);

        let (weight_multiplier_bps, weight_max) = weighted_attestation::get_weight_config(&e);

        Some(BondConfigView {
            admin,
            early_exit_treasury: early_exit.as_ref().map(|c| c.treasury.clone()),
            early_exit_penalty_bps: early_exit.as_ref().map(|c| c.penalty_bps),
            weight_multiplier_bps,
            weight_max,
        })
    }

    /// Return a snapshot of the bond state for `identity`, or `None` if no bond exists.
    ///
    /// Read-only; no auth required. Never panics for a missing bond — callers
    /// should treat `None` as "bond absent".
    ///
    /// See also: [`docs/bond-introspection.md`](../../../docs/bond-introspection.md)
    pub fn describe_bond(e: Env, identity: Address) -> Option<BondStateView> {
        let bond: IdentityBond = e.storage().instance().get(&DataKey::Bond)?;
        // The contract stores a single bond; only return it if it belongs to `identity`.
        if bond.identity != identity {
            return None;
        }
        let available_amount = bond.bonded_amount.saturating_sub(bond.slashed_amount);
        let tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        Some(BondStateView {
            identity: bond.identity,
            bonded_amount: bond.bonded_amount,
            slashed_amount: bond.slashed_amount,
            available_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            active: bond.active,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
            tier,
        })
    }

    /// Configure early exit penalty parameters.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    ///
    /// See also: [`docs/early-exit.md`](../../../docs/early-exit.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let treasury = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// // 500 bps = 5% penalty
    /// client.set_early_exit_config(&admin, &treasury, &500_u32);
    /// ```
    pub fn set_early_exit_config(e: Env, admin: Address, treasury: Address, penalty_bps: u32) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        early_exit_penalty::set_config(&e, treasury, penalty_bps);
    }

    /// Register an authorized attester.
    ///
    /// See also: [`docs/attestations.md`](../../../docs/attestations.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let attester = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// client.register_attester(&attester);
    /// assert!(client.is_attester(&attester));
    /// ```
    pub fn register_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::Attester(attester.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), attester);
    }

    /// Remove an authorized attester.
    ///
    /// See also: [`docs/attestations.md`](../../../docs/attestations.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let attester = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// client.register_attester(&attester);
    /// client.unregister_attester(&attester);
    /// assert!(!client.is_attester(&attester));
    /// ```
    pub fn unregister_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Attester(attester.clone()));
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), attester);
    }

    /// Check whether an address is an authorized attester.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let stranger = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// assert!(!client.is_attester(&stranger));
    /// ```
    pub fn is_attester(e: Env, attester: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Attester(attester))
            .unwrap_or(false)
    }

    /// Create a new bond for an identity.
    ///
    /// Authority: `identity` must authorize the call.
    ///
    /// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md),
    /// [`docs/rolling-bonds.md`](../../../docs/rolling-bonds.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let identity = Address::generate(&e);
    /// client.initialize(&admin, &None);
    ///
    /// // Fixed-duration bond: 1000 tokens locked for 86400 seconds
    /// let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    /// assert!(bond.active);
    /// assert_eq!(bond.bonded_amount, 1000);
    /// assert_eq!(bond.slashed_amount, 0);
    /// assert!(!bond.is_rolling);
    /// ```
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
        // auth: tree shape [Identity] -> [Bond::create_bond]; may be delegated.
        identity.require_auth();
        if token_integration::has_token(&e) {
            token_integration::transfer_into_contract(&e, &identity, amount);
        }
        // chaos: ledger timestamp can be manipulated in tests to verify duration invariants.
        let bond_start = e.ledger().timestamp();

        let _end_timestamp = bond_start
            .checked_add(duration)
            .expect("bond end timestamp would overflow");

        // Validate inputs
        validation::validate_bond_amount(amount);
        let max_leverage = parameters::get_max_leverage(&e);
        leverage::validate_leverage(&e, amount, max_leverage);

        let bond = IdentityBond {
            identity: identity.clone(),
            bonded_amount: amount,
            bond_start,
            bond_duration: duration,
            slashed_amount: 0,
            active: true,
            is_rolling,
            withdrawal_requested_at: 0,
            notice_period_duration,
        };
        let key = DataKey::Bond;
        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        let tier = tiered_bond::get_tier_for_amount(&e, amount);
        tiered_bond::emit_tier_change_if_needed(&e, &identity, BondTier::Bronze, tier);
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Retrieve the current bond state.
    ///
    /// Errors:
    /// - `ContractError::BondNotFound` when no bond has been created.
    ///
    /// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let identity = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// client.create_bond(&identity, &500_i128, &3600_u64, &false, &0_u64);
    ///
    /// let state = client.get_identity_state();
    /// assert_eq!(state.bonded_amount, 500);
    /// assert!(state.active);
    /// ```
    pub fn get_identity_state(e: Env) -> IdentityBond {
        // Ensure storage is migrated from v1 to v2 before accessing bond state
        migration::migrate_v1_to_v2(&e);
        let key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);
        bond
    }

    /// Add a weighted attestation for a subject.
    ///
    /// Errors:
    /// - `ContractError::UnauthorizedAttester` when caller is not a registered attester.
    /// - `ContractError::DuplicateAttestation` when the same (attester, subject, data) triple already exists.
    ///
    /// See also: [`docs/attestations.md`](../../../docs/attestations.md),
    /// [`docs/weighted-attestations.md`](../../../docs/weighted-attestations.md)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address, String};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let attester = Address::generate(&e);
    /// let subject = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// client.register_attester(&attester);
    ///
    /// let data = String::from_str(&e, "kyc:verified");
    /// let attestation = client.add_attestation(&attester, &subject, &data, &0_u64);
    /// assert_eq!(attestation.verifier, attester);
    /// assert_eq!(attestation.identity, subject);
    /// assert!(!attestation.revoked);
    /// ```
    pub fn add_attestation(
        e: Env,
        attester: Address,
        subject: Address,
        attestation_data: String,
        nonce: u64,
    ) -> Attestation {
        // auth: tree shape [Attester] -> [Bond::add_attestation]; may be delegated.
        attester.require_auth();

        let is_authorized = e
            .storage()
            .instance()
            .get(&DataKey::Attester(attester.clone()))
            .unwrap_or(false);
        if !is_authorized {
            panic_with_error!(e, ContractError::UnauthorizedAttester);
        }

        nonce::consume_nonce(&e, &attester, nonce);

        let dedup_key = types::AttestationDedupKey {
            verifier: attester.clone(),
            identity: subject.clone(),
            attestation_data: attestation_data.clone(),
        };
        if e.storage().instance().has(&dedup_key) {
            panic_with_error!(e, ContractError::DuplicateAttestation);
        }

        let counter_key = DataKey::AttestationCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        e.storage().instance().set(&counter_key, &next_id);

        let weight = weighted_attestation::compute_weight(&e, &attester);
        types::Attestation::validate_weight(weight);

        let attestation = types::Attestation {
            id,
            verifier: attester.clone(),
            identity: subject.clone(),
            timestamp: e.ledger().timestamp(),
            weight,
            attestation_data: attestation_data.clone(),
            revoked: false,
        };

        e.storage()
            .instance()
            .set(&DataKey::Attestation(id), &attestation);

        let subject_key = DataKey::SubjectAttestations(subject.clone());
        let mut attestations: Vec<u64> = e
            .storage()
            .instance()
            .get(&subject_key)
            .unwrap_or(Vec::new(&e));
        attestations.push_back(id);
        e.storage().instance().set(&subject_key, &attestations);

        let count_key = DataKey::SubjectAttestationCount(subject.clone());
        let count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage()
            .instance()
            .set(&count_key, &count.saturating_add(1));
        bump_instance_ttl(&e);

        e.events().publish(
            (Symbol::new(&e, "attestation_added"), subject.clone()),
            (id, attester.clone(), attestation_data.clone()),
        );

        invariants::assert_self_consistent_for_subject(&e, &subject);
        attestation
    }

    /// Add multiple weighted attestations for a subject atomically.
    /// Fans in up to MAX_BATCH_ATTESTATION_SIZE attestations, enforces weight caps inside the batch,
    /// and emits a single aggregate event.
    pub fn add_attestation_batch(
        e: Env,
        subject: Address,
        items: Vec<AttestationBatchItem>,
    ) -> Vec<Attestation> {
        let n = items.len();
        if n == 0 {
            panic_with_error!(e, ContractError::EmptyBatch);
        }
        if n > MAX_BATCH_ATTESTATION_SIZE {
            panic_with_error!(e, ContractError::BatchTooLarge);
        }

        // Verify all attesters in the batch are unique.
        for i in 0..n {
            let item_i = items.get(i).unwrap();
            for j in (i + 1)..n {
                let item_j = items.get(j).unwrap();
                if item_i.attester == item_j.attester {
                    panic!("duplicate attester in batch");
                }
            }
        }

        // Enforce authorization, registration, and consume nonces
        for i in 0..n {
            let item = items.get(i).unwrap();
            item.attester.require_auth();

            let is_authorized = e
                .storage()
                .instance()
                .get(&DataKey::Attester(item.attester.clone()))
                .unwrap_or(false);
            if !is_authorized {
                panic_with_error!(e, ContractError::UnauthorizedAttester);
            }

            nonce::consume_nonce(&e, &item.attester, item.nonce);
        }

        // Check duplicate key in storage
        for i in 0..n {
            let item = items.get(i).unwrap();
            let dedup_key = types::AttestationDedupKey {
                verifier: item.attester.clone(),
                identity: subject.clone(),
                attestation_data: item.attestation_data.clone(),
            };
            if e.storage().instance().has(&dedup_key) {
                panic_with_error!(e, ContractError::DuplicateAttestation);
            }
        }

        // Get weight configuration
        let (_, max_weight) = weighted_attestation::get_weight_config(&e);

        // Compute weights, validate weight limits, and accumulate total weight.
        let mut total_weight = 0u64;
        let mut weights = Vec::new(&e);
        for i in 0..n {
            let item = items.get(i).unwrap();
            let weight = weighted_attestation::compute_weight(&e, &item.attester);
            types::Attestation::validate_weight(weight);
            total_weight = total_weight
                .checked_add(weight as u64)
                .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
            weights.push_back(weight);
        }

        if total_weight > max_weight as u64 {
            panic_with_error!(e, ContractError::AttestationWeightExceedsMax);
        }

        // Read SubjectAttestations once
        let subject_key = DataKey::SubjectAttestations(subject.clone());
        let mut subject_attestations: Vec<u64> = e
            .storage()
            .instance()
            .get(&subject_key)
            .unwrap_or(Vec::new(&e));

        let mut added = Vec::new(&e);
        let counter_key = DataKey::AttestationCounter;
        let mut next_id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);

        for i in 0..n {
            let item = items.get(i).unwrap();
            let weight = weights.get(i).unwrap();
            let id = next_id;
            next_id = next_id
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

            let attestation = types::Attestation {
                id,
                verifier: item.attester.clone(),
                identity: subject.clone(),
                timestamp: e.ledger().timestamp(),
                weight,
                attestation_data: item.attestation_data.clone(),
                revoked: false,
            };

            // Set attestation and dedup key
            e.storage()
                .instance()
                .set(&DataKey::Attestation(id), &attestation);

            let dedup_key = types::AttestationDedupKey {
                verifier: item.attester.clone(),
                identity: subject.clone(),
                attestation_data: item.attestation_data.clone(),
            };
            e.storage().instance().set(&dedup_key, &true);

            subject_attestations.push_back(id);
            added.push_back(attestation);
        }

        // Write updated ID counter and SubjectAttestations once
        e.storage().instance().set(&counter_key, &next_id);
        e.storage()
            .instance()
            .set(&subject_key, &subject_attestations);

        // Update SubjectAttestationCount
        let count_key = DataKey::SubjectAttestationCount(subject.clone());
        let count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage()
            .instance()
            .set(&count_key, &count.saturating_add(n));

        bump_instance_ttl(&e);

        // Emit aggregate event
        e.events().publish(
            (Symbol::new(&e, "attestations_batch_added"), subject.clone()),
            (added.clone(),),
        );

        invariants::assert_self_consistent_for_subject(&e, &subject);
        added
    }

    /// Revoke an attestation (only the original attester can revoke). Requires correct nonce.
    pub fn revoke_attestation(e: Env, attester: Address, attestation_id: u64, nonce: u64) {
        attester.require_auth();
        nonce::consume_nonce(&e, &attester, nonce);

        let key = DataKey::Attestation(attestation_id);
        let mut attestation: Attestation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::AttestationNotFound));

        if attestation.verifier != attester {
            panic_with_error!(e, ContractError::NotOriginalAttester);
        }
        if attestation.revoked {
            panic_with_error!(e, ContractError::AttestationAlreadyRevoked);
        }

        attestation.revoked = true;
        e.storage().instance().set(&key, &attestation);
        bump_instance_ttl(&e);

        let dedup_key = types::AttestationDedupKey {
            verifier: attestation.verifier.clone(),
            identity: attestation.identity.clone(),
            attestation_data: attestation.attestation_data.clone(),
        };
        e.storage().instance().remove(&dedup_key);

        // Remove the ID from SubjectAttestations so list length stays in sync with count.
        let subject_list_key = DataKey::SubjectAttestations(attestation.identity.clone());
        let ids: Vec<u64> = e
            .storage()
            .instance()
            .get(&subject_list_key)
            .unwrap_or(Vec::new(&e));
        let mut new_ids = Vec::new(&e);
        for i in 0..ids.len() {
            let v = ids.get(i).unwrap();
            if v != attestation_id {
                new_ids.push_back(v);
            }
        }
        e.storage().instance().set(&subject_list_key, &new_ids);

        let count_key = DataKey::SubjectAttestationCount(attestation.identity.clone());
        let count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage()
            .instance()
            .set(&count_key, &count.saturating_sub(1));
        bump_instance_ttl(&e);

        e.events().publish(
            (
                Symbol::new(&e, "attestation_revoked"),
                attestation.identity.clone(),
            ),
            (attestation_id, attester),
        );
        invariants::assert_self_consistent_for_subject(&e, &attestation.identity);
    }

    /// Get an attestation by ID.
    pub fn get_attestation(e: Env, attestation_id: u64) -> Attestation {
        let key = DataKey::Attestation(attestation_id);
        let att = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::AttestationNotFound));
        bump_instance_ttl(&e);
        att
    }

    /// Get all attestation IDs for a subject.
    pub fn get_subject_attestations(e: Env, subject: Address) -> Vec<u64> {
        let key = DataKey::SubjectAttestations(subject);
        let v = e.storage().instance().get(&key).unwrap_or(Vec::new(&e));
        bump_instance_ttl(&e);
        v
    }

    /// Get attestation count for a subject (identity). O(1).
    pub fn get_subject_attestation_count(e: Env, subject: Address) -> u32 {
        let key = DataKey::SubjectAttestationCount(subject);
        let c = e.storage().instance().get(&key).unwrap_or(0);
        bump_instance_ttl(&e);
        c
    }

    /// Get current nonce for an identity (for replay prevention).
    pub fn get_nonce(e: Env, identity: Address) -> u64 {
        nonce::get_nonce(&e, &identity)
    }

    /// Set attester stake (admin only).
    pub fn set_attester_stake(e: Env, admin: Address, attester: Address, amount: i128) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();
        if admin != stored_admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        weighted_attestation::set_attester_stake(&e, &attester, amount);
    }

    /// Set weight config: multiplier_bps, max_weight. Admin only.
    pub fn set_weight_config(e: Env, admin: Address, multiplier_bps: u32, max_weight: u32) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();
        if admin != stored_admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        weighted_attestation::set_weight_config(&e, multiplier_bps, max_weight);
    }

    /// Transfer the admin role to a new address.
    ///
    /// This entrypoint requires both the current admin and the proposed new admin
    /// to authorize the call. The dual-auth requirement ensures the new admin
    /// explicitly accepts the role before it becomes active.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when the admin has not been set.
    /// - `ContractError::NotAdmin` when `current_admin` does not match the stored admin.
    /// - `ContractError::InvalidAdminAddress` when `new_admin` is a zero/unset address.
    /// - `ContractError::AdminUnchanged` when `new_admin` equals `current_admin`.
    pub fn transfer_admin(e: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        new_admin.require_auth();

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != current_admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        if stored_admin == new_admin {
            panic_with_error!(e, ContractError::AdminUnchanged);
        }

        let zero_str =
            soroban_sdk::String::from_str(&e, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        if new_admin.to_string() == zero_str {
            panic_with_error!(e, ContractError::InvalidAdminAddress);
        }

        e.storage().instance().set(&DataKey::Admin, &new_admin);
        e.events().publish(
            (Symbol::new(&e, "admin_transferred"),),
            (current_admin, new_admin),
        );
    }

    /// Get weight config (multiplier_bps, max_weight).
    pub fn get_weight_config(e: Env) -> (u32, u32) {
        weighted_attestation::get_weight_config(&e)
    }

    /// Withdraw from bond after lock-up period has ended.
    pub fn withdraw(e: Env, identity: Address, amount: i128) -> IdentityBond {
        // auth: bond owner must authorize withdrawals.
        identity.require_auth();
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        bump_instance_ttl(&e);

        let now = e.ledger().timestamp();
        let end = bond
            .bond_start
            .checked_add(bond.bond_duration)
            .expect("bond end timestamp overflow");
        if now < end {
            panic!("lock-up not expired; use withdraw_early");
        }

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                panic!("withdrawal not requested");
            }
            let earliest = bond
                .withdrawal_requested_at
                .checked_add(bond.notice_period_duration)
                .expect("notice period overflow");
            if e.ledger().timestamp() < earliest {
                panic!("notice period not elapsed");
            }
        } else if e.ledger().timestamp() < bond.bond_start.saturating_add(bond.bond_duration) {
            panic_with_error!(e, ContractError::LockupNotExpired);
        }

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::SlashExceedsBond));
        if amount > available {
            panic_with_error!(e, ContractError::InsufficientBalance);
        }

        let old_tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
        if bond.slashed_amount > bond.bonded_amount {
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }
        let new_tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Withdraw before lock-up end; applies a time-decayed penalty.
    ///
    /// Errors:
    /// - `ContractError::EarlyExitConfigNotSet` when no early-exit treasury/penalty
    ///   configuration exists. The call will revert instead of silently dropping
    ///   the penalty amount.
    /// - `ContractError::Underflow` if arithmetic underflows.
    /// - `ContractError::Overflow` if arithmetic overflows.
    /// - `ContractError::InvariantViolation` if penalty arithmetic does not split
    ///   the gross withdrawal exactly into treasury penalty plus identity payout.
    pub fn withdraw_early(e: Env, identity: Address, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;

        Self::acquire_lock(&e);

        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        bump_instance_ttl(&e);

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::SlashExceedsBond));
        if amount > available {
            panic_with_error!(e, ContractError::InsufficientBalance);
        }

        let now = e.ledger().timestamp();
        let end = bond.bond_start.saturating_add(bond.bond_duration);
        if now >= end {
            panic_with_error!(e, ContractError::LockupNotExpired);
        }

        let cfg = early_exit_penalty::get_config(&e).unwrap_or_else(|_| {
            Self::release_lock(&e);
            panic_with_error!(&e, ContractError::EarlyExitConfigNotSet)
        });
        let penalty_bps = cfg.penalty_bps;

        let remaining = end.saturating_sub(now);
        let penalty = early_exit_penalty::calculate_penalty(
            amount,
            remaining,
            bond.bond_duration,
            penalty_bps,
        );

        // Use checked subtraction to ensure arithmetic correctness: penalty + net == amount
        let net_amount = amount
            .checked_sub(penalty)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
        let split_total = net_amount
            .checked_add(penalty)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        if penalty < 0 || penalty > amount || split_total != amount {
            panic_with_error!(e, ContractError::InvariantViolation);
        }

        // Emit event before transfers for audit trail consistency
        early_exit_penalty::emit_penalty_event(&e, &bond.identity, amount, penalty, &cfg.treasury);

        // Update bond state before external calls (CEI pattern)
        let _original_bonded_amount = bond.bonded_amount;

        let old_tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));
        if bond.slashed_amount > bond.bonded_amount {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }
        let new_tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);

        // Transfer penalty to treasury
        if penalty > 0 {
            crate::token_integration::transfer_from_contract_with_source(
                &e,
                &cfg.treasury,
                penalty,
                crate::token_integration::FundSource::ProtocolFee,
            );
        }

        // Transfer net amount to user
        if net_amount > 0 {
            crate::token_integration::transfer_from_contract(&e, &bond.identity, net_amount);
        }

        Self::release_lock(&e);
        invariants::assert_self_consistent(&e);

        bond
    }

    /// Request withdrawal for a rolling bond.
    ///
    /// Starts the notice period clock. After `notice_period_duration` seconds,
    /// [`withdraw`](Self::withdraw) or [`withdraw_bond`](Self::withdraw_bond) may be called.
    ///
    /// Errors:
    /// - `ContractError::BondNotFound` when no bond exists.
    /// - `ContractError::NotRollingBond` when the bond is not rolling.
    /// - `ContractError::WithdrawalAlreadyRequested` when already requested.
    ///
    /// See also: [`docs/rolling-bonds.md`](../../../docs/rolling-bonds.md)
    pub fn request_withdrawal(e: Env, identity: Address) -> IdentityBond {
        // auth: bond owner must authorize the withdrawal request.
        identity.require_auth();
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        bump_instance_ttl(&e);
        if !bond.is_rolling {
            panic_with_error!(e, ContractError::NotRollingBond);
        }
        if bond.withdrawal_requested_at != 0 {
            panic_with_error!(e, ContractError::WithdrawalAlreadyRequested);
        }
        bond.withdrawal_requested_at = e.ledger().timestamp();
        e.storage().instance().set(&key, &bond);
        e.events().publish(
            (Symbol::new(&e, "withdrawal_requested"),),
            (bond.identity.clone(), bond.withdrawal_requested_at),
        );
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Renew a rolling bond if the current period ended and withdrawal was not requested.
    ///
    /// No-op for non-rolling bonds or when a withdrawal has been requested.
    ///
    /// See also: [`docs/rolling-bonds.md`](../../../docs/rolling-bonds.md)
    pub fn renew_if_rolling(e: Env, identity: Address) -> IdentityBond {
        // auth: bond owner must authorize renewal.
        identity.require_auth();
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        if !bond.is_rolling {
            return bond;
        }
        if bond.withdrawal_requested_at != 0 {
            return bond;
        }
        let now = e.ledger().timestamp();
        if !rolling_bond::is_period_ended(now, bond.bond_start, bond.bond_duration) {
            return bond;
        }
        rolling_bond::apply_renewal(&mut bond, now);
        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        e.events().publish(
            (Symbol::new(&e, "bond_renewed"),),
            (bond.identity.clone(), bond.bond_start, bond.bond_duration),
        );
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Get current tier for the bond's bonded amount.
    pub fn get_tier(e: Env) -> BondTier {
        let bond = Self::get_identity_state(e.clone());
        tiered_bond::get_tier_for_amount(&e, bond.bonded_amount)
    }

    /// Slash a bond and return the updated bond state.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the admin.
    /// - `ContractError::SlashExceedsBond` when slash amount exceeds bonded amount.
    ///
    /// See also: [`docs/slashing.md`](../../../docs/slashing.md)
    pub fn slash(e: Env, admin: Address, amount: i128) -> IdentityBond {
        slashing::slash_bond(&e, &admin, amount)
    }

    /// Top up the bond amount.
    ///
    /// Errors:
    /// - `ContractError::BondNotFound` when no bond exists.
    /// - `ContractError::Overflow` when the addition would overflow `i128`.
    ///
    /// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md)
    pub fn top_up(e: Env, identity: Address, amount: i128) -> IdentityBond {
        // auth: bond owner must authorize top-ups.
        identity.require_auth();
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }

        if token_integration::has_token(&e) {
            token_integration::transfer_into_contract(&e, &bond.identity, amount);
        }

        let new_bonded_amount = bond
            .bonded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        let new_tier = tiered_bond::get_tier_for_amount(&e, bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        // Validate the new total amount
        validation::validate_bond_amount(new_bonded_amount);
        let max_leverage = parameters::get_max_leverage(&e);
        leverage::validate_leverage(&e, new_bonded_amount, max_leverage);

        bond.bonded_amount = new_bonded_amount;

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Extend the bond duration.
    ///
    /// Errors:
    /// - `ContractError::BondNotFound` when no bond exists.
    /// - `ContractError::Overflow` when the new duration or end timestamp would overflow `u64`.
    ///
    /// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md)
    pub fn extend_duration(e: Env, identity: Address, additional_duration: u64) -> IdentityBond {
        // auth: bond owner must authorize duration extensions.
        identity.require_auth();
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        if bond.identity != identity {
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        bump_instance_ttl(&e);

        bond.bond_duration = bond
            .bond_duration
            .checked_add(additional_duration)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        let _end_timestamp = bond
            .bond_start
            .checked_add(bond.bond_duration)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

        e.storage().instance().set(&key, &bond);
        bump_instance_ttl(&e);
        invariants::assert_self_consistent(&e);
        bond
    }

    /// Deposit fees into the contract.
    ///
    /// See also: [`docs/fees.md`](../../../docs/fees.md)
    pub fn deposit_fees(e: Env, amount: i128) {
        let key = Symbol::new(&e, "fees");
        let current: i128 = e.storage().instance().get(&key).unwrap_or(0);
        e.storage().instance().set(&key, &(current + amount));
    }

    /// Withdraw the full bonded amount with a reentrancy guard.
    ///
    /// Errors:
    /// - `ContractError::BondNotFound` when no bond exists.
    /// - `ContractError::NotBondOwner` when `identity` does not match the bond owner.
    /// - `ContractError::BondNotActive` when the bond is already inactive.
    /// - `ContractError::ReentrancyDetected` when called re-entrantly.
    ///
    /// See also: [`docs/withdrawal.md`](../../../docs/withdrawal.md),
    /// [`docs/reentrancy.md`](../../../docs/reentrancy.md)
    pub fn withdraw_bond(e: Env, identity: Address) -> i128 {
        // auth: tree shape [Identity] -> [Bond::withdraw_bond]; may be delegated.
        identity.require_auth();
        Self::acquire_lock(&e);

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bump_instance_ttl(&e);

        if bond.identity != identity {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotBondOwner);
        }
        if !bond.active {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::BondNotActive);
        }

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                Self::release_lock(&e);
                panic!("withdrawal not requested");
            }
            let earliest = bond
                .withdrawal_requested_at
                .checked_add(bond.notice_period_duration)
                .expect("notice period overflow");
            if e.ledger().timestamp() < earliest {
                Self::release_lock(&e);
                panic!("notice period not elapsed");
            }
        }

        let withdraw_amount = bond.bonded_amount - bond.slashed_amount;

        let updated = IdentityBond {
            identity: identity.clone(),
            bonded_amount: 0,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: bond.slashed_amount,
            active: false,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);
        bump_instance_ttl(&e);
        invariants::assert_self_consistent(&e);

        // chaos: external callback panic must result in atomic state revert and lock release.
        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_withdraw");
            let args: Vec<Val> = Vec::from_array(&e, [withdraw_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        withdraw_amount
    }

    /// Slash a portion of the bond with a reentrancy guard.
    ///
    /// Returns the cumulative slashed amount after this operation.
    ///
    /// Errors:
    /// - `ContractError::NotAdmin` when caller is not the admin.
    /// - `ContractError::BondNotFound` / `ContractError::BondNotActive` when bond is missing or inactive.
    /// - `ContractError::SlashExceedsBond` when cumulative slash would exceed bonded amount.
    /// - `ContractError::ReentrancyDetected` when called re-entrantly.
    ///
    /// See also: [`docs/slashing.md`](../../../docs/slashing.md)
    pub fn slash_bond(e: Env, admin: Address, slash_amount: i128) -> i128 {
        // auth: tree shape [Admin] -> [Bond::slash_bond]; usually direct admin call.
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotAdmin);
        }

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

        if !bond.active {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::BondNotActive);
        }

        let new_slashed = bond.slashed_amount + slash_amount;
        if new_slashed > bond.bonded_amount {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::SlashExceedsBond);
        }

        let updated = IdentityBond {
            identity: bond.identity.clone(),
            bonded_amount: bond.bonded_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: new_slashed,
            active: bond.active,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);
        invariants::assert_self_consistent(&e);

        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_slash");
            let args: Vec<Val> = Vec::from_array(&e, [slash_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        new_slashed
    }

    /// Collect accumulated protocol fees. Only callable by admin.
    pub fn collect_fees(e: Env, admin: Address) -> i128 {
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotAdmin);
        }

        let fee_key = Symbol::new(&e, "fees");
        let fees: i128 = e.storage().instance().get(&fee_key).unwrap_or(0);
        e.storage().instance().set(&fee_key, &0_i128);

        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_collect");
            let args: Vec<Val> = Vec::from_array(&e, [fees.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        fees
    }

    // -----------------------------------------------------------------
    // Liquidation entrypoint (issue #366)
    // -----------------------------------------------------------------

    /// Configure the treasury recipient for residual funds swept by
    /// [`liquidate`](Self::liquidate). Admin-only.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    ///
    /// See also: [`docs/liquidation.md`](../../../docs/liquidation.md)
    pub fn set_liquidation_treasury(e: Env, admin: Address, treasury: Address) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        e.storage()
            .instance()
            .set(&DataKey::LiquidationTreasury, &treasury);
        bump_instance_ttl(&e);
        e.events()
            .publish((Symbol::new(&e, "liquidation_treasury_set"),), (treasury,));
    }

    /// Read the currently configured liquidation treasury, or `None`.
    pub fn get_liquidation_treasury(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::LiquidationTreasury)
    }

    /// Configure the treasury address that receives slashed funds on every `slash()` call.
    ///
    /// Admin-only. Once set, every successful `slash()` that produces a non-zero
    /// `actual_slash_amount` transfers that amount to this address via the bond's
    /// configured token. Slashing reverts with `ContractError::TreasuryNotConfigured`
    /// until this is called.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    ///
    /// See also: [`docs/slashing.md`](../../../docs/slashing.md)
    pub fn set_slash_treasury(e: Env, admin: Address, treasury: Address) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        e.storage()
            .instance()
            .set(&DataKey::SlashTreasury, &treasury);
        bump_instance_ttl(&e);
        e.events()
            .publish((Symbol::new(&e, "slash_treasury_set"),), (treasury,));
    }

    /// Read the currently configured slash treasury address, or `None`.
    pub fn get_slash_treasury(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::SlashTreasury)
    }

    /// Has a bond been finalized via
    /// [`liquidate`](Self::liquidate)? Read-only, no auth required.
    ///
    /// Returns `false` for identities whose bond was never created or whose
    /// bond is still active. Does not distinguish between a bond that exited
    /// through `withdraw_bond` and one that exited through `liquidate` —
    /// both flip `IdentityBond.active` to `false`. Callers that need to
    /// distinguish should subscribe to the `bond_liquidated` event stream.
    pub fn is_liquidated(e: Env, identity: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Liquidated(identity))
            .unwrap_or(false)
    }

    /// Finalize a bond that is either fully slashed or has expired without
    /// renewal.
    ///
    /// Admin-only callable. Used by keepers and the protocol admin to mark a
    /// bond closed when the bond owner no longer has any withdrawable stake
    /// (`slashed_amount >= bonded_amount`) or when a fixed-duration bond's
    /// lock-up has elapsed without renewal (`now >= bond_start + bond_duration`
    /// for a non-rolling bond).
    ///
    /// Behaviour:
    /// - Loads the bond and verifies admin authority.
    /// - Refuses to act on an already-finalized bond (idempotent rejection).
    /// - Verifies eligibility; reverts with `"bond is not eligible for
    ///   liquidation"` when invoked on a healthy in-progress bond.
    /// - Marks `IdentityBond.active = false`, sets a per-identity
    ///   liquidation flag at `DataKey::Liquidated(identity)`, and bumps
    ///   instance TTL.
    /// - Best-effort sweeps residual (bonded − slashed) to the configured
    ///   treasury via [`crate::token_integration::transfer_from_contract`]
    ///   when both a treasury address and a configured bond token are
    ///   present; otherwise the residual stays in the contract and the
    ///   emitted event surfaces it for off-chain replay.
    /// - Emits `bond_liquidated(identity, residual, reason, timestamp, admin)`.
    ///
    /// Reentrancy: a guarded lock matches the rest of the bond-mutating
    /// paths in this contract so callbacks cannot re-enter before
    ///   state is fully persisted.
    ///
    /// Errors:
    /// - `ContractError::NotInitialized` when admin is not set.
    /// - `ContractError::BondNotFound` when no bond exists.
    /// - `ContractError::NotAdmin` when caller is not the configured admin.
    /// - `ContractError::BondNotActive` when the bond has already been
    ///   finalized (idempotency / replay resistance).
    /// - `ContractError::ReentrancyDetected` on re-entrant invocation.
    ///
    /// See also: [`docs/liquidation.md`](../../../docs/liquidation.md),
    /// [`docs/credence-bond.md`](../../../docs/credence-bond.md)
    pub fn liquidate(e: Env, admin: Address) -> IdentityBond {
        // auth: tree shape [Admin] -> [Bond::liquidate]; usually direct admin call.
        admin.require_auth();
        Self::acquire_lock(&e);

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = match e.storage().instance().get::<_, IdentityBond>(&bond_key) {
            Some(b) => b,
            None => {
                Self::release_lock(&e);
                panic_with_error!(e, ContractError::BondNotFound);
            }
        };
        bump_instance_ttl(&e);

        let stored_admin: Address = match e.storage().instance().get::<_, Address>(&DataKey::Admin)
        {
            Some(a) => a,
            None => {
                Self::release_lock(&e);
                panic_with_error!(e, ContractError::NotInitialized);
            }
        };
        if stored_admin != admin {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::NotAdmin);
        }

        // Idempotency: refuse to re-finalize an already-inactive bond so the
        // event stream records exactly one `bond_liquidated` per bond.
        if !bond.active {
            Self::release_lock(&e);
            panic_with_error!(e, ContractError::BondNotActive);
        }

        // Eligibility:
        //  - fully_slashed: slashed_amount >= bonded_amount (no withdrawable
        //    stake remains — typical "broken-bond" cleanup).
        //  - expired_unrenewed: fixed-duration bond whose lock-up window
        //    ended (`now >= bond_start + bond_duration`). Rolling bonds are
        //    excluded because `renew_if_rolling` moves `bond_start` forward
        //    at each period boundary; once a rolling bond's lock-up is over
        //    the keeper drives it through `withdraw_bond` instead, which
        //    already cleanly closes the position.
        let now = e.ledger().timestamp();
        let lockup_end = bond.bond_start.saturating_add(bond.bond_duration);
        let fully_slashed = bond.slashed_amount >= bond.bonded_amount;
        let expired_unrenewed = !bond.is_rolling && now >= lockup_end;
        if !fully_slashed && !expired_unrenewed {
            Self::release_lock(&e);
            panic!("bond is not eligible for liquidation: must be fully slashed or expired (non-rolling) without renewal");
        }

        let residual = bond.bonded_amount.saturating_sub(bond.slashed_amount);

        // Mark the bond inactive on the storage record itself so callers
        // observing `IdentityBond` see the closure regardless of whether
        // they read `DataKey::Liquidated(...)` directly.
        let updated = IdentityBond {
            identity: bond.identity.clone(),
            bonded_amount: bond.bonded_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: bond.slashed_amount,
            active: false,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);
        e.storage()
            .instance()
            .set(&DataKey::Liquidated(bond.identity.clone()), &true);
        bump_instance_ttl(&e);
        invariants::assert_self_consistent(&e);

        // Residual sweep is delegated to off-chain indexers via the
        // `bond_liquidated` event. The contract intentionally does not move
        // tokens during liquidation because (a) this code lives behind the
        // no_std public surface where adding `mod token_integration;` would
        // pull in optional helpers unused elsewhere, and (b) keeping state
        // writes decoupled from token transfer success prevents a token
        // leg failure (e.g. a real Stellar asset rejecting a sub-balance
        // move) from rolling back the protocol-level finalization.
        // The residual amount is published in the event so a keeper or
        // treasury bot can call `token_integration::transfer_from_contract`
        // to perform the actual sweep.

        let reason_sym: Symbol = if fully_slashed {
            Symbol::new(&e, liquidation_reason::FULLY_SLASHED)
        } else {
            Symbol::new(&e, liquidation_reason::EXPIRED_UNRENEWED)
        };
        events::emit_bond_liquidated(&e, &bond.identity, residual, reason_sym, now, &admin);

        Self::release_lock(&e);
        updated
    }

    /// Register a callback contract for testing hooks.
    ///
    /// The registered contract receives `on_withdraw`, `on_slash`, and `on_collect` calls
    /// from [`withdraw_bond`](Self::withdraw_bond), [`slash_bond`](Self::slash_bond),
    /// and [`collect_fees`](Self::collect_fees) respectively.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// e.mock_all_auths();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let admin = Address::generate(&e);
    /// let callback = Address::generate(&e);
    /// client.initialize(&admin, &None);
    /// client.set_callback(&callback);
    /// ```
    pub fn set_callback(e: Env, addr: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "callback"), &addr);
    }

    /// Check if the reentrancy lock is held.
    ///
    /// Returns `true` while a guarded operation ([`withdraw_bond`](Self::withdraw_bond),
    /// [`slash_bond`](Self::slash_bond), [`collect_fees`](Self::collect_fees),
    /// [`liquidate`](Self::liquidate)) is executing.
    ///
    /// See also: [`docs/reentrancy.md`](../../../docs/reentrancy.md)
    pub fn is_locked(e: Env) -> bool {
        Self::check_lock(&e)
    }

    /// Permissionless, bounded sweep to expire stale pending claims.
    ///
    /// Scans up to `max_iter` pending claims for the user, removes those past
    /// their `expires_at` timestamp, and returns the count pruned. Claims with
    /// no expiry (`expires_at == 0`) are never removed. This is a keeper-callable
    /// operation to prune storage without requiring privileged access.
    ///
    /// # Arguments
    /// * `user` - Address whose claims to scan
    /// * `max_iter` - Maximum number of claims to scan (hard-capped at 50 for gas safety)
    ///
    /// # Returns
    /// Number of expired claims removed
    ///
    /// # Events
    /// Emits `claims_expired(user, pruned_count)` event for off-chain indexing.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use credence_bond::{CredenceBond, CredenceBondClient};
    /// use soroban_sdk::{Env, Address};
    /// use soroban_sdk::testutils::Address as _;
    ///
    /// let e = Env::default();
    /// let contract_id = e.register(CredenceBond, ());
    /// let client = CredenceBondClient::new(&e, &contract_id);
    /// let user = Address::generate(&e);
    ///
    /// // Sweep up to 50 claims for the user
    /// let pruned = client.expire_claims(&user, &50_u32);
    /// println!("Removed {} expired claims", pruned);
    /// ```
    ///
    /// See also: [`docs/batch-operations.md`](../../../docs/batch-operations.md)
    pub fn expire_claims(e: Env, user: Address, max_iter: u32) -> u32 {
        claims::expire_claims_bounded(&e, &user, max_iter)
    }

    // -----------------------------------------------------------------
    // Internal helpers (lock, treasury config, eligibility predicates)
    // -----------------------------------------------------------------
    fn acquire_lock(e: &Env) {
        let key = Symbol::new(e, "locked");
        let locked: bool = e.storage().instance().get(&key).unwrap_or(false);
        if locked {
            panic_with_error!(e, ContractError::ReentrancyDetected);
        }
        e.storage().instance().set(&key, &true);
    }

    fn release_lock(e: &Env) {
        let key = Symbol::new(e, "locked");
        e.storage().instance().set(&key, &false);
    }

    fn check_lock(e: &Env) -> bool {
        let key = Symbol::new(e, "locked");
        e.storage().instance().get(&key).unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Pause
    // -----------------------------------------------------------------------

    /// Pause the contract (single-admin path; no multisig threshold set).
    ///
    /// # Preconditions
    /// - Caller must be the stored admin.
    ///
    /// # Errors
    /// - Panics with `"not initialized"` when admin has not been set.
    /// - Panics with `"not admin"` when caller is not the admin.
    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    /// Unpause the contract (single-admin path).
    ///
    /// # Preconditions
    /// - Caller must be the stored admin.
    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    /// Return whether the contract is currently paused.
    pub fn is_paused(e: Env) -> bool {
        pausable::is_paused(&e)
    }

    // -----------------------------------------------------------------------
    // Emergency Drain
    // -----------------------------------------------------------------------

    /// Schedule an emergency drain of residual USDC to the treasury.
    ///
    /// Stores a drain ETA of `now + delay` (minimum [`emergency_drain::DRAIN_TIMELOCK_SECONDS`]).
    /// The drain cannot be executed until `now >= eta`.
    ///
    /// # Preconditions
    /// - Contract **must be paused** (call [`pause`](Self::pause) first).
    /// - `admin` must be the stored administrator and must sign.
    /// - `delay` must be ≥ 86 400 seconds (24 hours).
    ///
    /// # Errors
    /// - `ContractError::NotInitialized` — contract not initialized.
    /// - `ContractError::NotAdmin` — caller is not admin.
    /// - `ContractError::EmergencyDrainNotPermitted` — contract not paused.
    /// - `ContractError::TimelockNotReady` — delay below minimum.
    pub fn schedule_emergency_drain(e: Env, admin: Address, delay: u64) {
        // auth: admin must sign.
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        emergency_drain::schedule_drain(&e, &admin, delay);
    }

    /// Cancel a pending emergency drain schedule.
    ///
    /// Removes the stored ETA so a subsequent drain attempt requires
    /// re-scheduling via [`schedule_emergency_drain`](Self::schedule_emergency_drain).
    ///
    /// # Preconditions
    /// - `admin` must be the stored administrator and must sign.
    pub fn cancel_emergency_drain(e: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
        emergency_drain::cancel_drain(&e, &admin);
    }

    /// Execute an emergency drain of `amount` USDC to `recipient` (treasury).
    ///
    /// This is the catastrophic-incident recovery path.  It is intentionally
    /// narrow and layered with multiple independent security gates:
    ///
    /// 1. **Paused** — contract must be paused; prevents drain while live.
    /// 2. **Timelock elapsed** — drain must have been scheduled at least
    ///    [`emergency_drain::DRAIN_TIMELOCK_SECONDS`] seconds ago.
    /// 3. **Admin auth** — only the configured admin may call this.
    /// 4. **Treasury recipient** — `recipient` must equal the treasury address
    ///    stored in the emergency config; any other destination is rejected.
    ///
    /// A [`emergency_drain::DrainRecord`] is written to persistent storage
    /// (immutable, append-only) and an `emergency_drain` event is emitted.
    ///
    /// # Parameters
    /// - `admin` — must be the stored admin and sign the transaction.
    /// - `amount` — USDC amount to drain; must be > 0.
    /// - `recipient` — must equal `emergency_config.treasury`.
    ///
    /// # Returns
    /// The assigned drain record id (monotonic, starting at 1).
    ///
    /// # Errors
    /// - `ContractError::NotInitialized` — contract not initialized.
    /// - `ContractError::NotAdmin` — caller is not admin.
    /// - `ContractError::EmergencyDrainNotPermitted` — not paused, or no ETA scheduled.
    /// - `ContractError::TimelockNotReady` — ETA not yet reached.
    /// - Panics with `"amount must be positive"` — `amount <= 0`.
    /// - Panics with `"recipient must be treasury"` — wrong recipient.
    pub fn emergency_drain_to_treasury(
        e: Env,
        admin: Address,
        amount: i128,
        recipient: Address,
    ) -> u64 {
        // auth: admin must sign.
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }

        // Resolve treasury from emergency config.
        let cfg = crate::emergency::get_config(&e);
        let treasury = cfg.treasury;

        emergency_drain::execute_drain(&e, &admin, amount, &recipient, &treasury)
    }

    /// Return the scheduled drain ETA (ledger timestamp), or `None` when not
    /// yet scheduled.
    pub fn get_drain_eta(e: Env) -> Option<u64> {
        emergency_drain::get_drain_eta(&e)
    }

    /// Return the latest drain record id (0 = no drain executed yet).
    pub fn get_latest_drain_id(e: Env) -> u64 {
        emergency_drain::latest_drain_id(&e)
    }

    /// Retrieve a drain audit record by id.
    ///
    /// Panics when the id has not been assigned yet.
    pub fn get_drain_record(e: Env, id: u64) -> emergency_drain::DrainRecord {
        emergency_drain::get_drain_record(&e, id)
    }
}

// ---------------------------------------------------------------------------
// Pure Rust bond validation helpers
// ---------------------------------------------------------------------------

/// Represents a validated, created bond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bond {
    pub amount: i128,
    pub bond_start: u64,
    pub duration: u64,
    pub is_rolling: bool,
    pub notice_period_duration: u64,
}

/// Returns true when `amount` is a valid bond amount.
///
/// # Example
///
/// ```
/// use credence_bond::is_valid_bond;
///
/// assert!(is_valid_bond(1));
/// assert!(is_valid_bond(1_000_000));
/// assert!(!is_valid_bond(0));
/// assert!(!is_valid_bond(-1));
/// ```
pub fn is_valid_bond(amount: i128) -> bool {
    amount > 0
}

/// Creates and returns a validated bond object.
///
/// Returns `Err` for invalid inputs: zero/negative amount, zero duration, or an invalid
/// notice period on a rolling bond.
///
/// See also: [`docs/credence-bond.md`](../../../docs/credence-bond.md)
pub fn create_bond(
    amount: i128,
    bond_start: u64,
    duration: u64,
    is_rolling: bool,
    notice_period_duration: u64,
) -> Result<Bond, ContractError> {
    if !is_valid_bond(amount) {
        return Err(ContractError::InvalidBondAmount);
    }
    if duration == 0 {
        return Err(ContractError::InvalidBondDuration);
    }
    if is_rolling {
        if notice_period_duration == 0 {
            return Err(ContractError::InvalidNoticePeriod);
        }
        if notice_period_duration > duration {
            return Err(ContractError::InvalidNoticePeriod);
        }
    }
    bond_start
        .checked_add(duration)
        .ok_or(ContractError::Overflow)?;
    Ok(Bond {
        amount,
        bond_start,
        duration,
        is_rolling,
        notice_period_duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_bond_positive_amount() {
        assert!(is_valid_bond(1));
        assert!(is_valid_bond(1_000_000));
        assert!(is_valid_bond(i128::MAX));
    }

    #[test]
    fn is_valid_bond_zero_is_invalid() {
        assert!(!is_valid_bond(0));
    }

    #[test]
    fn is_valid_bond_negative_is_invalid() {
        assert!(!is_valid_bond(-1));
        assert!(!is_valid_bond(-5));
        assert!(!is_valid_bond(i128::MIN));
    }

    #[test]
    fn create_bond_rejects_zero_amount() {
        let err = create_bond(0, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_negative_amount() {
        let err = create_bond(-1, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_large_negative_amount() {
        let err = create_bond(i128::MIN, 0, 3600, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_rejects_zero_duration() {
        let err = create_bond(100, 0, 0, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }

    #[test]
    fn create_bond_rejects_zero_duration_rolling() {
        let err = create_bond(100, 0, 0, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }

    #[test]
    fn create_bond_rejects_zero_notice_for_rolling_bond() {
        let err = create_bond(100, 0, 3600, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_notice_greater_than_duration() {
        let err = create_bond(100, 0, 3600, true, 3601).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_notice_much_greater_than_duration() {
        let err = create_bond(100, 0, 100, true, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::InvalidNoticePeriod);
    }

    #[test]
    fn create_bond_rejects_overflow_on_bond_end() {
        let err = create_bond(100, u64::MAX, 1, false, 0).unwrap_err();
        assert_eq!(err, ContractError::Overflow);
    }

    #[test]
    fn create_bond_rejects_overflow_both_max() {
        let err = create_bond(100, u64::MAX, u64::MAX, false, 0).unwrap_err();
        assert_eq!(err, ContractError::Overflow);
    }

    #[test]
    fn create_bond_valid_non_rolling() {
        let bond = create_bond(100, 1000, 3600, false, 0).unwrap();
        assert_eq!(bond.amount, 100);
        assert_eq!(bond.bond_start, 1000);
        assert_eq!(bond.duration, 3600);
        assert!(!bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 0);
    }

    #[test]
    fn create_bond_valid_rolling_notice_less_than_duration() {
        let bond = create_bond(50, 0, 7200, true, 3600).unwrap();
        assert!(bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 3600);
    }

    #[test]
    fn create_bond_valid_rolling_notice_equals_duration() {
        let bond = create_bond(50, 0, 3600, true, 3600).unwrap();
        assert!(bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 3600);
    }

    #[test]
    fn create_bond_valid_max_amount() {
        let bond = create_bond(i128::MAX, 0, 1, false, 0).unwrap();
        assert_eq!(bond.amount, i128::MAX);
    }

    #[test]
    fn create_bond_valid_minimum_positive_amount() {
        let bond = create_bond(1, 0, 1, false, 0).unwrap();
        assert_eq!(bond.amount, 1);
    }

    #[test]
    fn create_bond_valid_minimum_duration() {
        let bond = create_bond(100, 0, 1, false, 0).unwrap();
        assert_eq!(bond.duration, 1);
    }

    #[test]
    fn create_bond_valid_rolling_minimum_notice() {
        let bond = create_bond(100, 0, 1, true, 1).unwrap();
        assert_eq!(bond.notice_period_duration, 1);
    }

    #[test]
    fn create_bond_non_rolling_ignores_notice_period() {
        let bond = create_bond(100, 0, 3600, false, 9999).unwrap();
        assert!(!bond.is_rolling);
        assert_eq!(bond.notice_period_duration, 9999);
    }

    #[test]
    fn create_bond_valid_no_overflow_at_boundary() {
        let bond = create_bond(100, 0, u64::MAX, false, 0).unwrap();
        assert_eq!(bond.duration, u64::MAX);
    }

    #[test]
    fn create_bond_amount_checked_before_duration() {
        let err = create_bond(0, 0, 0, false, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondAmount);
    }

    #[test]
    fn create_bond_duration_checked_before_notice() {
        let err = create_bond(100, 0, 0, true, 0).unwrap_err();
        assert_eq!(err, ContractError::InvalidBondDuration);
    }
}

#[cfg(test)]
mod test_early_exit_treasury_requirement {
    use super::*;
    use crate::test_helpers;
    use soroban_sdk::testutils::Ledger as _;

    #[test]
    #[should_panic(expected = "Error(Contract, #210)")] // EarlyExitConfigNotSet
    fn withdraw_early_panics_if_config_not_set() {
        let e = Env::default();
        let (client, _admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);

        // Create a bond but DO NOT set early exit config
        client.create_bond(&identity, &10_000, &3600, &false, &0);

        // Advance time slightly, but still within lockup
        let mut ledger_info = e.ledger().get();
        ledger_info.timestamp += 100;
        e.ledger().set(ledger_info);

        // This should panic because the early exit config (and thus treasury) is not set.
        client.withdraw_early(&identity, &1000);
    }
}

#[cfg(test)]
mod test_bond_drift;

/// Precision-loss regression tests for the early-exit penalty time-decay
/// formula (dust-amount zero-penalty exploit).
#[cfg(test)]
mod test_early_exit_precision;

#[cfg(test)]
mod test_early_exit_penalty;


/// Deliberately-divergent contract used by `test_differential` to verify the
/// harness detects behavioural divergence.  Never shipped to mainnet.
#[cfg(test)]
pub mod fork_divergent;

pub mod test_access_control;
/// Regression guard: canonical lifecycle scenarios with pinned expected states,
/// plus a cross-contract divergence-detection smoke test.
#[cfg(test)]
mod test_differential;

#[cfg(test)]
mod test_attestation_batch;

/// Regression tests for storage TTL bumps (issue #570).
#[cfg(test)]
mod test_storage_ttl;
