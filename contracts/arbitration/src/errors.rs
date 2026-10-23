use soroban_sdk::contracterror;

/// Error type for the arbitration contract.
///
/// Discriminants are explicit and stable. New variants must be appended
/// with the next available discriminant; existing values must never be
/// renumbered.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq::PartialEq, PartialOrd::PartialOrd)]
#[soroban_sdk.contracterror]
pub enum ArbitrationError {
    /// The contract has not been initialized.
    NotInitialized = 0,
    /// The contract has already been initialized.
    AlreadyInitialized = 1,
    /// The caller is not the admin.
    NotAdmin = 2,
    /// The caller is not a registered arbitrator.
    NotArbitrator = 3,
    /// The caller is not authorized for this action.
    NotAuthorized = 4,
    /// The requested status transition is not allowed.
    InvalidTransition = 5,
    /// The dispute id does not exist.
    DisputeNotFound = 6,
    /// The voting window has closed.
    VotingClosed = 7,
    /// The voting window has not yet ended.
    VotingNotEnded = 8,
    /// The voter has already cast a vote on this dispute.
    AlreadyVoted = 9,
    /// The creator already has an ongoing dispute.
    OngoingDispute = 10,
    /// The supplied outcome is invalid.
    InvalidOutcome = 11,
    /// The supplied arbitrator weight is invalid.
    InvalidWeight = 12,
    /// The arbitrator is already registered.
    AlreadyRegistered = 13,
    /// The arbitrator is not registered.
    NotRegistered = 14,
    /// The supplied duration is invalid.
    InvalidDuration = 15,
    /// No votes were cast before resolution.
    NoVotes = 16,
    /// The dispute has already been resolved.
    AlreadyResolved = 17,
    /// The dispute is in an invalid status for this operation.
    InvalidStatus = 18,
    /// The creator may only cancel before any vote is cast and before
    /// the voting deadline.
    CancelNotAllowed = 19,
    /// The voter is the creator of the dispute (conflict of interest).
    ConflictOfInterest = 20,
}
