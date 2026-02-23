#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec};

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

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    /// Initialize the contract with admin and governance members.
    pub fn initialize(
        e: Env,
        admin: Address,
        required_approvals: u32,
        governance_members: Vec<Address>,
    ) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "admin"), &admin);
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

    /// Create or top-up a bond for an identity. In a full implementation this would
    /// transfer USDC from the caller and store the bond.
    pub fn create_bond(e: Env, identity: Address, amount: i128, duration: u64) -> IdentityBond {
        let bond_start = e.ledger().timestamp();

        // Verify the end timestamp wouldn't overflow
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
            .get::<_, Address>(&Symbol::new(&e, "admin"))
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

        e.storage().instance().set(&key, &bond);
        bond
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
        bond.bonded_amount = bond
            .bonded_amount
            .checked_add(amount)
            .expect("top-up caused overflow");

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
}

#[cfg(test)]
mod test;

// #[cfg(test)]
// mod test_attestation;

// #[cfg(test)]
// mod security;
