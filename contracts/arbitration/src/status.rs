use soroban_sdk::contracterror;

/// Canonical dispute status machine.
///
/// Valid transitions:
///   Open      → Voting    (voting period begins — implicit at creation)
///   Voting    → Resolving (voting period ends, resolve_dispute called)
///   Voting    → Cancelled (cancel_dispute called by creator or admin)
///   Resolving → Resolved  (outcome tallied and stored)
///   Open      → Cancelled (cancel before voting starts)
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
    );
    if valid {
        Ok(())
    } else {
        Err(ArbitrationError::InvalidTransition)
    }
}
