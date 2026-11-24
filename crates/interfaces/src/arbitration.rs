//! Cross-contract interface for the Credence arbitration contract.
//!
//! `dispute_resolution` needs to read the final ruling of a `credence_arbitration`
//! dispute so it can accept an arbitration-backed resolution. Depending on the
//! concrete `credence_arbitration` crate would introduce a build edge between two
//! sibling contracts (and risk a dependency cycle), so the client surface lives
//! here instead:
//!
//! - [`ArbitrationStatus`] mirrors the arbitration dispute status machine.
//! - [`ArbitrationDispute`] mirrors the on-chain `Dispute` record field-for-field.
//! - [`ArbitrationClient`] is the generated cross-contract client.
//!
//! Both mirrored types must stay type- and name-compatible with the arbitration
//! contract's XDR encoding; the field names below are part of that contract.
use soroban_sdk::{contractclient, contracttype, Address, Env, String, Symbol};

/// Mirror of the arbitration contract's `DisputeStatus`.
///
/// Unit-only variants are encoded as `u32`, so the numeric discriminants must
/// match the arbitration contract exactly.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArbitrationStatus {
    Open = 0,
    Voting = 1,
    Resolving = 2,
    Resolved = 3,
    Cancelled = 4,
    /// Tie or no clear winner; `outcome` is 0.
    Tied = 5,
    /// Finalized and archived by an admin.
    Archived = 6,
    /// Voting ended without meeting quorum; `outcome` is 0.
    NoQuorum = 7,
}

/// Mirror of the arbitration contract's `Dispute` record.
///
/// Field names and types must match the arbitration contract so the wire
/// encoding round-trips through [`ArbitrationClient`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArbitrationDispute {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub voting_start: u64,
    pub voting_end: u64,
    pub status: ArbitrationStatus,
    /// Winning outcome, or 0 when the ruling is `Tied`/`NoQuorum`.
    pub outcome: u32,
    pub cancellation_reason: Option<String>,
    pub cancelled_by_role: Option<Symbol>,
}

/// Cross-contract client for reading arbitration rulings.
///
/// The success payload of `get_dispute` is returned directly; a missing or
/// invalid arbitration dispute aborts the whole invocation.
#[contractclient(name = "ArbitrationClient")]
pub trait Arbitration {
    /// Read the arbitration dispute record by id.
    fn get_dispute(env: Env, dispute_id: u64) -> ArbitrationDispute;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_discriminants_match_arbitration_spec() {
        assert_eq!(ArbitrationStatus::Open as u32, 0);
        assert_eq!(ArbitrationStatus::Voting as u32, 1);
        assert_eq!(ArbitrationStatus::Resolving as u32, 2);
        assert_eq!(ArbitrationStatus::Resolved as u32, 3);
        assert_eq!(ArbitrationStatus::Cancelled as u32, 4);
        assert_eq!(ArbitrationStatus::Tied as u32, 5);
        assert_eq!(ArbitrationStatus::Archived as u32, 6);
        assert_eq!(ArbitrationStatus::NoQuorum as u32, 7);
    }

    #[test]
    fn arbitration_client_type_is_available() {
        let _client: Option<ArbitrationClient<'_>> = None;
    }
}
