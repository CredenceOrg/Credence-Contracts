use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisputeError {
    DisputeNotFound = 1,
    AlreadyClosed = 2,
    Unauthorized = 3,
    Paused = 4,
    /// The bound arbitration dispute has not reached a final `Resolved` ruling
    /// (still voting/resolving, cancelled, archived, or an inconsistent outcome).
    ArbitrationNotResolved = 4,
    /// The bound arbitration dispute ended in a tie, so there is no winning
    /// outcome to apply.
    ArbitrationTied = 5,
    /// The bound arbitration dispute ended without meeting quorum.
    ArbitrationNoQuorum = 6,
}
