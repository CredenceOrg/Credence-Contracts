#![no_std]

pub mod early_exit_penalty;
mod fees;
pub mod governance_approval;
mod nonce;
mod parameters;
mod rolling_bond;
pub mod rolling_bond;
mod slashing;
pub mod tiered_bond;
mod weighted_attestation;

pub mod types;

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

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

#[contracttype]
#[derive(Clone, Debug)]
pub struct IdentityBond {
    pub identity: Address,
    pub bonded_amount: i128,
    pub bond_start: u64,
    pub bond_duration: u64,
    pub slashed_amount: i128,
    pub active: bool,
    /// If true, bond auto-renews at period end unless withdrawal was requested.
    pub is_rolling: bool,
    /// When withdrawal was requested (0 = not requested).
    pub withdrawal_requested_at: u64,
    /// Notice period duration for rolling bonds (seconds).
    pub notice_period_duration: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Bond,
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
    /// Per-identity attestation count (updated on add/revoke).
    SubjectAttestationCount(Address),
    /// Per-identity nonce for replay prevention.
    Nonce(Address),
    /// Attester stake used for weighted attestation.
    AttesterStake(Address),
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
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    fn acquire_lock(e: &Env) {
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

    fn with_reentrancy_guard<T, F: FnOnce() -> T>(e: &Env, f: F) -> T {
        if Self::check_lock(e) {
            panic!("reentrancy detected");
        }
        Self::acquire_lock(e);
        let result = f();
        Self::release_lock(e);
        result
    }

    fn require_admin(e: &Env, admin: &Address) {
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
    }

    /// Set early exit penalty config. Only admin should call.
    pub fn set_early_exit_config(e: Env, admin: Address, treasury: Address, penalty_bps: u32) {
        Self::require_admin(&e, &admin);
        early_exit_penalty::set_config(&e, treasury, penalty_bps);
    }

    pub fn register_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        e.storage()
            .instance()
            .set(&DataKey::Attester(attester.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), attester);
    }

    pub fn unregister_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        e.storage()
            .instance()
            .remove(&DataKey::Attester(attester.clone()));
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), attester);
    }

    pub fn is_attester(e: Env, attester: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Attester(attester))
            .unwrap_or(false)
    }

    /// Create a bond for an identity.
    /// Bond creation fee (if configured) is deducted and recorded for treasury.
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
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
            notice_period_duration,
        };

        e.storage().instance().set(&DataKey::Bond, &bond);

        let old_tier = BondTier::Bronze;
        let new_tier = tiered_bond::get_tier_for_amount(net_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &identity, old_tier, new_tier);
        bond
    }

    pub fn create_bond_with_rolling(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
        Self::create_bond(
            e,
            identity,
            amount,
            duration,
            is_rolling,
            notice_period_duration,
        )
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
        nonce: u64,
    ) -> Attestation {
        attester.require_auth();

        let is_authorized: bool = e
            .storage()
            .instance()
            .get(&DataKey::Attester(attester.clone()))
            .unwrap_or(false);
        if !is_authorized {
            panic!("unauthorized attester");
        }

        nonce::consume_nonce(&e, &attester, nonce);

        let dedup_key = types::AttestationDedupKey {
            verifier: attester.clone(),
            identity: subject.clone(),
            attestation_data: attestation_data.clone(),
        };
        if e.storage().instance().has(&dedup_key) {
            panic!("duplicate attestation");
        }

        let counter_key = DataKey::AttestationCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id.checked_add(1).expect("attestation counter overflow");
        e.storage().instance().set(&counter_key, &next_id);

        let weight = weighted_attestation::compute_weight(&e, &attester);
        types::Attestation::validate_weight(weight);

        let attestation = Attestation {
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
        e.storage().instance().set(&dedup_key, &id);

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

        e.events().publish(
            (Symbol::new(&e, "attestation_added"), subject),
            (id, attester, attestation_data, weight),
        );

        attestation
    }

    /// Revoke an attestation (only original attester). Requires correct nonce.
    pub fn revoke_attestation(e: Env, attester: Address, attestation_id: u64, nonce: u64) {
        attester.require_auth();
        nonce::consume_nonce(&e, &attester, nonce);

        let key = DataKey::Attestation(attestation_id);
        let mut attestation: Attestation = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("attestation not found"));

        if attestation.verifier != attester {
            panic!("only original attester can revoke");
        }
        if attestation.revoked {
            panic!("attestation already revoked");
        }

        attestation.revoked = true;
        e.storage().instance().set(&key, &attestation);

        let dedup_key = types::AttestationDedupKey {
            verifier: attestation.verifier.clone(),
            identity: attestation.identity.clone(),
            attestation_data: attestation.attestation_data.clone(),
        };
        e.storage().instance().remove(&dedup_key);

        let count_key = DataKey::SubjectAttestationCount(attestation.identity.clone());
        let count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage()
            .instance()
            .set(&count_key, &count.saturating_sub(1));

        e.events().publish(
            (
                Symbol::new(&e, "attestation_revoked"),
                attestation.identity.clone(),
            ),
            (attestation_id, attester),
        );
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
        Self::require_admin(&e, &admin);
        weighted_attestation::set_attester_stake(&e, &attester, amount);
    }

    pub fn set_weight_config(e: Env, admin: Address, multiplier_bps: u32, max_weight: u32) {
        Self::require_admin(&e, &admin);
        weighted_attestation::set_weight_config(&e, multiplier_bps, max_weight);
    }

    pub fn get_weight_config(e: Env) -> (u32, u32) {
        weighted_attestation::get_weight_config(&e)
    }

    /// Early withdrawal path (only valid before lock-up end).
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
        bond
    }

    /// Withdraw from bond. For rolling bonds requires prior notice and elapsed notice period.
    pub fn withdraw(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));

        if bond.is_rolling {
            if bond.withdrawal_requested_at == 0 {
                panic!("withdrawal not requested");
            }
            let now = e.ledger().timestamp();
            if !rolling_bond::can_withdraw_after_notice(
                now,
                bond.withdrawal_requested_at,
                bond.notice_period_duration,
            ) {
                panic!("notice period not elapsed");
            }
        }

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");
        if amount > available {
            panic!("insufficient balance for withdrawal");
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
        bond
    }

    pub fn request_withdrawal(e: Env) -> IdentityBond {
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
        Self::require_admin(&e, &admin);
        governance_approval::initialize_governance(&e, governors, quorum_bps, min_governors);
    }

    pub fn propose_slash(e: Env, proposer: Address, amount: i128) -> u64 {
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
        voter.require_auth();
        governance_approval::vote(&e, &voter, proposal_id, approve);
    }

    pub fn governance_delegate(e: Env, governor: Address, to: Address) {
        governance_approval::delegate(&e, &governor, &to);
    }

    pub fn execute_slash_with_governance(
        e: Env,
        proposer: Address,
        proposal_id: u64,
    ) -> IdentityBond {
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
        Self::require_admin(&e, &admin);
        fees::set_config(&e, treasury, fee_bps);
    }

        // State update BEFORE external interaction (checks-effects-interactions)
        let updated = IdentityBond {
            identity: identity.clone(),
            bonded_amount: 0,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: bond.slashed_amount,
            is_rolling: bond.is_rolling,
            notice_period_duration: bond.notice_period_duration,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            active: false,
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
    pub fn get_fee_config(e: Env) -> (Option<Address>, u32) {
        fees::get_config(&e)
    }

    pub fn collect_fees(e: Env, admin: Address) -> i128 {
        Self::require_admin(&e, &admin);
        let key = Symbol::new(&e, "fees");
        let collected: i128 = e.storage().instance().get(&key).unwrap_or(0);
        e.storage().instance().set(&key, &0_i128);
        collected
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

    pub fn is_locked(e: Env) -> bool {
        e.storage()
            .instance()
            .get(&Self::lock_key(&e))
            .unwrap_or(false)
    }

    pub fn withdraw_bond(e: Env, identity: Address) -> i128 {
        let key = DataKey::Bond;
        Self::with_reentrancy_guard(&e, || {
            let mut bond: IdentityBond = e
                .storage()
                .instance()
                .get(&key)
                .unwrap_or_else(|| panic!("no bond"));
            if bond.identity != identity {
                panic!("not bond identity");
            }

            let amount = bond
                .bonded_amount
                .checked_sub(bond.slashed_amount)
                .expect("slashed amount exceeds bonded amount");
            bond.bonded_amount = 0;
            bond.active = false;
            e.storage().instance().set(&key, &bond);
            amount
        })
    }

    pub fn slash_bond(e: Env, admin: Address, amount: i128) -> i128 {
        Self::with_reentrancy_guard(&e, || {
            let before = Self::get_identity_state(e.clone()).slashed_amount;
            let after = slashing::slash_bond(&e, &admin, amount).slashed_amount;
            after.checked_sub(before).expect("slashing delta underflow")
        })
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
        let updated = IdentityBond {
            identity: bond.identity.clone(),
            bonded_amount: bond.bonded_amount,
            bond_start: bond.bond_start,
            bond_duration: bond.bond_duration,
            slashed_amount: new_slashed,
            is_rolling: bond.is_rolling,
            notice_period_duration: bond.notice_period_duration,
            withdrawal_requested_at: bond.withdrawal_requested_at,
            active: bond.active,
        };
        e.storage().instance().set(&bond_key, &updated);

        // External call: invoke callback if registered
        let cb_key = Symbol::new(&e, "callback");
        if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
            let fn_name = Symbol::new(&e, "on_slash");
            let args: Vec<Val> = Vec::from_array(&e, [slash_amount.into_val(&e)]);
            e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
        }
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
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("no bond"));

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_add(amount)
            .expect("top-up caused overflow");
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);

        e.storage().instance().set(&key, &bond);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);
        bond
    }

    pub fn extend_duration(e: Env, additional_duration: u64) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e
            .storage()
            .instance()
            .set(&Symbol::new(&e, "callback"), &addr);
    }

    /// Check if the reentrancy lock is currently held.
    pub fn is_locked(e: Env) -> bool {
        Self::check_lock(&e)
    }

    // --- Protocol Parameters (Governance-Controlled) ---

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

    // --- Reentrancy guard helpers ---
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
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_attestation;

#[cfg(test)]
mod test_attestation_types;

#[cfg(test)]
mod test_weighted_attestation;

#[cfg(test)]
mod test_replay_prevention;

#[cfg(test)]
mod test_parameters;
mod test_governance_approval;

#[cfg(test)]
mod test_fees;

#[cfg(test)]
mod integration;

#[cfg(test)]
mod security;

#[cfg(test)]
mod test_early_exit_penalty;

#[cfg(test)]
mod test_rolling_bond;

#[cfg(test)]
mod test_tiered_bond;

#[cfg(test)]
mod test_slashing;

#[cfg(test)]
mod test_withdraw_bond;
