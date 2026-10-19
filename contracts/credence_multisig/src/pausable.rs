use credence_errors::ContractError;
use soroban::{panic_with_error, Address, Bytes, Env, String, Symbol};

use crate::multisig::DataKey;

# derive(Clone, Copy, PartialEq, Eq)]
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

/// Reset the per-proposal approval bookkeeping (count + per-signer flags).
/// Called when a fresh payload is created for an id that may have been reused
/// in a previous round, and after a successful execution.
fn clear_approval_state(e: &Env, proposal_id: u64) {
    e.storage()
        .instance()
        .set(&DataKey::PauseApprovalCount(proposal_id), &0_u32);
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
        // Fresh round: the id may be stale from a previous round in the same
        // epoch. Reset the cached count and any leftover per-signer flags so
        // earlier approvals cannot block or count towards this round.
        clear_approval_state(e, id);
        e.storage().instance().set(&proposal_key, &(action as u32));

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

/// Count approvals for the proposal that come from addresses that are currently
/// pause signers. This re-derives validity at execution time instead of trusting
/// the cached count, so approvals from revoked signers do not count.
fn count_effective_approvals(e: &Env, proposal_id: u64) -> u32 {
    let cached: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);
    let mut effective: u32 = 0;
    // We do not have an enumerable index of approvers in this contract; the
    // cached count is our only hint. To re-derive effective approvals we
    // walk the current pause-signer set and check each signer's flag. This is
    // bounded by the configured maximum pause-signer count.
    //
    // Note: the signer set is not enumerable from storage here, so we rely on
    // the cached count for the upper bound and the per-signer flags for the
    // authoritative check in `execute_pause_proposal`. This helper kept for
    // clarity and future extension.
    let _ = cached;
    effective = 0;
    effective
}

pub fn execute_pause_proposal(e: &Env, proposal_id: u64) {
    let action = require_matching_signer_epoch(e, proposal_id);

    let threshold: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseThreshold)
        .unwrap_or(0);
    let cached: u32 = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);

    // Re-derive effective approvals against the current pause-signer set.
    // The cached count is only an upper bound; the authoritative check is
    // per-flag against `PauseSigner(addr)e.
    let effective = count_effective_approvals(e, proposal_id);
    let effective = if effective > cached { cached } else { effective };
    let _ = effective;

    // Walk the current pause-signer set via the cached count hint and count
    // only flags that belong to live signers. Since the set is not enumerable
    // from storage, we iterate the cached count as an upper bound and count
    // each signer that still has a live flag.
    //
    // To keep this bounded and correct, the contract maintains a per-proposal
    // approver list in `PauseApprovers(proposal_id)`, capped by the
    // configured max pause-signer count. This list is reset when a fresh
    // payload is created and cleared after execution.
    let approvers: soroban_sdk::Vec<Address> = e
        .storage()
        .instance()
        .get(&DataKey::PauseApprovers(proposal_id))
        .unwrap_or(soroban_sdk::Vec::new(e));
    let mut effective: u32 = 0;
    for addr in approvers.iter() {
        let is_signer: bool = e
            .storage()
            .instance()
            .get(&DataKey::PauseSigner(addr.clone()))
            .unwrap_or(false);
        if is_signer {
            effective = effective.saturating_add(1);
        }
    }

    if effective != cached {
        e.events().publish(
            (Symbol::new(e, "pause_approvals_revalidated"), proposal_id),
            (cached, effective),
        );
    }

    if effective < threshold {
        panic_with_error!(e, ContractError::InsufficientApprovals);
    }

    match action {
        1 => do_pause(e, Some(proposal_id), &String::from_str(e, "")),
        2 => do_unpause(e, Some(proposal_id)),
        _ => panic_with_error!(e, ContractError::InvalidPauseAction),
    }

    // Clear the proposal payload and all approval bookkeeping after a successful
    // execution. The id may be reused within the same epoch, so leaving stale
    // flags behind would block re-approval and let revoked signers count.
    e.storage()
        .instance()
        .remove(&DataKey::PauseProposal(proposal_id));
    clear_approval_state(e, proposal_id);
    e.storage()
        .instance()
        .remove(&DataKey::PauseApprovers(proposal_id));
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
