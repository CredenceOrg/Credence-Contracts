use credence_errors::ContractError;
use soroban_sdk::{Address, Env, String, Symbol};

use crate::DataKey;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PauseAction {
    Pause = 1,
    Unpause = 2,
}

fn require_admin_auth(e: &Env, admin: &Address) {
    credence_errors::require_admin!(e, admin, DataKey::Admin);
}

pub fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn require_not_paused(e: &Env) {
    if is_paused(e) {
        e.panic_with_error(ContractError::ContractPaused);
    }
}

pub fn set_pause_signer(e: &Env, admin: &Address, signer: &Address, enabled: bool) {
    require_admin_auth(e, admin);

    let key = DataKey::PauseSigner(signer.clone());
    let old_enabled: bool = e.storage().instance().get(&key).unwrap_or(false);

    if enabled {
        if !old_enabled {
            e.storage().instance().set(&key, &true);
            let count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseSignerCount)
                .unwrap_or(0);
            e.storage()
                .instance()
                .set(&DataKey::PauseSignerCount, &count.saturating_add(1));
        }
    } else if old_enabled {
        e.storage().instance().remove(&key);
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &count.saturating_sub(1));

        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        let new_count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        if threshold > new_count {
            e.storage()
                .instance()
                .set(&DataKey::PauseThreshold, &new_count);
        }
    }

    // Emit old and new values for auditability
    e.events().publish(
        (Symbol::new(e, "pause_signer_set"), signer.clone()),
        (old_enabled, enabled),
    );
}

pub fn set_pause_threshold(e: &Env, admin: &Address, threshold: u32) {
    require_admin_auth(e, admin);
    let count: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseSignerCount)
        .unwrap_or(0);
    if threshold > count {
        panic!("threshold cannot exceed signer count");
    }
    let old_threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    e.storage()
        .instance()
        .set(&DataKey::PauseThreshold, &threshold);

    // Emit old and new values for auditability
    e.events().publish(
        (Symbol::new(e, "pause_threshold_set"),),
        (old_threshold, threshold),
    );
}

fn require_pause_signer(e: &Env, signer: &Address) {
    signer.require_auth();
    let ok: bool = e
        .storage()
        .instance()
        .get(&DataKey::PauseSigner(signer.clone()))
        .unwrap_or(false);
    if !ok {
        panic!("not pause signer");
    }
}

fn next_proposal_id(e: &Env) -> u64 {
    let id: u64 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposalCounter)
        .unwrap_or(0);
    let next = id.checked_add(1).expect("pause proposal counter overflow");
    e.storage()
        .instance()
        .set(&DataKey::PauseProposalCounter, &next);
    id
}

fn record_approval(e: &Env, proposal_id: u64, signer: &Address) {
    let approval_key = DataKey::PauseApproval(proposal_id, signer.clone());
    if e.storage().instance().has(&approval_key) {
        return;
    }
    e.storage().instance().set(&approval_key, &true);
    let count: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);
    let new_count = count.checked_add(1).expect("pause approval count overflow");
    e.storage()
        .instance()
        .set(&DataKey::PauseApprovalCount(proposal_id), &new_count);
}

pub fn pause(e: &Env, caller: &Address) -> Option<u64> {
    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    if threshold == 0 {
        require_admin_auth(e, caller);
        do_pause(e, None, &caller.to_string());
        None
    } else {
        propose_action(e, caller, PauseAction::Pause)
    }
}

pub fn unpause(e: &Env, caller: &Address) -> Option<u64> {
    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    if threshold == 0 {
        require_admin_auth(e, caller);
        do_unpause(e, None);
        None
    } else {
        propose_action(e, caller, PauseAction::Unpause)
    }
}

fn propose_action(e: &Env, caller: &Address, action: PauseAction) -> Option<u64> {
    require_pause_signer(e, caller);

    let id = next_proposal_id(e);
    e.storage()
        .instance()
        .set(&DataKey::PauseProposal(id), &(action as u32));
    e.storage()
        .instance()
        .set(&DataKey::PauseApprovalCount(id), &0_u32);

    record_approval(e, id, caller);

    e.events()
        .publish((Symbol::new(e, "pause_proposed"), id), action as u32);

    Some(id)
}

pub fn approve_pause_proposal(e: &Env, signer: &Address, proposal_id: u64) {
    require_pause_signer(e, signer);

    let _action: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"));

    record_approval(e, proposal_id, signer);

    e.events().publish(
        (Symbol::new(e, "pause_approved"), proposal_id),
        signer.clone(),
    );
}

pub fn execute_pause_proposal(e: &Env, proposal_id: u64) {
    let action: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"));

    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    let approvals: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);

    if approvals < threshold {
        panic!("insufficient approvals to execute");
    }

    match action {
        1 => do_pause(e, Some(proposal_id), &String::from_str(e, "")),
        2 => do_unpause(e, Some(proposal_id)),
        _ => panic!("invalid pause action"),
    }

    e.storage()
        .instance()
        .remove(&DataKey::PauseProposal(proposal_id));
}

fn do_pause(e: &Env, proposal_id: Option<u64>, reason: &String) {
    e.storage().instance().set(&DataKey::Paused, &true);
    e.events()
        .publish((Symbol::new(e, "paused"),), (proposal_id, reason.clone()));
}

fn do_unpause(e: &Env, proposal_id: Option<u64>) {
    e.storage().instance().set(&DataKey::Paused, &false);
    e.events()
        .publish((Symbol::new(e, "unpaused"),), proposal_id);
}

// ============================================================================
// Governance-Controlled Borrow Freeze Helpers
// ============================================================================

/// Returns `true` when new borrows/increases are frozen.
#[must_use]
pub fn is_borrow_frozen(e: &Env) -> bool {
    crate::parameters::is_borrow_frozen(e)
}

/// Panics with `BorrowFrozen` if borrows are currently frozen.
pub fn require_not_borrow_frozen(e: &Env) {
    crate::parameters::require_not_borrow_frozen(e);
}

/// Freeze or unfreeze new bond creation and top-ups. Governance-only.
pub fn set_borrow_frozen(e: &Env, admin: &Address, frozen: bool) {
    crate::parameters::set_borrow_frozen(e, admin, frozen);
}

