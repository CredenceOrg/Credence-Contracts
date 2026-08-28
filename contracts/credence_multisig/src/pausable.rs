use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Bytes, Env, String, Symbol};

use crate::multisig::DataKey;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PauseAction {
    Pause = 1,
    Unpause = 2,
}

/// Absolute ceiling on the configured pause-signer count.
pub const MAX_PAUSE_SIGNERS_HARD_CAP: u32 = 1_000;

/// Default pause-signer cap when no explicit value is configured.
pub const DEFAULT_MAX_PAUSE_SIGNERS: u32 = 100;

/// Number of ledger sequences per signer pause-proposal epoch bucket.
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;

pub fn get_max_pause_signers(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::MaxPauseSigners)
        .unwrap_or(DEFAULT_MAX_PAUSE_SIGNERS)
}

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

fn derive_proposal_id(e: &Env, action: PauseAction) -> u64 {
    let epoch = e.ledger().sequence() / PROPOSAL_EPOCH_SIZE;
    let action_u32 = action as u32;

    let preimage = Bytes::from_array(
        e,
        &[
            ((action_u32 >> 24) & 0xff) as u8,
            ((action_u32 >> 16) & 0xff) as u8,
            ((action_u32 >> 8) & 0xff) as u8,
            (action_u32 & 0xff) as u8,
            ((epoch >> 24) & 0xff) as u8,
            ((epoch >> 16) & 0xff) as u8,
            ((epoch >> 8) & 0xff) as u8,
            (epoch & 0xff) as u8,
        ],
    );

    let hash = e.crypto().sha256(&preimage);
    let b = hash.to_array();
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

fn require_matching_signer_epoch(e: &Env, ep: u64) -> u32 {
    let action: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseProposal(ep))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));
    let action_kind = match action {
        1 => PauseAction::Pause,
        2 => PauseAction::Unpause,
        _ => panic_with_error!(e, ContractError::InvalidPauseAction),
    };

    let expected_id = derive_proposal_id(e, action_kind);
    if ep != expected_id {
        panic_with_error!(e, ContractError::StaleSignerEpoch);
    }

    action
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

    let id = derive_proposal_id(e, action);
    let proposal_key = DataKey::PauseProposal(id);

    if !e.storage().instance().has(&proposal_key) {
        e.storage().instance().set(&proposal_key, &(action as u32));
        e.storage()
            .instance()
            .set(&DataKey::PauseApprovalCount(id), &0_u32);

        e.events()
            .publish((Symbol::new(e, "pause_proposed"), id), action as u32);
    }

    record_approval(e, id, caller);

    Some(id)
}

pub fn approve_pause_proposal(e: &Env, signer: &Address, proposal_id: u64) {
    require_pause_signer(e, signer);
    require_matching_signer_epoch(e, proposal_id);

    record_approval(e, proposal_id, signer);

    e.events().publish(
        (Symbol::new(e, "pause_approved"), proposal_id),
        signer.clone(),
    );
}

pub fn execute_pause_proposal(e: &Env, proposal_id: u64) {
    let action = require_matching_signer_epoch(e, proposal_id);

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
        1 => do_pause(e, Some(proposal_id), &String::from_str(e, "")),
        2 => do_unpause(e, Some(proposal_id)),
        _ => panic_with_error!(e, ContractError::InvalidPauseAction),
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
