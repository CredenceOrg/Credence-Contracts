use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Env, Symbol};

use crate::multisig::DataKey;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PauseAction {
    Pause = 1,
    Unpause = 2,
}

/// Absolute ceiling on `MaxPauseSigners`, independent of what an admin
/// configures. Bounds unmetered instance-storage growth from repeated
/// `set_pause_signer(..., enabled: true)` calls.
pub const MAX_PAUSE_SIGNERS_HARD_CAP: u32 = 1_000;

/// Default cap on the number of registered pause signers when the admin
/// has not configured one explicitly. Generous enough that no realistic
/// existing signer set exceeds it — preserving current behavior for every
/// deployment in practice — while still bounding unmetered growth.
pub const DEFAULT_MAX_PAUSE_SIGNERS: u32 = 100;

/// Read the configured cap on the number of pause signers, or the default
/// if the admin has not configured one.
pub fn get_max_pause_signers(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::MaxPauseSigners)
        .unwrap_or(DEFAULT_MAX_PAUSE_SIGNERS)
}

/// Configure the cap on the number of pause signers that can be
/// registered. Admin-only.
///
/// # Errors
/// `ContractError::InvalidMaxPauseSigners` when `max_signers` is `0` or
/// exceeds [`MAX_PAUSE_SIGNERS_HARD_CAP`].
pub fn set_max_pause_signers(e: &Env, admin: &Address, max_signers: u32) {
    require_admin_auth(e, admin);

    if max_signers == 0 || max_signers > MAX_PAUSE_SIGNERS_HARD_CAP {
        panic_with_error!(e, ContractError::InvalidMaxPauseSigners);
    }

    let old = get_max_pause_signers(e);
    e.storage()
        .instance()
        .set(&DataKey::MaxPauseSigners, &max_signers);

    e.events().publish(
        (Symbol::new(e, "max_pause_signers_set"),),
        (old, max_signers),
    );
}

fn require_admin_auth(e: &Env, admin: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
    if stored_admin != *admin {
        panic_with_error!(e, ContractError::NotAdmin);
    }
    admin.require_auth();
}

pub fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn require_not_paused(e: &Env) {
    if is_paused(e) {
        panic_with_error!(e, ContractError::ContractPaused);
    }
}

pub fn set_pause_signer(e: &Env, admin: &Address, signer: &Address, enabled: bool) {
    require_admin_auth(e, admin);

    let key = DataKey::PauseSigner(signer.clone());
    let existing: bool = e.storage().instance().get(&key).unwrap_or(false);

    if enabled {
        if !existing {
            let count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseSignerCount)
                .unwrap_or(0);
            if count >= get_max_pause_signers(e) {
                panic_with_error!(e, ContractError::MaxPauseSignersExceeded);
            }
            e.storage().instance().set(&key, &true);
            e.storage()
                .instance()
                .set(&DataKey::PauseSignerCount, &count.saturating_add(1));
        }
    } else if existing {
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

    e.events().publish(
        (Symbol::new(e, "pause_signer_set"), signer.clone()),
        enabled,
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
        panic_with_error!(e, ContractError::ThresholdExceedsSigners);
    }
    e.storage()
        .instance()
        .set(&DataKey::PauseThreshold, &threshold);
    e.events()
        .publish((Symbol::new(e, "pause_threshold_set"),), threshold);
}

fn require_pause_signer(e: &Env, signer: &Address) {
    signer.require_auth();
    let ok: bool = e
        .storage()
        .instance()
        .get(&DataKey::PauseSigner(signer.clone()))
        .unwrap_or(false);
    if !ok {
        panic_with_error!(e, ContractError::NotSigner);
    }
}

fn next_proposal_id(e: &Env) -> u64 {
    let id: u64 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposalCounter)
        .unwrap_or(0);
    let next = id
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
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
    let new_count = count
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
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
        do_pause(e, None);
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
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

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
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

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
        panic_with_error!(e, ContractError::InsufficientApprovals);
    }

    match action {
        1 => do_pause(e, Some(proposal_id)),
        2 => do_unpause(e, Some(proposal_id)),
        _ => panic_with_error!(e, ContractError::InvalidPauseAction),
    }

    e.storage()
        .instance()
        .remove(&DataKey::PauseProposal(proposal_id));
}

fn do_pause(e: &Env, proposal_id: Option<u64>) {
    e.storage().instance().set(&DataKey::Paused, &true);
    e.events().publish((Symbol::new(e, "paused"),), proposal_id);
}

fn do_unpause(e: &Env, proposal_id: Option<u64>) {
    e.storage().instance().set(&DataKey::Paused, &false);
    e.events()
        .publish((Symbol::new(e, "unpaused"),), proposal_id);
}
