#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec};

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

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    /// Initialize the contract (admin).
    pub fn initialize(e: Env, admin: Address) {
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
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
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), attester);
    }

    /// Remove an attester's authorization (only admin can call).
    pub fn unregister_attester(e: Env, attester: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Attester(attester.clone()));
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), attester);
    }

    /// Check if an address is an authorized attester.
    pub fn is_attester(e: Env, attester: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Attester(attester))
            .unwrap_or(false)
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
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period: 0,
        };
        let key = DataKey::Bond;
        e.storage().instance().set(&key, &bond);
        bond
    }

    /// Return current bond state for an identity (simplified: single bond per contract instance).
    pub fn get_identity_state(e: Env) -> IdentityBond {
        e.storage()
            .instance()
            .get::<_, IdentityBond>(&DataKey::Bond)
            .unwrap_or_else(|| panic!("no bond"))
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

#[cfg(test)]
mod test_attestation;

#[cfg(test)]
mod security;
