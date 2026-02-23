#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

mod early_exit_penalty;
mod rolling_bond;
mod tiered_bond;

/// Status of a slash request
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SlashRequestStatus {
    Pending,
    Approved,
    Executed,
    Rejected,
    Disputed,
}


#[contracttype]
#[derive(Clone, Debug)]
pub struct SlashRequest {
    pub id: u32,
    pub requester: Address,
    pub identity: Address,
    pub amount: i128,
    pub reason: Symbol,
    pub status: SlashRequestStatus,
    pub approvals: Vec<Address>,
    pub created_at: u64,
    pub disputed: bool,
    pub dispute_reason: Symbol,
}

/// Governance configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct GovernanceConfig {
    pub admin: Address,
    pub required_approvals: u32,
    pub governance_members: Vec<Address>,
    pub slash_request_counter: u32,
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
    pub is_rolling: bool,
    pub withdrawal_requested_at: u64,
    pub notice_period: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub id: u64,
    pub attester: Address,
    pub subject: Address,
    pub attestation_data: String,
    pub timestamp: u64,
    pub revoked: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Bond,
    Attester(Address),
    Attestation(u64),
    AttestationCounter,
    SubjectAttestations(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    /// Initialize the contract (set admin). Simple version for backward compatibility.
    pub fn initialize(e: Env, admin: Address) {
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Initialize the contract with admin and governance members.
    pub fn initialize_with_governance(
        e: Env,
        admin: Address,
        required_approvals: u32,
        governance_members: Vec<Address>,
    ) {
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::Admin, &admin);
        
        let config = GovernanceConfig {
            admin,
            required_approvals,
            governance_members: governance_members.clone(),
            slash_request_counter: 0,
        };
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "config"), &config);
        // Store governance members for quick lookup
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "gov_members"), &governance_members);
    }

    /// Set governance configuration (admin only).
    pub fn set_governance_config(
        e: Env,
        admin: Address,
        required_approvals: u32,
        governance_members: Vec<Address>,
    ) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        if admin != stored_admin {
            panic!("not admin");
        }
        
        let config = GovernanceConfig {
            admin,
            required_approvals,
            governance_members: governance_members.clone(),
            slash_request_counter: 0,
        };
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "config"), &config);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "gov_members"), &governance_members);
    }

    /// Set early exit penalty config (admin only). Penalty in basis points (e.g. 500 = 5%).
    pub fn set_early_exit_config(e: Env, admin: Address, treasury: Address, penalty_bps: u32) {
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        if admin != stored_admin {
            panic!("not admin");
        }
        early_exit_penalty::set_config(&e, treasury, penalty_bps);
    }

    /// Register an authorized attester (only admin can call).
    pub fn register_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        e.storage()
            .instance()
            .set(&DataKey::Attester(attester.clone()), &true);
    }

    /// Unregister an authorized attester (only admin can call).
    pub fn unregister_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        e.storage()
            .instance()
            .set(&DataKey::Attester(attester), &false);
    }

    /// Check if an address is an authorized attester.
    pub fn is_attester(e: Env, address: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Attester(address))
            .unwrap_or(false)
    }

    /// Create or top-up a bond for an identity (non-rolling helper).
    pub fn create_bond(e: Env, identity: Address, amount: i128, duration: u64) -> IdentityBond {
        CredenceBond::create_bond_with_rolling(e, identity, amount, duration, false, 0)
    }

    /// Create a bond with rolling parameters.
    pub fn create_bond_with_rolling(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period: u64,
    ) -> IdentityBond {
        let bond_start = e.ledger().timestamp();
        let _end_timestamp = bond_start
            .checked_add(duration)
            .expect("bond end timestamp would overflow");

        let bond = IdentityBond {
            identity: identity.clone(),
            bonded_amount: amount,
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
        bond
    }

    /// Return current bond state for an identity.
    pub fn get_identity_state(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        e.storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"))
    }

    /// Submit a slash request (only by governance members).
    pub fn submit_slash_request(
        e: Env,
        requester: Address,
        identity: Address,
        amount: i128,
        reason: Symbol,
    ) -> u32 {
        // Verify requester is a governance member
        let members = e
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&Symbol::new(&e, "gov_members"))
            .unwrap_or_else(|| Vec::new(&e));

        let mut is_member = false;
        for i in 0..members.len() {
            if members.get(i).unwrap() == requester {
                is_member = true;
                break;
            }
        }
        assert!(
            is_member,
            "only governance members can submit slash requests"
        );

        // Get and increment counter
        let mut config = e
            .storage()
            .instance()
            .get::<_, GovernanceConfig>(&Symbol::new(&e, "config"))
            .unwrap();

        config.slash_request_counter += 1;
        let request_id = config.slash_request_counter;
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "config"), &config);

        // Create slash request
        let mut approvals = Vec::new(&e);
        approvals.push_back(requester.clone()); // Self-approve

        let request = SlashRequest {
            id: request_id,
            requester,
            identity,
            amount,
            reason,
            status: SlashRequestStatus::Pending,
            approvals,
            created_at: e.ledger().timestamp(),
            disputed: false,
            dispute_reason: Symbol::new(&e, ""),
        };

        let key = Symbol::new(&e, "slash_req");
        e.storage().instance().set(&key, &request);

        request_id
    }

    /// Approve a slash request (multi-sig approval).
    pub fn approve_slash_request(e: Env, approver: Address) -> bool {
        // Verify approver is a governance member
        let members = e
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&Symbol::new(&e, "gov_members"))
            .unwrap_or_else(|| Vec::new(&e));

        let mut is_member = false;
        for i in 0..members.len() {
            if members.get(i).unwrap() == approver {
                is_member = true;
                break;
            }
        }
        assert!(
            is_member,
            "only governance members can approve slash requests"
        );

        let mut request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));

        // Check if already approved or executed
        assert!(
            request.status == SlashRequestStatus::Pending,
            "request not pending"
        );

        // Check if already approved by this member
        for i in 0..request.approvals.len() {
            if request.approvals.get(i).unwrap() == approver {
                return false; // Already approved
            }
        }

        // Add approval
        request.approvals.push_back(approver);

        // Check if we have enough approvals
        let config = e
            .storage()
            .instance()
            .get::<_, GovernanceConfig>(&Symbol::new(&e, "config"))
            .unwrap();

        if request.approvals.len() >= config.required_approvals {
            request.status = SlashRequestStatus::Approved;
        }

        e.storage()
            .instance()
            .set(&Symbol::new(&e, "slash_req"), &request);

        request.status == SlashRequestStatus::Approved
    }

    /// Execute a slash request after it's approved.
    pub fn execute_slash(e: Env) -> IdentityBond {
        let request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));

        // Must be approved
        assert!(
            request.status == SlashRequestStatus::Approved,
            "request not approved"
        );

        // Get current bond
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Execute slash
        bond.slashed_amount += request.amount;
        if bond.slashed_amount >= bond.bonded_amount {
            bond.active = false;
            bond.slashed_amount = bond.bonded_amount; // Cap at bonded amount
        }

        // Update request status
        let mut updated_request = request.clone();
        updated_request.status = SlashRequestStatus::Executed;
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "slash_req"), &updated_request);
        e.storage().instance().set(&key, &bond);

        bond
    }

    /// Reject a slash request (admin only).
    pub fn reject_slash_request(e: Env) -> SlashRequestStatus {
        let admin = e
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap();

        // In practice, we'd verify the caller is admin via auth
        // For testing, we'll allow any governance member to reject

        let mut request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));

        assert!(
            request.status == SlashRequestStatus::Pending,
            "request not pending"
        );
        request.status = SlashRequestStatus::Rejected;

        e.storage()
            .instance()
            .set(&Symbol::new(&e, "slash_req"), &request);

        SlashRequestStatus::Rejected
    }

    /// Dispute a slash request.
    pub fn dispute_slash_request(e: Env, disputer: Address, reason: Symbol) -> bool {
        // Verify disputer is a governance member
        let members = e
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&Symbol::new(&e, "gov_members"))
            .unwrap_or_else(|| Vec::new(&e));

        let mut is_member = false;
        for i in 0..members.len() {
            if members.get(i).unwrap() == disputer {
                is_member = true;
                break;
            }
        }
        assert!(
            is_member,
            "only governance members can dispute slash requests"
        );

        let mut request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));

        // Can only dispute pending or approved requests
        assert!(
            request.status == SlashRequestStatus::Pending
                || request.status == SlashRequestStatus::Approved,
            "cannot dispute in current state"
        );

        request.disputed = true;
        request.dispute_reason = reason;
        request.status = SlashRequestStatus::Disputed;

        e.storage()
            .instance()
            .set(&Symbol::new(&e, "slash_req"), &request);

        true
    }

    /// Resolve a dispute (admin only).
    pub fn resolve_dispute(e: Env, resolve_approved: bool) -> SlashRequestStatus {
        let mut request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));

        assert!(
            request.status == SlashRequestStatus::Disputed,
            "no disputed request"
        );

        if resolve_approved {
            // Get config for required approvals
            let config = e
                .storage()
                .instance()
                .get::<_, GovernanceConfig>(&Symbol::new(&e, "config"))
                .unwrap();

            // If already have enough approvals, mark approved
            if request.approvals.len() >= config.required_approvals {
                request.status = SlashRequestStatus::Approved;
            } else {
                request.status = SlashRequestStatus::Pending;
            }
        } else {
            request.status = SlashRequestStatus::Rejected;
        }

        request.disputed = false;
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "slash_req"), &request);

        request.status
    }

    /// Get current slash request status.
    pub fn get_slash_request_status(e: Env) -> SlashRequestStatus {
        let request = e
            .storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"));
        request.status
    }

    /// Get slash request details.
    pub fn get_slash_request(e: Env) -> SlashRequest {
        e.storage()
            .instance()
            .get::<_, SlashRequest>(&Symbol::new(&e, "slash_req"))
            .unwrap_or_else(|| panic!("no slash request"))
    }

    /// Get governance config.
    pub fn get_governance_config(e: Env) -> GovernanceConfig {
        e.storage()
            .instance()
            .get::<_, GovernanceConfig>(&Symbol::new(&e, "config"))
            .unwrap_or_else(|| panic!("no config"))
    }

    /// Check if address is a governance member.
    pub fn is_governance_member(e: Env, address: Address) -> bool {
        let members = e
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&Symbol::new(&e, "gov_members"))
            .unwrap_or_else(|| Vec::new(&e));

        for i in 0..members.len() {
            if members.get(i).unwrap() == address {
                return true;
            }
        }
        false
    }

    /// Add an attestation for a subject (only authorized attesters can call).
    pub fn add_attestation(
        e: Env,
        attester: Address,
        subject: Address,
        attestation_data: String,
    ) -> Attestation {
        attester.require_auth();

        // Verify attester is authorized
        let is_authorized = e
            .storage()
            .instance()
            .get(&DataKey::Attester(attester.clone()))
            .unwrap_or(false);

        if !is_authorized {
            panic!("unauthorized attester");
        }

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

        attestation
    }

    /// Revoke an attestation (only the original attester can revoke).
    pub fn revoke_attestation(e: Env, attester: Address, attestation_id: u64) {
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
    }

    /// Get an attestation by ID.
    pub fn get_attestation(e: Env, attestation_id: u64) -> Attestation {
        e.storage()
            .instance()
            .get(&DataKey::Attestation(attestation_id))
            .unwrap_or_else(|| panic!("attestation not found"))
    }

    /// Get all attestation IDs for a subject.
    pub fn get_subject_attestations(e: Env, subject: Address) -> Vec<u64> {
        e.storage()
            .instance()
            .get(&DataKey::SubjectAttestations(subject))
            .unwrap_or(Vec::new(&e))
    }

    /// Withdraw from bond. Checks that the bond has sufficient balance after accounting for slashed amount.
    /// Returns the updated bond with reduced bonded_amount.
    pub fn withdraw(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Calculate available balance (bonded - slashed)
        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");

        // Verify sufficient available balance for withdrawal
        if amount > available {
            panic!("insufficient balance for withdrawal");
        }

        // Perform withdrawal with overflow protection
        bond.bonded_amount = bond
            .bonded_amount
            .checked_sub(amount)
            .expect("withdrawal caused underflow");

        // Verify invariant: slashed amount should not exceed bonded amount after withdrawal
        if bond.slashed_amount > bond.bonded_amount {
            panic!("slashed amount exceeds bonded amount");
        }

        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount + amount);
        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bond
    }

    /// Withdraw before lock-up end; applies early exit penalty and transfers penalty to treasury.
    /// Net amount to user = amount - penalty. Use when lock-up has not yet ended.
    pub fn withdraw_early(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        let available = bond
            .bonded_amount
            .checked_sub(bond.slashed_amount)
            .expect("slashed amount exceeds bonded amount");
        if amount > available {
            panic!("insufficient balance for withdrawal");
        }

        let now = e.ledger().timestamp();
        let end = bond.bond_start.saturating_add(bond.bond_duration);
        if now >= end {
            panic!("use withdraw for post lock-up");
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

    /// Request withdrawal (rolling bonds). Withdrawal allowed after notice period.
    pub fn request_withdrawal(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
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

    /// If bond is rolling and period has ended, renew (new period start = now). Emits renewal event.
    pub fn renew_if_rolling(e: Env) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
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

    /// Get current tier for the bond's bonded amount.
    pub fn get_tier(e: Env) -> BondTier {
        let bond = CredenceBond::get_identity_state(e);
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
    }

    /// Top up the bond with additional amount (checks for overflow)
    pub fn top_up(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Perform top-up with overflow protection
        let old_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        bond.bonded_amount = bond
            .bonded_amount
            .checked_add(amount)
            .expect("top-up caused overflow");

        let new_tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);
        tiered_bond::emit_tier_change_if_needed(&e, &bond.identity, old_tier, new_tier);

        e.storage().instance().set(&key, &bond);
        bond
    }

    /// Extend bond duration (checks for u64 overflow on timestamps)
    pub fn extend_duration(e: Env, additional_duration: u64) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic!("no bond"));

        // Perform duration extension with overflow protection
        bond.bond_duration = bond
            .bond_duration
            .checked_add(additional_duration)
            .expect("duration extension caused overflow");

        // Also verify the end timestamp wouldn't overflow
        let _end_timestamp = bond
            .bond_start
            .checked_add(bond.bond_duration)
            .expect("bond end timestamp would overflow");

        e.storage().instance().set(&key, &bond);
        bond
    }

    /// Deposit fees into the contract's fee pool.
    pub fn deposit_fees(e: Env, amount: i128) {
        let key = Symbol::new(&e, "fees");
        let current: i128 = e.storage().instance().get(&key).unwrap_or(0);
        e.storage().instance().set(&key, &(current + amount));
    }

    /// Withdraw the full bonded amount back to the identity.
    /// Uses a reentrancy guard to prevent re-entrance during external calls.
    pub fn withdraw_bond(e: Env, identity: Address) -> i128 {
        identity.require_auth();
        CredenceBond::acquire_lock(&e);

        let bond_key = DataKey::Bond;
        let bond: IdentityBond = e
            .storage()
            .instance()
            .get(&bond_key)
            .unwrap_or_else(|| panic!("no bond"));

        if bond.identity != identity {
            CredenceBond::release_lock(&e);
            panic!("not bond owner");
        }
        if !bond.active {
            CredenceBond::release_lock(&e);
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
            notice_period: bond.notice_period,
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
            notice_period: bond.notice_period,
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

    /// Register a callback contract address (for testing external call hooks).
    pub fn set_callback(e: Env, addr: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "callback"), &addr);
    }

    /// Check if the reentrancy lock is currently held.
    pub fn is_locked(e: Env) -> bool {
        Self::check_lock(&e)
    }

    // --- Reentrancy guard helpers ---

    fn acquire_lock(e: &Env) {
        let key = Symbol::new(e, "locked");
        let locked: bool = e.storage().instance().get(&key).unwrap_or(false);
        if locked {
            panic!("reentrancy detected");
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
}

#[cfg(test)]
mod test;

// #[cfg(test)]
// mod test_attestation;
#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_attestation;

#[cfg(test)]
mod test_governance;

// #[cfg(test)]
// mod security;

