use soroban_sdk::contracterror;

/// Canonical dispute status machine.
///
/// Valid transitions:
///   Open      → Voting      (voting period begins — implicit at creation)
///   Voting    → Resolving   (voting period ends, resolve_dispute called)
///   Voting    → Cancelled   (cancel_dispute called by creator or admin)
///   Resolving → Resolved    (outcome tallied and stored, outcome != 0)
///   Resolving → Tied        (votes tallied with tie, outcome = 0 is reserved)
///   Open      → Cancelled   (cancel before voting starts)
///   Resolved  → Archived    (admin archives a finalized dispute)
///   Tied      → Archived    (admin archives a tied dispute)
///   Cancelled → Archived    (admin archives a cancelled dispute)
///   Archived  → Voting      (admin reopens an archived dispute for new voting)
///
/// Tied state indicates the voting outcome was ambiguous (two or more outcomes
/// tied for the highest weight). This is distinct from a definite ruling and must
/// be handled separately by consumers, as outcome = 0 is reserved as invalid.
///
/// All other transitions are rejected with InvalidTransition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[soroban_sdk::contracttype]
pub enum DisputeStatus {
    Open = 0,
    Voting = 1,
    Resolving = 2,
    Resolved = 3,
    Cancelled = 4,
    /// Vote resulted in a tie or no clear winner.
    /// outcome field will be 0 in this state.
    Tied = 5,
    /// Dispute has been finalized and archived by an admin.
    /// Archived disputes can be reopened by an admin via reopen_dispute().
    Archived = 6,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArbitrationError {
    InvalidTransition = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    NotAdmin = 4,
    NotArbitrator = 5,
    AlreadyVoted = 6,
    VotingInactive = 7,
    VotingNotEnded = 8,
    DisputeNotFound = 9,
    InvalidOutcome = 10,
    WeightNotPositive = 11,
    NotAuthorized = 12,
    ReasonTooLong = 14,
    QuorumNotMet = 13,
    /// The actual outcome does not match the promised outcome.
    PromiseNotKept = 15,
    /// Pagination cursor is out of range (cursor >= registry_len).
    /// Triggered by: `get_arbitrators_page` when the supplied cursor
    /// equals or exceeds the current arbitrator count. Accepting
    /// cursor == registry_len would silently return a done=true result,
    /// allowing a caller to synthesize a completed-scan response
    /// without actually scanning any entries.
    CursorOutOfRange = 16,
}

/// Assert a status transition is valid, returning ArbitrationError::InvalidTransition otherwise.
pub fn require_transition(from: DisputeStatus, to: DisputeStatus) -> Result<(), ArbitrationError> {
    let valid = matches!(
        (from, to),
        (DisputeStatus::Open, DisputeStatus::Voting)
            | (DisputeStatus::Open, DisputeStatus::Cancelled)
            | (DisputeStatus::Voting, DisputeStatus::Resolving)
            | (DisputeStatus::Voting, DisputeStatus::Cancelled)
            | (DisputeStatus::Resolving, DisputeStatus::Resolved)
            | (DisputeStatus::Resolving, DisputeStatus::Tied)
            | (DisputeStatus::Resolved, DisputeStatus::Archived)
            | (DisputeStatus::Tied, DisputeStatus::Archived)
            | (DisputeStatus::Cancelled, DisputeStatus::Archived)
            | (DisputeStatus::Archived, DisputeStatus::Voting)
    );
    if valid {
        Ok(())
    } else {
        Err(ArbitrationError::InvalidTransition)
    }
}

/// Check whether a dispute status is considered "active" (operations should be blocked).
pub fn is_dispute_active(status: DisputeStatus) -> bool {
    matches!(
        status,
        DisputeStatus::Open | DisputeStatus::Voting | DisputeStatus::Resolving
    )
}

/// Require that a dispute's status is inactive (Resolved, Cancelled, or Tied).
///
/// Active disputes (Open, Voting, Resolving) block lease-modifying operations;
/// resolved disputes allow them to proceed.
pub fn require_dispute_inactive(status: DisputeStatus) -> Result<(), ArbitrationError> {
    if is_dispute_active(status) {
        Err(ArbitrationError::DisputeActive)
    } else {
        Ok(())
    }
}

/// Assert that a promised outcome matches the actual outcome.
///
/// Returns `Ok(())` when the outcomes match (promise kept),
/// or `Err(ArbitrationError::PromiseNotKept)` when they differ (promise broken).
pub fn require_kept_promise(promised: u32, actual: u32) -> Result<(), ArbitrationError> {
    if promised == actual {
        Ok(())
    } else {
        Err(ArbitrationError::PromiseNotKept)
    }
}

/// Assert that a dispute is in a resolved (terminal) state.
///
/// Active dispute states (`Open`, `Voting`, `Resolving`) indicate the dispute
/// is still ongoing. Terminal states (`Resolved`, `Cancelled`, `Tied`) mean
/// the dispute has concluded and downstream operations (e.g. lease/bond
/// actions) may proceed.
///
/// # Returns
///
/// - `Ok(())` when the dispute is in a terminal state.
/// - `Err(ArbitrationError::DisputeActive)` when the dispute is still active.
pub fn require_dispute_resolved(status: &DisputeStatus) -> Result<(), ArbitrationError> {
    match status {
        DisputeStatus::Resolved | DisputeStatus::Cancelled | DisputeStatus::Tied => Ok(()),
        DisputeStatus::Open | DisputeStatus::Voting | DisputeStatus::Resolving => {
            Err(ArbitrationError::DisputeActive)
        }
    }
}
