#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

mod early_exit_penalty;
mod rolling_bond;
mod tiered_bond;
pub mod access_control;
mod batch;
pub mod early_exit_penalty;
#[allow(dead_code)]
pub mod evidence;
mod events;
mod emergency;
mod fees;
pub mod governance_approval;
#[allow(dead_code)]
mod math;
mod nonce;
pub mod pausable;
pub mod rolling_bond;
mod parameters;
mod rolling_bond;
#[allow(dead_code)]
mod slash_history;
#[allow(dead_code)]
mod slashing;
mod tiered_bond;
mod validation;
pub mod tiered_bond;
mod token_integration;
mod validation;
pub mod verifier;
mod weighted_attestation;
pub mod types;

use crate::access_control::{
    add_verifier_role, is_verifier, remove_verifier_role, require_admin, require_verifier,
};
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

use soroban_sdk::token::TokenClient;

pub use evidence::{Evidence, EvidenceType};
pub use types::Attestation;

/// Identity tier based on bonded amount (Bronze < Silver < Gold < Platinum).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

pub mod cooldown;

#[contracttype]
#[derive(Clone, Debug)]
pub struct IdentityBond {
    pub identity: Address,
    pub bonded_amount: i128,
    pub bond_start: u64,
    pub bond_duration: u64,
    pub slashed_amount: i128,
    pub active: bool,
    pub is_rolling: bool,
    pub withdrawal_requested_at: u64,
    pub notice_period: u64,
}

// Re-export batch types
pub use batch::{BatchBondParams, BatchBondResult};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub id: u64,
    pub attester: Address,
    pub subject: Address,
    pub attestation_data: String,
    pub timestamp: u64,
    pub revoked: bool,
// Re-export batch types
pub use batch::{BatchBondParams, BatchBondResult};
/// A pending cooldown withdrawal request. Created when a bond holder signals
/// intent to withdraw; the withdrawal can only execute after the cooldown
/// period elapses.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CooldownRequest {
    pub requester: Address,
    pub amount: i128,
    pub requested_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Bond,
    Token,
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
    DuplicateCheck(Address, Address, String),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    /// Per-identity attestation count (updated on add/revoke).
    SubjectAttestationCount(Address),
    /// Per-identity nonce for replay prevention.
    Nonce(Address),
    /// Attester stake used for weighted attestation.
    AttesterStake(Address),
    CooldownReq(Address),
    // Governance approval for slashing
    GovernanceNextProposalId,
    GovernanceProposal(u64),
    GovernanceVote(u64, Address),
    GovernanceDelegate(Address),
    GovernanceGovernors,
    GovernanceQuorumBps,
    GovernanceMinGovernors,
    // Bond creation fee
    FeeTreasury,
    FeeBps,
    // Evidence storage
    EvidenceCounter,
    Evidence(u64),
    ProposalEvidence(u64),
    HashExists(String),
    // Pause mechanism
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    // USDC token used for bond operations requiring token transfers.
    BondToken,
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    /// Initialize the contract (set admin).
    fn acquire_lock(e: &Env) {
        if Self::check_lock(e) {
            panic!("reentrancy detected");
        }
        e.storage().instance().set(&Self::lock_key(e), &true);
    }

    fn release_lock(e: &Env) {
        e.storage().instance().set(&Self::lock_key(e), &false);
    }

    fn check_lock(e: &Env) -> bool {
        e.storage()
            .instance()
            .get(&Self::lock_key(e))
            .unwrap_or(false)
    }

    fn lock_key(e: &Env) -> Symbol {
        Symbol::new(e, "lock")
    }

    fn callback_key(e: &Env) -> Symbol {
        Symbol::new(e, "callback")
    }

    #[allow(dead_code)]
    fn with_reentrancy_guard<T, F: FnOnce() -> T>(e: &Env, f: F) -> T {
        if Self::check_lock(e) {
            panic!("reentrancy detected");
        }
        Self::acquire_lock(e);
        let result = f();
        Self::release_lock(e);
        result
    }

    fn require_admin_internal(e: &Env, admin: &Address) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        if stored_admin != *admin {
            panic!("not admin");
        }
    }

    /// Initialize the contract (admin).
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&DataKey::Admin, &admin);
        // Initialize pause state
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);
        // Keep legacy admin key for shared access-control helpers.
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "admin"), &admin);
    }

    /// Set early exit penalty config (admin only). Penalty in basis points (e.g. 500 = 5%).
    pub fn set_early_exit_config(e: Env, admin: Address, treasury: Address, penalty_bps: u32) {
        pausable::require_not_paused(&e);
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        if admin != stored_admin {
            panic!("not admin");
        Self::require_admin_internal(&e, &admin);
        early_exit_penalty::set_config(&e, treasury, penalty_bps);
    }

    /// @notice Configure emergency withdrawal controls.
    /// @dev Requires admin authorization and stores governance approver, treasury, fee, and enabled mode.
    /// @param admin Admin address authorized to configure emergency settings.
    /// @param governance Governance address required for elevated approval on emergency withdrawals.
    /// @param treasury Treasury receiving emergency fees.
    /// @param emergency_fee_bps Emergency fee in basis points (max 10000).
    /// @param enabled Initial emergency mode state.
    pub fn set_emergency_config(
        e: Env,
        admin: Address,
        governance: Address,
        treasury: Address,
        emergency_fee_bps: u32,
        enabled: bool,
    ) {
        Self::require_admin_internal(&e, &admin);
        admin.require_auth();
        emergency::set_config(&e, governance, treasury, emergency_fee_bps, enabled);
    }

    /// @notice Toggle emergency mode with elevated governance approval.
    /// @dev Requires both admin and configured governance approvals.
    /// @param admin Admin approver.
    /// @param governance Governance approver.
    /// @param enabled New emergency mode status.
    pub fn set_emergency_mode(e: Env, admin: Address, governance: Address, enabled: bool) {
        Self::require_admin_internal(&e, &admin);
        let cfg = emergency::get_config(&e);
        if governance != cfg.governance {
            panic!("not governance");
        }
        admin.require_auth();
        governance.require_auth();
        emergency::set_enabled(&e, enabled);
        emergency::emit_emergency_mode_event(&e, enabled, &admin, &governance);
    }

    /// @notice Execute emergency withdrawal during crisis mode.
    /// @dev Requires elevated approval from both admin and governance, applies emergency fee, emits event, and writes immutable audit record.
    /// @param admin Admin approver for emergency override.
    /// @param governance Governance approver for emergency override.
    /// @param amount Gross amount withdrawn from bond.
    /// @param reason Symbolic reason code for audit trail.
    /// @return Updated bond after emergency withdrawal.
    pub fn emergency_withdraw(
        e: Env,
        admin: Address,
        governance: Address,
        amount: i128,
        reason: Symbol,
    ) -> IdentityBond {
        Self::require_admin_internal(&e, &admin);

        let cfg = emergency::get_config(&e);
        if governance != cfg.governance {
            panic!("not governance");
        }
        if !cfg.enabled {
            panic!("emergency mode disabled");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }

        admin.require_auth();
        governance.require_auth();

        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");
        if amount > available {
            panic!("insufficient balance for withdrawal");
        }

        let fee_amount = emergency::calculate_fee(amount, cfg.emergency_fee_bps);
        let net_amount = amount
            .checked_sub(fee_amount)
            .expect("emergency fee exceeds amount");

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .expect("withdrawal caused underflow");
        if bond.slashed_amount > bond.bonded_amount {
            panic!("slashed amount exceeds bonded amount");
        }
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        let record_id = emergency::store_record(
            &e,
            bond.identity.clone(),
            amount,
            fee_amount,
            net_amount,
            cfg.treasury.clone(),
            admin,
            governance,
            reason.clone(),
        );

        emergency::emit_emergency_withdrawal_event(
            &e,
            record_id,
            &bond.identity,
            amount,
            fee_amount,
            net_amount,
            &reason,
        );

        e.storage().instance().set(&key, &bond);
        bond
    }

    /// @notice Return current emergency configuration.
    /// @return Emergency configuration struct.
    pub fn get_emergency_config(e: Env) -> emergency::EmergencyConfig {
        emergency::get_config(&e)
    }

    /// @notice Return latest emergency withdrawal record id (0 when none).
    /// @return Latest record id.
    pub fn get_latest_emergency_record_id(e: Env) -> u64 {
        emergency::latest_record_id(&e)
    }

    /// @notice Return immutable emergency withdrawal record by id.
    /// @param id Emergency record id.
    /// @return Emergency withdrawal audit record.
    pub fn get_emergency_record(e: Env, id: u64) -> emergency::EmergencyWithdrawalRecord {
        emergency::get_record(&e, id)
    }

    pub fn register_attester(e: Env, attester: Address) {
        pausable::require_not_paused(&e);
        let _admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        require_admin(&e, &_admin);
        _admin.require_auth();
        add_verifier_role(&e, &_admin, &attester);
        Self::require_admin_internal(&e, &admin);
        admin.require_auth();
        add_verifier_role(&e, &admin, &attester);
        e.storage()
            .instance()
            .set(&DataKey::Attester(attester.clone()), &true);
        // Ensure verifier info exists for reputation tracking (legacy admin path).
        verifier::register_legacy(&e, &attester);
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), attester);
    }

    pub fn unregister_attester(e: Env, attester: Address) {
        pausable::require_not_paused(&e);
        let _admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        require_admin(&e, &_admin);
        _admin.require_auth();
        remove_verifier_role(&e, &_admin, &attester);
        Self::require_admin_internal(&e, &admin);
        admin.require_auth();
        remove_verifier_role(&e, &admin, &attester);
        verifier::deactivate_if_exists(&e, &attester, Symbol::new(&e, "admin"));
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), attester);
    }

    pub fn is_attester(e: Env, attester: Address) -> bool {
        is_verifier(&e, &attester)
    }

    /// @notice Set the minimum stake required to register/activate as a verifier (admin only).
    pub fn set_verifier_stake_requirement(e: Env, admin: Address, min_stake: i128) {
        admin.require_auth();
        Self::require_admin_internal(&e, &admin);
        verifier::set_min_stake(&e, min_stake);
    }

    /// @notice Get the minimum stake required to register/activate as a verifier.
    pub fn get_verifier_stake_requirement(e: Env) -> i128 {
        verifier::get_min_stake(&e)
    }

    /// @notice Register (or reactivate) as a verifier by staking the configured token.
    /// @dev Caller must approve the contract to transfer the stake amount via `transfer_from`.
    pub fn register_verifier(
        e: Env,
        verifier_addr: Address,
        stake_deposit: i128,
    ) -> verifier::VerifierInfo {
        verifier_addr.require_auth();
        Self::with_reentrancy_guard(&e, || {
            verifier::register_with_stake(&e, &verifier_addr, stake_deposit)
        })
    }

    /// @notice Deactivate the caller as a verifier (self-deactivation).
    pub fn deactivate_verifier(e: Env, verifier_addr: Address) -> verifier::VerifierInfo {
        verifier_addr.require_auth();
        verifier::deactivate_verifier(&e, &verifier_addr, Symbol::new(&e, "self"))
    }

    /// @notice Deactivate a verifier (admin only).
    pub fn deactivate_verifier_by_admin(
        e: Env,
        admin: Address,
        verifier_addr: Address,
    ) -> verifier::VerifierInfo {
        admin.require_auth();
        Self::require_admin_internal(&e, &admin);
        verifier::deactivate_verifier(&e, &verifier_addr, Symbol::new(&e, "admin"))
    }

    /// @notice Withdraw staked tokens after deactivation.
    pub fn withdraw_verifier_stake(
        e: Env,
        verifier_addr: Address,
        amount: i128,
    ) -> verifier::VerifierInfo {
        verifier_addr.require_auth();
        Self::with_reentrancy_guard(&e, || verifier::withdraw_stake(&e, &verifier_addr, amount))
    }

    /// @notice Get verifier info (stake, reputation, status), if present.
    pub fn get_verifier_info(e: Env, verifier_addr: Address) -> Option<verifier::VerifierInfo> {
        verifier::get_verifier_info(&e, &verifier_addr)
    }

    /// @notice Set verifier reputation (admin only).
    pub fn set_verifier_reputation(
        e: Env,
        admin: Address,
        verifier_addr: Address,
        new_reputation: i128,
    ) {
        admin.require_auth();
        Self::require_admin_internal(&e, &admin);
        verifier::set_reputation(&e, &verifier_addr, new_reputation, Symbol::new(&e, "admin"));
    }

    /// Set the token contract address (admin only). Required before `create_bond`, `top_up`,
    /// and `withdraw_bond`.
    pub fn set_token(e: Env, admin: Address, token: Address) {
        token_integration::set_token(&e, &admin, &token);
    }

    /// @notice Set the USDC token contract and network label (admin only).
    /// @dev Network label must be either "mainnet" or "testnet".
    pub fn set_usdc_token(e: Env, admin: Address, token: Address, network: String) {
        token_integration::set_usdc_token(&e, &admin, &token, &network);
    }

    /// @notice Return configured USDC token contract address.
    pub fn get_usdc_token(e: Env) -> Address {
        token_integration::get_token(&e)
    }

    /// @notice Return configured USDC network label if set.
    pub fn get_usdc_network(e: Env) -> Option<String> {
        token_integration::get_usdc_network(&e)
    }

    /// Create or top-up a bond for an identity (non-rolling helper).
    pub fn create_bond(e: Env, identity: Address, amount: i128, duration: u64) -> IdentityBond {
        Self::create_bond_with_rolling(e, identity, amount, duration, false, 0)
    }

    /// Create a bond with rolling parameters.
    pub fn create_bond_with_rolling(
    /// Create a bond for an identity.
    /// Transfers USDC from the identity to the contract (token must be set and approved).
    /// Bond creation fee (if configured) is deducted and recorded for the treasury.
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period: u64,
    ) -> IdentityBond {
        pausable::require_not_paused(&e);
        // Validate bond amount before creating the bond
        validation::validate_bond_amount(amount);
        
        // Validate bond duration is within allowed range
        validation::validate_bond_duration(duration);
        Self::create_bond_with_rolling(
            e,
            identity,
            amount,
            duration,
            is_rolling,
            notice_period_duration,
        )
    }

    /// Create a bond with rolling parameters.
    pub fn create_bond_with_rolling(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
        if amount < 0 {
            panic!("amount must be non-negative");
        }
        token_integration::transfer_into_contract(&e, &identity, amount);

        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("token not set"));
        let contract = e.current_contract_address();
        TokenClient::new(&e, &token).transfer_from(&contract, &identity, &contract, &amount);
        let bond_start = e.ledger().timestamp();

        // Verify end timestamp wouldn't overflow.
        let _end_timestamp = bond_start
            .checked_add(duration)
            .expect("bond end timestamp would overflow");

        let (fee, net_amount) = fees::calculate_fee(&e, amount);
        if fee > 0 {
            let (treasury_opt, _) = fees::get_config(&e);
            if let Some(treasury) = treasury_opt {
                fees::record_fee(&e, &identity, amount, fee, &treasury);
            }
        }

        let bond = IdentityBond {
            identity: identity.clone(),
            bonded_amount: net_amount,
            bond_start,
            bond_duration: duration,
            slashed_amount: 0,
            active: true,
            is_rolling,
            withdrawal_requested_at: 0,
            notice_period,
        };
        let key = DataKey::Bond;
        e.storage().instance().set(&key, &bond);

        e.storage().instance().set(&DataKey::Bond, &bond);

        let old_tier = BondTier::Bronze;
        let new_tier = tiered_bond::get_tier_for_amount(net_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &identity, old_tier, new_tier);

        events::emit_bond_created(&e, &identity, amount, duration, is_rolling);
        bond
    }

    pub fn get_identity_state(e: Env) -> IdentityBond {
        e.storage()
            .instance()
            .get::<_, IdentityBond>(&DataKey::Bond)
            .unwrap_or_else(|| panic!("no bond"))
    }

    /// Add an attestation for a subject (only authorized attesters can call).
    /// Requires correct nonce for replay prevention; rejects duplicate (verifier, identity, data).
    /// Weight is computed from attester stake.
    pub fn add_attestation(
        e: Env,
        attester: Address,
        subject: Address,
        attestation_data: String,
    ) -> Attestation {
        attester.require_auth();
        require_verifier(&e, &attester);

        // Verify attester is authorized
        let is_authorized = e
        let is_authorized: bool = e
            .storage()
            .instance()
            .get(&DataKey::Attester(attester.clone()))
            .unwrap_or(false);

        if !is_authorized {
            panic!("unauthorized attester");
        }

        // 2. NEW: Duplicate Check Logic
        // We create a unique key based on the content of the attestation
        let dup_key =
            DataKey::DuplicateCheck(attester.clone(), subject.clone(), attestation_data.clone());

        if e.storage().instance().has(&dup_key) {
            panic!("duplicate attestation");
        }
        // --- THE FIX: Mark this as "seen" so the NEXT call fails ---
        e.storage().instance().set(&dup_key, &true);
        // Get and increment attestation counter
        let counter_key = DataKey::AttestationCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);

        let next_id = id.checked_add(1).expect("attestation counter overflow");
        e.storage().instance().set(&counter_key, &next_id);

        // Create attestation
        let attestation = Attestation {
            id,
            attester: attester.clone(),
            subject: subject.clone(),
            attestation_data: attestation_data.clone(),
            timestamp: e.ledger().timestamp(),
            revoked: false,
        };

        // Store attestation
        e.storage()
            .instance()
            .set(&DataKey::Attestation(id), &attestation);

        // Add to subject's attestation list
        let subject_key = DataKey::SubjectAttestations(subject.clone());
        let mut attestations: Vec<u64> = e
            .storage()
            .instance()
            .get(&subject_key)
            .unwrap_or(Vec::new(&e));
        attestations.push_back(id);
        e.storage().instance().set(&subject_key, &attestations);

        // Emit event
        e.events().publish(
            (Symbol::new(&e, "attestation_added"), subject),
            (id, attester, attestation_data),
        );

        verifier::record_attestation_issued(&e, &attestation.verifier, attestation.weight);

        attestation
    }

    /// Revoke an attestation (only the original attester can revoke).
    pub fn revoke_attestation(e: Env, attester: Address, attestation_id: u64) {
    /// Revoke an attestation (only original attester). Requires correct nonce.
    pub fn revoke_attestation(e: Env, attester: Address, attestation_id: u64, nonce: u64) {
        pausable::require_not_paused(&e);
        attester.require_auth();

        // Get attestation
        let key = DataKey::Attestation(attestation_id);
        let mut attestation: Attestation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("attestation not found"));

        // Verify attester is the original attester
        if attestation.attester != attester {
            panic!("only original attester can revoke");
        }

        // Check if already revoked
        if attestation.revoked {
            panic!("attestation already revoked");
        }

        // Mark as revoked
        attestation.revoked = true;
        e.storage().instance().set(&key, &attestation);

        // Emit event
        e.events().publish(
            (
                Symbol::new(&e, "attestation_revoked"),
                attestation.subject.clone(),
            ),
            (attestation_id, attester),
        );

        verifier::record_attestation_revoked(&e, &attestation.verifier, attestation.weight);
    }

    pub fn get_attestation(e: Env, attestation_id: u64) -> Attestation {
        e.storage()
            .instance()
            .get(&DataKey::Attestation(attestation_id))
            .unwrap_or_else(|| panic!("attestation not found"))
    }

    pub fn get_subject_attestations(e: Env, subject: Address) -> Vec<u64> {
        e.storage()
            .instance()
            .get(&DataKey::SubjectAttestations(subject))
            .unwrap_or(Vec::new(&e))
    }

    /// Withdraw from bond. Checks that the bond has sufficient balance after accounting for slashed amount.
    /// Returns the updated bond with reduced bonded_amount.
    pub fn get_subject_attestation_count(e: Env, subject: Address) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::SubjectAttestationCount(subject))
            .unwrap_or(0)
    }

    pub fn get_nonce(e: Env, identity: Address) -> u64 {
        nonce::get_nonce(&e, &identity)
    }

    pub fn set_attester_stake(e: Env, admin: Address, attester: Address, amount: i128) {
        Self::require_admin_internal(&e, &admin);
        weighted_attestation::set_attester_stake(&e, &attester, amount);
    }

    pub fn set_weight_config(e: Env, admin: Address, multiplier_bps: u32, max_weight: u32) {
        Self::require_admin_internal(&e, &admin);
        weighted_attestation::set_weight_config(&e, multiplier_bps, max_weight);
    }

    pub fn get_weight_config(e: Env) -> (u32, u32) {
        weighted_attestation::get_weight_config(&e)
    }

    /// Withdraw from bond (no penalty). Alias for `withdraw_bond`. Use when lock-up has ended
    /// or after the notice period for rolling bonds.
    pub fn withdraw(e: Env, amount: i128) -> IdentityBond {
        Self::withdraw_bond(e, amount)
    }

    /// Withdraw USDC from bond after lock-up has elapsed and (for rolling bonds) the cooldown
    /// window has passed. Verifies:
    /// 1. Lock-up period has elapsed for non-rolling bonds.
    /// 2. For rolling bonds, withdrawal was requested and the notice period has elapsed.
    /// 3. `amount` does not exceed the available balance (`bonded_amount - slashed_amount`).
    /// Transfers USDC to the identity owner and updates tiers.
    pub fn withdraw_bond(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        let now = e.ledger().timestamp();
        let end = bond.bond_start.saturating_add(bond.bond_duration);

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                panic!("cooldown window not elapsed; request_withdrawal first");
            }
            if !rolling_bond::can_withdraw_after_notice(
                now,
                bond.withdrawal_requested_at,
                bond.notice_period_duration,
            ) {
                panic!("cooldown window not elapsed; request_withdrawal first");
            }
        } else if now < end {
            panic!("lock-up period not elapsed; use withdraw_early");
        }

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");

        if amount > available {
            panic!("insufficient balance for withdrawal");
        }

        token_integration::transfer_from_contract(&e, &bond.identity, amount);

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .expect("withdrawal caused underflow");

        if bond.slashed_amount > bond.bonded_amount {
            bond.slashed_amount = bond.bonded_amount;
        }
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount + amount);
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);

        events::emit_bond_withdrawn(&e, &bond.identity, amount, bond.bonded_amount);
        bond
    }

    /// Early withdrawal path (only valid before lock-up end). Applies an early exit penalty and
    /// transfers the penalty to the configured treasury.
    pub fn withdraw_early(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        let now = e.ledger().timestamp();
        let end = bond.bond_start.saturating_add(bond.bond_duration);
        if now >= end {
            panic!("use withdraw for post lock-up");
        }

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");
        if amount > available {
            panic!("insufficient balance for withdrawal");
        }

        let (treasury, penalty_bps) = early_exit_penalty::get_config(&e);
        let remaining = end.saturating_sub(now);
        let penalty = early_exit_penalty::calculate_penalty(
            amount,
            remaining,
            bond.bond_duration,
            penalty_bps,
        );
        early_exit_penalty::emit_penalty_event(&e, &bond.identity, amount, penalty, &treasury);

        let net_amount = amount.checked_sub(penalty).expect("penalty exceeds amount");
        token_integration::transfer_from_contract(&e, &bond.identity, net_amount);
        if penalty > 0 {
            token_integration::transfer_from_contract(&e, &treasury, penalty);
        }
        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .expect("withdrawal caused underflow");

        if bond.slashed_amount > bond.bonded_amount {
            panic!("slashed amount exceeds bonded amount");
        }

        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        events::emit_bond_withdrawn(&e, &bond.identity, amount, bond.bonded_amount);
        bond
    }

    pub fn request_withdrawal(e: Env) -> IdentityBond {
        pausable::require_not_paused(&e);
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));
        if !bond.is_rolling {
            panic!("not a rolling bond");
        }
        if bond.withdrawal_requested_at != 0 {
            panic!("withdrawal already requested");
        }

        bond.withdrawal_requested_at = e.ledger().timestamp();
        e.storage().instance().set(&key, &bond);
        e.events().publish(
            (Symbol::new(&e, "withdrawal_requested"),),
            (bond.identity.clone(), bond.withdrawal_requested_at),
        );
        bond
    }

    pub fn renew_if_rolling(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));
        if !bond.is_rolling {
            return bond;
        }

        let now = e.ledger().timestamp();
        if !rolling_bond::is_period_ended(now, bond.bond_start, bond.bond_duration) {
            return bond;
        }

        rolling_bond::apply_renewal(&mut bond, now);
        e.storage().instance().set(&key, &bond);
        e.events().publish(
            (Symbol::new(&e, "bond_renewed"),),
            (bond.identity.clone(), bond.bond_start, bond.bond_duration),
        );
        bond
    }

    pub fn get_tier(e: Env) -> BondTier {
        let bond = Self::get_identity_state(e);
        tiered_bond::get_tier_for_amount(bond.bonded_amount)
    }

    /// Slash a portion of the bond. Increases slashed_amount up to the bonded_amount.
    /// Returns the updated bond with increased slashed_amount.
    pub fn slash(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Calculate new slashed amount, checking for overflow
        let new_slashed = bond
            .slashed_amount
            .checked_add(amount)
            .expect("slashing caused overflow");

        // Cap slashed amount at bonded amount
        bond.slashed_amount = if new_slashed > bond.bonded_amount {
            bond.bonded_amount
        } else {
            new_slashed
        };

        e.storage().instance().set(&key, &bond);
        bond
    pub fn slash(e: Env, admin: Address, amount: i128) -> IdentityBond {
        slashing::slash_bond(&e, &admin, amount)
    }

    pub fn initialize_governance(
        e: Env,
        admin: Address,
        governors: Vec<Address>,
        quorum_bps: u32,
        min_governors: u32,
    ) {
        pausable::require_not_paused(&e);
        Self::require_admin_internal(&e, &admin);
        governance_approval::initialize_governance(&e, governors, quorum_bps, min_governors);
    }

    pub fn propose_slash(e: Env, proposer: Address, amount: i128) -> u64 {
        pausable::require_not_paused(&e);
        proposer.require_auth();
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        let governors = governance_approval::get_governors(&e);
        let is_governor = governors.iter().any(|g| g == proposer);
        if proposer != admin && !is_governor {
            panic!("not admin or governor");
        }
        governance_approval::propose_slash(&e, &proposer, amount)
    }

    pub fn governance_vote(e: Env, voter: Address, proposal_id: u64, approve: bool) {
        pausable::require_not_paused(&e);
        voter.require_auth();
        governance_approval::vote(&e, &voter, proposal_id, approve);
    }

    pub fn governance_delegate(e: Env, governor: Address, to: Address) {
        pausable::require_not_paused(&e);
        governance_approval::delegate(&e, &governor, &to);
    }

    pub fn execute_slash_with_governance(
        e: Env,
        proposer: Address,
        proposal_id: u64,
    ) -> IdentityBond {
        pausable::require_not_paused(&e);
        proposer.require_auth();
        let proposal = governance_approval::get_proposal(&e, proposal_id)
            .unwrap_or_else(|| panic!("proposal not found"));
        if proposal.proposed_by != proposer {
            panic!("only proposer can execute");
        }
        let executed = governance_approval::execute_slash_if_approved(&e, proposal_id);
        if !executed {
            panic!("proposal not approved");
        }
        slashing::slash_bond(&e, &proposer, proposal.amount)
    }

    pub fn set_fee_config(e: Env, admin: Address, treasury: Address, fee_bps: u32) {
        pausable::require_not_paused(&e);
        Self::require_admin_internal(&e, &admin);
        fees::set_config(&e, treasury, fee_bps);
    }

    // State update BEFORE external interaction (checks-effects-interactions)

    pub fn get_fee_config(e: Env) -> (Option<Address>, u32) {
        fees::get_config(&e)
    }

    pub fn deposit_fees(e: Env, amount: i128) {
        let key = Symbol::new(&e, "fees");
        let current: i128 = e.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(amount).expect("fee pool overflow");
        e.storage().instance().set(&key, &next);
    }

    pub fn set_callback(e: Env, callback: Address) {
        e.storage()
            .instance()
            .set(&Self::callback_key(&e), &callback);
    }

    /// Configure the USDC token contract used by `increase_bond`.
    /// Only admin may set this.
    pub fn set_bond_token(e: Env, admin: Address, token: Address) {
        Self::require_admin_internal(&e, &admin);
        e.storage().instance().set(&DataKey::BondToken, &token);
    }

    /// Return configured USDC token contract address, if any.
    pub fn get_bond_token(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::BondToken)
    }

    pub fn is_locked(e: Env) -> bool {
        e.storage()
            .instance()
            .get(&Self::lock_key(&e))
            .unwrap_or(false)
    }

    pub fn get_slash_proposal(
        e: Env,
        proposal_id: u64,
    ) -> Option<governance_approval::SlashProposal> {
        governance_approval::get_proposal(&e, proposal_id)
    }

    pub fn get_governance_vote(e: Env, proposal_id: u64, voter: Address) -> Option<bool> {
        governance_approval::get_vote(&e, proposal_id, &voter)
    }

    // State update BEFORE external interaction

    pub fn get_governors(e: Env) -> Vec<Address> {
        governance_approval::get_governors(&e)
    }

    pub fn get_governance_delegate(e: Env, governor: Address) -> Option<Address> {
        governance_approval::get_delegate(&e, &governor)
    }

    pub fn get_quorum_config(e: Env) -> (u32, u32) {
        governance_approval::get_quorum_config(&e)
    }

    pub fn top_up(e: Env, amount: i128) -> IdentityBond {
        // Validate the top-up amount meets minimum requirements
        if amount < validation::MIN_BOND_AMOUNT {
            panic!(
                "top-up amount below minimum required: {} (minimum: {})",
                amount,
                validation::MIN_BOND_AMOUNT
            );
        }
        
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Calculate the new bonded amount after top-up
        let new_bonded_amount = bond
        // Perform top-up with overflow protection
        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
        // Overflow check before token transfer (CEI pattern)
        let new_bonded = bond
            .bonded_amount
            .checked_add(amount)
            .expect("top-up caused overflow");
            
        // Validate the new total bonded amount is within limits
        validation::validate_bond_amount(new_bonded_amount);

        // Perform top-up with overflow protection
        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = new_bonded_amount;

        token_integration::transfer_into_contract(&e, &bond.identity, amount);

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = new_bonded;
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);
        events::emit_bond_increased(&e, &bond.identity, amount, bond.bonded_amount);

        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bond
    }

    /// Increase the bond with additional USDC from the caller.
    /// Requires caller authentication and ownership of the existing bond.
    /// Transfers USDC from caller to this contract using token allowance, then
    /// updates storage and emits `bond_increased`.
    ///
    /// Security notes:
    /// - Amount must be strictly positive.
    /// - Arithmetic is checked for overflow.
    /// - Reentrancy guard protects this external token interaction.
    ///
    /// Panics if no bond exists, if caller is not owner, if token is not configured,
    /// or if transfer/overflow checks fail.
    pub fn increase_bond(e: Env, caller: Address, amount: i128) -> IdentityBond {
        caller.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::with_reentrancy_guard(&e, || {
            let key = DataKey::Bond;
            let mut bond = e
                .storage()
                .instance()
                .get::<_, IdentityBond>(&key)
                .unwrap_or_else(|| panic!("no bond"));

            if bond.identity != caller {
                panic!("not bond owner");
            }

            let token_addr: Address = e
                .storage()
                .instance()
                .get(&DataKey::BondToken)
                .unwrap_or_else(|| panic!("bond token not configured"));

            let old_amount = bond.bonded_amount;
            let new_amount = old_amount
                .checked_add(amount)
                .expect("bond increase caused overflow");

            let token_client = TokenClient::new(&e, &token_addr);
            let contract_address = e.current_contract_address();
            token_client.transfer_from(&contract_address, &caller, &contract_address, &amount);

            let old_tier = tiered_bond::get_tier_for_amount(old_amount);
            let new_tier = tiered_bond::get_tier_for_amount(new_amount);

            bond.bonded_amount = new_amount;
            e.storage().instance().set(&key, &bond);

            tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);
            e.events().publish(
                (Symbol::new(&e, "bond_increased"), bond.identity.clone()),
                (amount, old_amount, new_amount),
            );

            bond
        })
    }

    pub fn extend_duration(e: Env, additional_duration: u64) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));

        bond.bond_duration = bond
            .bond_duration
            .checked_add(additional_duration)
            .expect("duration extension caused overflow");

        let _end_timestamp = bond
            .bond_start
            .checked_add(bond.bond_duration)
            .expect("bond end timestamp would overflow");

        e.storage().instance().set(&key, &bond);
        bond
    }

    // ==================== Evidence Storage ====================

    /// Submit evidence hash for a slash proposal.
    ///
    /// @param e Contract environment
    /// @param submitter Address submitting the evidence (must be authorized)
    /// @param proposal_id ID of the slash proposal
    /// @param hash Content hash (IPFS CID, SHA-256, etc.)
    /// @param hash_type Type of hash
    /// @param description Optional description/metadata
    /// @return Evidence ID
    ///
    /// # Panics
    /// * If hash is empty
    /// * If hash already exists
    /// * If submitter is not authorized
    pub fn submit_evidence(
        e: Env,
        submitter: Address,
        proposal_id: u64,
        hash: String,
        hash_type: EvidenceType,
        description: Option<String>,
    ) -> u64 {
        submitter.require_auth();
        evidence::submit_evidence(&e, &submitter, proposal_id, &hash, &hash_type, &description)
    }

    /// Get evidence by ID.
    ///
    /// @param e Contract environment
    /// @param evidence_id Unique evidence identifier
    /// @return Evidence record
    ///
    /// # Panics
    /// If evidence ID does not exist
    pub fn get_evidence(e: Env, evidence_id: u64) -> Evidence {
        evidence::get_evidence(&e, evidence_id)
    }

    /// Get all evidence IDs for a slash proposal.
    ///
    /// @param e Contract environment
    /// @param proposal_id Slash proposal ID
    /// @return Vector of evidence IDs
    pub fn get_proposal_evidence(e: Env, proposal_id: u64) -> Vec<u64> {
        evidence::get_proposal_evidence(&e, proposal_id)
    }

    /// Get all evidence details for a proposal.
    ///
    /// @param e Contract environment
    /// @param proposal_id Slash proposal ID
    /// @return Vector of complete Evidence records
    pub fn get_proposal_evidence_details(e: Env, proposal_id: u64) -> Vec<Evidence> {
        evidence::get_proposal_evidence_details(&e, proposal_id)
    }

    /// Check if a hash already exists.
    ///
    /// @param e Contract environment
    /// @param hash Content hash to check
    /// @return true if hash exists
    pub fn evidence_hash_exists(e: Env, hash: String) -> bool {
        evidence::hash_exists(&e, &hash)
    }

    /// Get total evidence count.
    ///
    /// @param e Contract environment
    /// @return Total number of evidence submissions
    pub fn get_evidence_count(e: Env) -> u64 {
        evidence::get_evidence_count(&e)
    }

    // ==================== Protocol Parameters (Governance-Controlled) ====================

    // ==================== Batch Operations ====================

    /// Create multiple bonds atomically in a single transaction.
    ///
    /// All bonds are validated before any are created. If any bond fails validation,
    /// the entire batch is rejected (all-or-nothing atomicity).
    ///
    /// @param e Contract environment
    /// @param params_list Vector of bond creation parameters
    /// @return BatchBondResult containing created count and bond list
    ///
    /// # Panics
    /// * If validation fails for any bond
    /// * If params_list is empty
    /// * If a bond already exists for any identity
    ///
    /// # Events
    /// Emits `batch_bonds_created` with the batch result
    ///
    /// # Example
    /// ```ignore
    /// let params = vec![
    ///     BatchBondParams {
    ///         identity: addr1,
    ///         amount: 1000,
    ///         duration: 86400,
    ///         is_rolling: false,
    ///         notice_period_duration: 0,
    ///     },
    /// ];
    /// let result = client.create_batch_bonds(&params);
    /// assert_eq!(result.created_count, 1);
    /// ```
    pub fn create_batch_bonds(e: Env, params_list: Vec<BatchBondParams>) -> BatchBondResult {
        batch::create_batch_bonds(&e, params_list)
    }

    /// Validate a batch of bonds without creating them.
    ///
    /// Useful for pre-flight checks before submitting a batch transaction.
    ///
    /// @param e Contract environment
    /// @param params_list Vector of bond creation parameters to validate
    /// @return true if all bonds are valid
    ///
    /// # Panics
    /// * If any bond has invalid parameters
    pub fn validate_batch_bonds(e: Env, params_list: Vec<BatchBondParams>) -> bool {
        batch::validate_batch(&e, params_list)
    }

    /// Get the total bonded amount across a batch.
    ///
    /// @param params_list Vector of bond creation parameters
    /// @return Total amount across all bonds
    ///
    /// # Panics
    /// * If the total would overflow i128
    pub fn get_batch_total_amount(params_list: Vec<BatchBondParams>) -> i128 {
        batch::get_batch_total_amount(&params_list)
    }

    // ==================== Protocol Parameters (Governance-Controlled) ====================

    // ==================== Reentrancy Test Functions ====================
    /// Check if the reentrancy lock is currently held.
    pub fn is_locked(e: Env) -> bool {
        Self::check_lock(&e)
    }

    /// Get protocol fee rate in basis points.
    pub fn get_protocol_fee_bps(e: Env) -> u32 {
        parameters::get_protocol_fee_bps(&e)
    }

    /// Set protocol fee rate. Governance-only.
    pub fn set_protocol_fee_bps(e: Env, admin: Address, value: u32) {
        parameters::set_protocol_fee_bps(&e, &admin, value)
    }

    /// Get attestation fee rate in basis points.
    pub fn get_attestation_fee_bps(e: Env) -> u32 {
        parameters::get_attestation_fee_bps(&e)
    }

    /// Set attestation fee rate. Governance-only.
    pub fn set_attestation_fee_bps(e: Env, admin: Address, value: u32) {
        parameters::set_attestation_fee_bps(&e, &admin, value)
    }

    /// Get withdrawal cooldown period in seconds.
    pub fn get_withdrawal_cooldown_secs(e: Env) -> u64 {
        parameters::get_withdrawal_cooldown_secs(&e)
    }

    /// Set withdrawal cooldown period. Governance-only.
    pub fn set_withdrawal_cooldown_secs(e: Env, admin: Address, value: u64) {
        parameters::set_withdrawal_cooldown_secs(&e, &admin, value)
    }

    /// Get slash cooldown period in seconds.
    pub fn get_slash_cooldown_secs(e: Env) -> u64 {
        parameters::get_slash_cooldown_secs(&e)
    }

    /// Set slash cooldown period. Governance-only.
    pub fn set_slash_cooldown_secs(e: Env, admin: Address, value: u64) {
        parameters::set_slash_cooldown_secs(&e, &admin, value)
    }

    /// Get bronze tier threshold.
    pub fn get_bronze_threshold(e: Env) -> i128 {
        parameters::get_bronze_threshold(&e)
    }

    /// Set bronze tier threshold. Governance-only.
    pub fn set_bronze_threshold(e: Env, admin: Address, value: i128) {
        parameters::set_bronze_threshold(&e, &admin, value)
    }

    /// Get silver tier threshold.
    pub fn get_silver_threshold(e: Env) -> i128 {
        parameters::get_silver_threshold(&e)
    }

    /// Set silver tier threshold. Governance-only.
    pub fn set_silver_threshold(e: Env, admin: Address, value: i128) {
        parameters::set_silver_threshold(&e, &admin, value)
    }

    /// Get gold tier threshold.
    pub fn get_gold_threshold(e: Env) -> i128 {
        parameters::get_gold_threshold(&e)
    }

    /// Set gold tier threshold. Governance-only.
    pub fn set_gold_threshold(e: Env, admin: Address, value: i128) {
        parameters::set_gold_threshold(&e, &admin, value)
    }

    /// Get platinum tier threshold.
    pub fn get_platinum_threshold(e: Env) -> i128 {
        parameters::get_platinum_threshold(&e)
    }

    /// Set platinum tier threshold. Governance-only.
    pub fn set_platinum_threshold(e: Env, admin: Address, value: i128) {
        parameters::set_platinum_threshold(&e, &admin, value)
    }

    // ==================== Reentrancy Test Functions ====================

    /// Withdraw the full bonded amount back to the identity (callback-based, for reentrancy tests).
    /// Uses a reentrancy guard to prevent re-entrance during external calls.
    pub fn withdraw_bond_full(e: Env, identity: Address) -> i128 {
        identity.require_auth();
        Self::acquire_lock(&e);

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic!("no bond"));

        if bond.identity != identity {
            Self::release_lock(&e);
            panic!("not bond owner");
        }
        if !bond.active {
            Self::release_lock(&e);
            panic!("bond not active");
        }

        let withdraw_amount = bond.bonded_amount - bond.slashed_amount;

        // State update BEFORE external interaction (checks-effects-interactions)
        let updated = IdentityBond {
            identity: identity.clone(),
            bonded_amount: 0,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: bond.slashed_amount,
            active: false,
            // Add these missing fields:
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period: bond.notice_period,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);

        // External call: invoke callback if a callback contract is registered.
        // In production this would be a token transfer; here we use a hook for testing.
        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_withdraw");
            let args: Vec<Val> = Vec::from_array(&e, [withdraw_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        withdraw_amount
    }

    /// Slash a portion of a bond. Only callable by admin.
    /// Uses a reentrancy guard to prevent re-entrance during external calls.
    pub fn slash_bond(e: Env, admin: Address, slash_amount: i128) -> i128 {
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("no admin"));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic!("not admin");
        }

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic!("no bond"));

        if !bond.active {
            Self::release_lock(&e);
            panic!("bond not active");
        }

        let new_slashed = bond.slashed_amount + slash_amount;
        if new_slashed > bond.bonded_amount {
            Self::release_lock(&e);
            panic!("slash exceeds bond");
        }

        // State update BEFORE external interaction
        let updated = IdentityBond {
            identity: bond.identity.clone(),
            bonded_amount: bond.bonded_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: new_slashed,
            active: bond.active,
            // Add these missing fields:
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period: bond.notice_period,
            is_rolling: bond.is_rolling,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            notice_period_duration: bond.notice_period_duration,
        };
        e.storage().instance().set(&bond_key, &updated);

        // External call: invoke callback if registered
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
    /// Uses a reentrancy guard to prevent re-entrance during external calls.
    pub fn collect_fees(e: Env, admin: Address) -> i128 {
        admin.require_auth();
        Self::acquire_lock(&e);

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("no admin"));
        if stored_admin != admin {
            Self::release_lock(&e);
            panic!("not admin");
        }

        let fee_key = Symbol::new(&e, "fees");
        let fees: i128 = e.storage().instance().get(&fee_key).unwrap_or(0);

        // State update BEFORE external interaction
        e.storage().instance().set(&fee_key, &0_i128);

        // External call: invoke callback if registered
        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_collect");
            let args: Vec<Val> = Vec::from_array(&e, [fees.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }

        Self::release_lock(&e);
        fees
    }

    // ------------------------------------------------------------------
    // Cooldown window methods
    // ------------------------------------------------------------------

    /// Set the cooldown period (in seconds). Only the admin may call this.
    /// @param admin Caller who must be the contract admin
    /// @param period Duration in seconds that must elapse between request and withdrawal
    pub fn set_cooldown_period(e: Env, admin: Address, period: u64) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        if admin != stored_admin {
            panic!("not admin");
        }
        admin.require_auth();

        let old = cooldown::get_cooldown_period(&e);
        cooldown::set_cooldown_period(&e, period);
        cooldown::emit_cooldown_period_updated(&e, old, period);
    }

    /// Read the current cooldown period.
    pub fn get_cooldown_period(e: Env) -> u64 {
        cooldown::get_cooldown_period(&e)
    }

    /// Request a cooldown withdrawal. Records the caller's intent plus the
    /// requested amount and the current ledger timestamp. Panics if a request
    /// already exists for the same address, or if the amount exceeds the
    /// available bond balance.
    /// @param requester The bond holder requesting the withdrawal
    /// @param amount    The amount to withdraw after cooldown
    pub fn request_cooldown_withdrawal(
        e: Env,
        requester: Address,
        amount: i128,
    ) -> CooldownRequest {
        requester.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        // Verify a bond exists and the requester matches the bond identity
        let bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&DataKey::Bond)
            .unwrap_or_else(|| panic!("no bond"));

        if bond.identity != requester {
            panic!("requester is not the bond holder");
        }

        // Check available balance
        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");

        if amount > available {
            panic!("amount exceeds available balance");
        }

        // Reject if a cooldown request already exists for this address
        let req_key = DataKey::CooldownReq(requester.clone());
        if e.storage().instance().has(&req_key) {
            panic!("cooldown request already pending");
        }

        let request = CooldownRequest {
            requester: requester.clone(),
            amount,
            requested_at: e.ledger().timestamp(),
        };
        e.storage().instance().set(&req_key, &request);

        cooldown::emit_cooldown_requested(&e, &requester, amount);
        request
    }

    /// Execute a previously requested cooldown withdrawal. Panics if the
    /// cooldown period has not yet elapsed, no request exists, or the bond
    /// balance is insufficient at execution time.
    /// @param requester The address that originally requested the withdrawal
    pub fn execute_cooldown_withdrawal(e: Env, requester: Address) -> IdentityBond {
        requester.require_auth();

        let req_key = DataKey::CooldownReq(requester.clone());
        let request: CooldownRequest = e
            .storage()
            .instance()
            .get(&req_key)
            .unwrap_or_else(|| panic!("no cooldown request"));

        let period = cooldown::get_cooldown_period(&e);
        let now = e.ledger().timestamp();

        if !cooldown::can_withdraw(now, request.requested_at, period) {
            panic!("cooldown period has not elapsed");
        }

        // Perform the actual withdrawal on the bond
        let bond_key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&bond_key)
            .unwrap_or_else(|| panic!("no bond"));

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");

        if request.amount > available {
            panic!("insufficient balance for withdrawal");
        }

        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(request.amount)
            .expect("withdrawal caused underflow");

        if bond.slashed_amount > bond.bonded_amount {
            panic!("slashed amount exceeds bonded amount after withdrawal");
        }

        e.storage().instance().set(&bond_key, &bond);
        e.storage().instance().remove(&req_key);

        cooldown::emit_cooldown_executed(&e, &requester, request.amount);
        bond
    }

    /// Cancel a pending cooldown withdrawal request. Only the original
    /// requester may cancel.
    /// @param requester The address that originally requested the withdrawal
    pub fn cancel_cooldown(e: Env, requester: Address) {
        requester.require_auth();

        let req_key = DataKey::CooldownReq(requester.clone());
        if !e.storage().instance().has(&req_key) {
            panic!("no cooldown request to cancel");
        }

        e.storage().instance().remove(&req_key);
        cooldown::emit_cooldown_cancelled(&e, &requester);
    }

    /// Read the pending cooldown request for an address, if any.
    /// @param requester The address to query
    pub fn get_cooldown_request(e: Env, requester: Address) -> CooldownRequest {
        e.storage()
            .instance()
            .get(&DataKey::CooldownReq(requester))
            .unwrap_or_else(|| panic!("no cooldown request"))
    }
}

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_reentrancy;
mod test_attestation;

#[cfg(test)]
mod test_batch;

#[cfg(test)]
mod test_attestation_types;

#[cfg(test)]
mod test_validation;
mod test_attestation_types;

#[cfg(test)]
mod test_attestation;

#[cfg(test)]
mod test_governance_approval;

#[cfg(test)]
mod test_parameters;

#[cfg(test)]
mod test_fees;

#[cfg(test)]
mod integration;

#[cfg(test)]
mod test_increase_bond;

#[cfg(test)]
mod security;

// Pause mechanism entrypoints
#[contractimpl]
impl CredenceBond {
    pub fn is_paused(e: Env) -> bool {
        pausable::is_paused(&e)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        pausable::execute_pause_proposal(&e, proposal_id)
    }
}

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_duration_validation;

#[cfg(test)]
mod test_access_control;

mod test_cooldown;
#[cfg(test)]
mod test_events;

#[cfg(test)]
mod test_early_exit_penalty;

#[cfg(test)]
mod test_verifier;
mod test_evidence;
mod test_emergency;

#[cfg(test)]
mod test_rolling_bond;

#[cfg(test)]
mod test_tiered_bond;

#[cfg(test)]
mod test_slashing;

#[cfg(test)]
mod test_withdraw_bond;

#[cfg(test)]
mod test_math;

#[cfg(test)]
mod token_integration_test;
