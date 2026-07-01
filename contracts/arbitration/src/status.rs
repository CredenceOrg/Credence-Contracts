use soroban_sdk::contracterror;

/// Canonical dispute status machine.
///
/// Valid transitions:
///   Open      → Voting    (voting period begins — implicit at creation)
///   Voting    → Resolving (voting period ends, resolve_dispute called)
///   Voting    → Cancelled (cancel_dispute called by creator or admin)
///   Resolving → Resolved  (outcome tallied and stored, outcome != 0)
///   Resolving → Tied      (votes tallied with tie, outcome = 0 is reserved)
///   Open      → Cancelled (cancel before voting starts)
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
    );
    if valid {
        Ok(())
    } else {
        Err(ArbitrationError::InvalidTransition)
    }
}
