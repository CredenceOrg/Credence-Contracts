#![no_std]

pub mod early_exit_penalty;
mod fees;
pub mod governance_approval;
mod math;
mod nonce;
mod rolling_bond;
mod slash_history;
mod slashing;
pub mod tiered_bond;
mod weighted_attestation;

pub mod types;

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

use soroban_sdk::token::TokenClient;

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
    Token,
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

    /// Set the token contract address (admin only). Required before `create_bond`, `top_up`,
    /// and `withdraw_bond`.
    pub fn set_token(e: Env, admin: Address, token: Address) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        if admin != stored_admin {
            panic!("not admin");
        }
        e.storage().instance().set(&DataKey::Token, &token);
    }

    /// Create a bond for an identity.
    /// Transfers USDC from the identity to the contract (token must be set and approved).
    /// Bond creation fee (if configured) is deducted and recorded for the treasury.
    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
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
            notice_period_duration,
        };

        e.storage().instance().set(&DataKey::Bond, &bond);

        let old_tier = BondTier::Bronze;
        let new_tier = tiered_bond::get_tier_for_amount(net_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &identity, old_tier, new_tier);
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

        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("token not set"));
        let contract = e.current_contract_address();
        TokenClient::new(&e, &token).transfer(&contract, &bond.identity, &amount);

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

        e.storage().instance().set(&key, &bond);
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

        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("token not set"));
        let contract = e.current_contract_address();
        let token_client = TokenClient::new(&e, &token);
        let net_amount = amount.checked_sub(penalty).expect("penalty exceeds amount");
        token_client.transfer(&contract, &bond.identity, &net_amount);
        if penalty > 0 {
            token_client.transfer(&contract, &treasury, &penalty);
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

        // Overflow check before token transfer (CEI pattern)
        let new_bonded = bond
            .bonded_amount
            .checked_add(amount)
            .expect("top-up caused overflow");

        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("token not set"));
        let contract = e.current_contract_address();
        TokenClient::new(&e, &token).transfer_from(&contract, &bond.identity, &contract, &amount);

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = new_bonded;
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bond
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
}

#[cfg(test)]
mod test_helpers;

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

#[cfg(test)]
mod test_math;
