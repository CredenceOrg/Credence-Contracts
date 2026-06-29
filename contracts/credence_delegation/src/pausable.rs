use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Bytes, Env, Symbol, Vec};

use crate::DataKey;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PauseAction {
    Pause = 1,
    Unpause = 2,
}

/// Read-only aggregated snapshot of a single pause proposal, for operator
/// monitoring dashboards.
///
/// This struct is the typed result of [`get_pause_proposal_state`], which
/// **aggregates four distinct storage entries** into one read:
/// * [`DataKey::PauseProposalCounter`] — retained for backward compatibility
///   with clients that stored counter-based IDs before the hash migration.
/// * [`DataKey::PauseProposal`] — the proposed action payload.
/// * [`DataKey::PauseApproval`] — the per-signer approval flags.
/// * [`DataKey::PauseApprovalCount`] — the running approval tally.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PauseProposalView {
    /// The proposal id this view describes (echoes the query argument).
    pub proposal_id: u64,
    /// Proposed action: `1` = Pause, `2` = Unpause, `0` = no live payload
    /// (the proposal was never allocated, or has already been executed/cleared).
    pub action: u32,
    /// Number of distinct signer approvals recorded for the proposal.
    pub approvals: u32,
    /// The subset of the caller-supplied `signers` that have approved. See
    /// [`get_pause_proposal_state`] for why the candidate set must be supplied.
    pub approved_by: Vec<Address>,
    /// `true` when the proposal was executed (payload cleared).
    /// For hash-derived IDs this is detected via surviving approval keys in the
    /// supplied `signers` set; for legacy counter-based IDs it is detected via
    /// the counter. See [`get_pause_proposal_state`] for details.
    pub executed: bool,
}

/// Number of ledger sequences that form one epoch bucket for proposal-ID
/// derivation.
///
/// **Tradeoff:** a wider bucket (larger N) gives operators a longer window in
/// which concurrent submissions of the same action converge to one ID, but also
/// means a new `(action, target_ledger)` pair cannot be re-proposed until the
/// next epoch begins — even after the previous proposal is executed.  A narrow
/// bucket (smaller N) reduces the convergence window and increases the chance
/// that two operators straddle an epoch boundary.
///
/// 100 ledgers ≈ 8 minutes on Stellar (≈ 5-second close times), which is
/// comfortably wider than any realistic multisig signing round-trip.
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;

/// Derive a stable, deterministic proposal ID from an action and the current
/// epoch.
///
/// # Derivation rule
///
/// ```text
/// epoch   = ledger_sequence / PROPOSAL_EPOCH_SIZE
/// preimage = action_u32_big_endian ++ epoch_u32_big_endian   (8 bytes)
/// hash    = SHA-256(preimage)                                  (32 bytes)
/// id      = first 8 bytes of hash interpreted as big-endian u64
/// ```
///
/// # Idempotency guarantee
///
/// Any number of operators calling `propose_action` with the **same** `action`
/// during the **same** epoch produce the **same** `proposal_id`.  Votes
/// therefore accumulate on a single shared proposal regardless of submission
/// order or count.
///
/// # Limits
///
/// The guarantee does *not* extend across epoch boundaries: the same action
/// submitted in epoch *k* and epoch *k+1* yields two different IDs and two
/// independent proposals.  This is intentional — it allows re-proposing the
/// same action after a previous epoch's proposal was abandoned without reaching
/// quorum.
///
/// The helper is **pure**: identical inputs always produce identical output;
/// it reads the ledger sequence from `env` but writes nothing to storage.
fn derive_proposal_id(e: &Env, action: PauseAction) -> u64 {
    let epoch = e.ledger().sequence() / PROPOSAL_EPOCH_SIZE;
    let action_u32 = action as u32;

    // Build an 8-byte preimage: 4 bytes action || 4 bytes epoch (big-endian).
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

/// Add or remove a pause signer.
///
/// Invariant: the stored `PauseSignerCount` MUST always equal the number
/// of `PauseSigner(Address)` entries set to `true` in contract storage.
///
/// Implementations must ensure `PauseSignerCount` is only incremented when
/// a previously-false entry is set to `true`, and only decremented when a
/// previously-true entry is removed. Tests should assert this invariant after
/// every `set_pause_signer` call.
pub fn set_pause_signer(e: &Env, admin: &Address, signer: &Address, enabled: bool) {
    require_admin_auth(e, admin);

    // No-lockout invariants:
    // 1. If there are active signers (count > 0), threshold MUST be > 0.
    // 2. Threshold MUST be <= signer count.
    // 3. Unpause MUST ALWAYS be reachable (admin override is available).

    let key = DataKey::PauseSigner(signer.clone());
    let existing: bool = e.storage().instance().get(&key).unwrap_or(false);

    if enabled {
        if !existing {
            e.storage().instance().set(&key, &true);
            let count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseSignerCount)
                .unwrap_or(0);
            e.storage()
                .instance()
                .set(&DataKey::PauseSignerCount, &count.saturating_add(1));

            // Auto-adjust threshold to 1 if it is currently 0, to maintain no-lockout invariant
            let threshold: u32 = e
                .storage()
                .instance()
                .get(&DataKey::PauseThreshold)
                .unwrap_or(0);
            if threshold == 0 {
                e.storage().instance().set(&DataKey::PauseThreshold, &1_u32);
            }
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
    if threshold == 0 && count > 0 {
        panic_with_error!(e, ContractError::InvalidPauseAction);
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
        // Admin override: Admin can always unpause without a proposal.
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if *caller == stored_admin {
            caller.require_auth();
            do_unpause(e, None);
            return None;
        }

        propose_action(e, caller, PauseAction::Unpause)
    }
}

/// Submit a pause or unpause proposal.
///
/// The proposal ID is derived deterministically from `(action, epoch)` rather
/// than from a counter. If a proposal with the same ID already exists in
/// storage the submission is **idempotent**: the existing proposal is not
/// overwritten, no error is returned, and the submitter's approval is recorded
/// on the shared proposal. Votes therefore accumulate correctly regardless of
/// how many operators submit the same action in the same epoch.
fn propose_action(e: &Env, caller: &Address, action: PauseAction) -> Option<u64> {
    require_pause_signer(e, caller);

    let id = derive_proposal_id(e, action);
    let proposal_key = DataKey::PauseProposal(id);

    // Idempotent: only write the proposal record if it does not already exist.
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
    e.storage()
        .instance()
        .remove(&DataKey::PauseApprovalCount(proposal_id));
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

/// Fetch a proposal that was recorded under a legacy counter-based ID.
///
/// **Compatibility shim**: before the hash-derivation migration, proposal IDs
/// were issued by a monotone counter (`PauseProposalCounter`). Clients that
/// persisted those numeric IDs can use this function to resolve them.  If the
/// proposal is not found under the given ID the function returns
/// `Err(ContractError::ProposalNotFound)` rather than panicking, so callers
/// can distinguish a stale counter-ID from a network error.
///
/// This function is intentionally **not** the primary resolution path for
/// new proposals; use [`get_pause_proposal_state`] for those.
pub fn get_proposal_by_legacy_id(e: &Env, legacy_id: u64) -> Result<u32, ContractError> {
    e.storage()
        .instance()
        .get(&DataKey::PauseProposal(legacy_id))
        .ok_or(ContractError::ProposalNotFound)
}

/// Aggregate the full state of a pause proposal into a single typed view.
///
/// This is **read-only**: it performs no `require_auth` and never mutates
/// storage, so it is safe to expose as a public entrypoint. It combines the
/// four proposal-related storage entries (see [`PauseProposalView`]).
///
/// `signers` is the candidate set used to populate `approved_by`. Soroban
/// instance storage is a key/value map with no key enumeration, and the
/// contract keeps no list of approvers — only per-`(proposal, signer)` flags.
/// The view therefore cannot discover approvers on its own; the caller passes
/// the addresses it wants resolved (operators already track their signer set).
/// Passing an empty vector yields an empty `approved_by` while still returning
/// the action/approvals/executed fields, which do not depend on `signers`.
///
/// Field derivation:
/// * `action` is `0` when no live payload exists for `proposal_id`.
/// * `executed` is `true` when the proposal's payload was cleared by execution.
///   Two detection paths are used: (a) legacy counter path — `proposal_id <
///   counter && !has_payload`; (b) hash-derived path — `!has_payload &&
///   approved_by.len() > 0` (per-signer approval keys survive execution while
///   the payload and approval count are removed). Supplying a non-empty
///   `signers` set that includes at least one approver ensures the hash-derived
///   path fires correctly.
pub fn get_pause_proposal_state(
    e: &Env,
    proposal_id: u64,
    signers: &Vec<Address>,
) -> PauseProposalView {
    let store = e.storage().instance();

    // Read 1: the legacy counter, for backward-compatible `executed` detection.
    let counter: u64 = store.get(&DataKey::PauseProposalCounter).unwrap_or(0);

    // Read 2: the action payload. Absent (0) once executed or if never created.
    let action: u32 = store.get(&DataKey::PauseProposal(proposal_id)).unwrap_or(0);
    let has_payload = action != 0;

    // Read 3: the approval count.
    let approvals: u32 = store
        .get(&DataKey::PauseApprovalCount(proposal_id))
        .unwrap_or(0);

    // Read 4: per-signer approval flags, resolved across the supplied set.
    let mut approved_by = Vec::new(e);
    for signer in signers.iter() {
        let approved: bool = store
            .get(&DataKey::PauseApproval(proposal_id, signer.clone()))
            .unwrap_or(false);
        if approved {
            approved_by.push_back(signer);
        }
    }

    // `executed` is true when a proposal was run to completion and its payload
    // was cleared by `execute_pause_proposal`.
    //
    // For legacy counter-based IDs: the counter was incremented at proposal
    // time, so `proposal_id < counter && !has_payload` is sufficient.
    //
    // For hash-derived IDs: the counter is never incremented, so the counter
    // check cannot detect executed state. Instead we use the surviving
    // per-signer approval keys (which `execute_pause_proposal` does not remove)
    // as the signal: if the payload is absent yet at least one signer in the
    // supplied candidate set has an approval flag, the proposal was executed.
    let executed = if !has_payload {
        // Legacy path: counter covers the ID.
        let legacy_executed = proposal_id < counter;
        // Hash-derived path: surviving approval key(s) in the supplied set.
        let hash_executed = !approved_by.is_empty();
        legacy_executed || hash_executed
    } else {
        false
    };

    PauseProposalView {
        proposal_id,
        action,
        approvals,
        approved_by,
        executed,
    }
}
