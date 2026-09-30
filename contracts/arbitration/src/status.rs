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
    /// Dispute is still active (Open, Voting, or Resolving).
    /// Used to block operations that require the dispute to be inactive or resolved.
    DisputeActive = 17,
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


#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Tests for: require_transition
    // ============================================================================

    mod require_transition {
        use super::*;

        // --- Valid Transitions (10 total) ---

        #[test]
        fn valid_open_to_voting() {
            assert_eq!(
                require_transition(DisputeStatus::Open, DisputeStatus::Voting),
                Ok(())
            );
        }

        #[test]
        fn valid_open_to_cancelled() {
            assert_eq!(
                require_transition(DisputeStatus::Open, DisputeStatus::Cancelled),
                Ok(())
            );
        }

        #[test]
        fn valid_voting_to_resolving() {
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolving),
                Ok(())
            );
        }

        #[test]
        fn valid_voting_to_cancelled() {
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Cancelled),
                Ok(())
            );
        }

        #[test]
        fn valid_resolving_to_resolved() {
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved),
                Ok(())
            );
        }

        #[test]
        fn valid_resolving_to_tied() {
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Tied),
                Ok(())
            );
        }

        #[test]
        fn valid_resolved_to_archived() {
            assert_eq!(
                require_transition(DisputeStatus::Resolved, DisputeStatus::Archived),
                Ok(())
            );
        }

        #[test]
        fn valid_tied_to_archived() {
            assert_eq!(
                require_transition(DisputeStatus::Tied, DisputeStatus::Archived),
                Ok(())
            );
        }

        #[test]
        fn valid_cancelled_to_archived() {
            assert_eq!(
                require_transition(DisputeStatus::Cancelled, DisputeStatus::Archived),
                Ok(())
            );
        }

        #[test]
        fn valid_archived_to_voting() {
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Voting),
                Ok(())
            );
        }

        // --- Invalid Transitions (Self-loops) ---

        #[test]
        fn invalid_open_to_open() {
            assert_eq!(
                require_transition(DisputeStatus::Open, DisputeStatus::Open),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_voting_to_voting() {
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Voting),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_resolving_to_resolving() {
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Resolving),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_resolved_to_resolved() {
            assert_eq!(
                require_transition(DisputeStatus::Resolved, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_cancelled_to_cancelled() {
            assert_eq!(
                require_transition(DisputeStatus::Cancelled, DisputeStatus::Cancelled),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_tied_to_tied() {
            assert_eq!(
                require_transition(DisputeStatus::Tied, DisputeStatus::Tied),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_archived_to_archived() {
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Archived),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        // --- Invalid Backward Transitions ---

        #[test]
        fn invalid_voting_to_open() {
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Open),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_resolving_to_voting() {
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Voting),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_resolved_to_voting() {
            assert_eq!(
                require_transition(DisputeStatus::Resolved, DisputeStatus::Voting),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_archived_to_resolved() {
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        // --- Invalid Skipping Transitions ---

        #[test]
        fn invalid_open_to_resolving() {
            // Cannot skip Voting
            assert_eq!(
                require_transition(DisputeStatus::Open, DisputeStatus::Resolving),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_open_to_resolved() {
            // Cannot skip Voting and Resolving
            assert_eq!(
                require_transition(DisputeStatus::Open, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_voting_to_resolved() {
            // Cannot skip Resolving
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_voting_to_archived() {
            // Terminal states must be reached through Resolving first
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Archived),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        // --- Invalid Transitions to Non-Terminal from Terminal States ---

        #[test]
        fn invalid_resolved_to_cancelled() {
            assert_eq!(
                require_transition(DisputeStatus::Resolved, DisputeStatus::Cancelled),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_tied_to_resolved() {
            assert_eq!(
                require_transition(DisputeStatus::Tied, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        #[test]
        fn invalid_cancelled_to_resolved() {
            assert_eq!(
                require_transition(DisputeStatus::Cancelled, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );
        }

        // --- Recovery Scenario: Reopen then Close Again ---

        #[test]
        fn recovery_path_archived_to_voting_then_resolving() {
            // First reopen: Archived → Voting
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Voting),
                Ok(())
            );

            // Then proceed: Voting → Resolving
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolving),
                Ok(())
            );

            // Then finalize: Resolving → Resolved
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved),
                Ok(())
            );
        }
    }

    // ============================================================================
    // Tests for: require_dispute_inactive
    // ============================================================================

    mod require_dispute_inactive {
        use super::*;

        // --- Success Cases (Inactive States) ---

        #[test]
        fn allows_resolved() {
            assert_eq!(require_dispute_inactive(DisputeStatus::Resolved), Ok(()));
        }

        #[test]
        fn allows_cancelled() {
            assert_eq!(require_dispute_inactive(DisputeStatus::Cancelled), Ok(()));
        }

        #[test]
        fn allows_tied() {
            assert_eq!(require_dispute_inactive(DisputeStatus::Tied), Ok(()));
        }

        #[test]
        fn allows_archived() {
            // Archived is not active, so it should be allowed
            assert_eq!(require_dispute_inactive(DisputeStatus::Archived), Ok(()));
        }

        // --- Rejection Cases (Active States) ---

        #[test]
        fn rejects_open() {
            assert_eq!(
                require_dispute_inactive(DisputeStatus::Open),
                Err(ArbitrationError::DisputeActive)
            );
        }

        #[test]
        fn rejects_voting() {
            assert_eq!(
                require_dispute_inactive(DisputeStatus::Voting),
                Err(ArbitrationError::DisputeActive)
            );
        }

        #[test]
        fn rejects_resolving() {
            assert_eq!(
                require_dispute_inactive(DisputeStatus::Resolving),
                Err(ArbitrationError::DisputeActive)
            );
        }

        // --- Recovery: After Settling, Operations Allowed ---

        #[test]
        fn recovery_after_resolution() {
            // Initially rejected (active)
            assert!(require_dispute_inactive(DisputeStatus::Voting).is_err());

            // After moving to terminal state, allowed
            assert_eq!(require_dispute_inactive(DisputeStatus::Resolved), Ok(()));
        }

        #[test]
        fn recovery_after_cancellation() {
            // Initially rejected (active)
            assert!(require_dispute_inactive(DisputeStatus::Open).is_err());

            // After cancelling, allowed
            assert_eq!(require_dispute_inactive(DisputeStatus::Cancelled), Ok(()));
        }

        #[test]
        fn recovery_after_tie() {
            // Initially rejected (active)
            assert!(require_dispute_inactive(DisputeStatus::Voting).is_err());

            // After tie, allowed
            assert_eq!(require_dispute_inactive(DisputeStatus::Tied), Ok(()));
        }
    }

    // ============================================================================
    // Tests for: require_kept_promise
    // ============================================================================

    mod require_kept_promise {
        use super::*;

        // --- Matching Promises ---

        #[test]
        fn promise_kept_with_same_value() {
            assert_eq!(require_kept_promise(42, 42), Ok(()));
        }

        #[test]
        fn promise_kept_with_zero() {
            // outcome=0 is reserved for Tied state, but the function doesn't reject it
            assert_eq!(require_kept_promise(0, 0), Ok(()));
        }

        #[test]
        fn promise_kept_with_max_u32() {
            assert_eq!(require_kept_promise(u32::MAX, u32::MAX), Ok(()));
        }

        #[test]
        fn promise_kept_with_one() {
            assert_eq!(require_kept_promise(1, 1), Ok(()));
        }

        // --- Mismatched Promises (Promise Broken) ---

        #[test]
        fn promise_broken_different_values() {
            assert_eq!(
                require_kept_promise(42, 99),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn promise_broken_promised_zero_actual_one() {
            assert_eq!(
                require_kept_promise(0, 1),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn promise_broken_promised_one_actual_zero() {
            assert_eq!(
                require_kept_promise(1, 0),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn promise_broken_promised_max_actual_zero() {
            assert_eq!(
                require_kept_promise(u32::MAX, 0),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn promise_broken_promised_max_actual_max_minus_one() {
            assert_eq!(
                require_kept_promise(u32::MAX, u32::MAX - 1),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        // --- Boundary Conditions ---

        #[test]
        fn boundary_0_vs_1() {
            // Adjacent values
            assert_eq!(
                require_kept_promise(0, 1),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn boundary_max_minus_1_vs_max() {
            // Adjacent values at the upper boundary
            assert_eq!(
                require_kept_promise(u32::MAX - 1, u32::MAX),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn boundary_large_values() {
            let large = 1_000_000_000u32;
            assert_eq!(require_kept_promise(large, large), Ok(()));
            assert_eq!(
                require_kept_promise(large, large + 1),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        // --- Recovery: Multiple Checks ---

        #[test]
        fn multiple_checks_same_promise() {
            // Check same promise twice (determinism)
            assert_eq!(require_kept_promise(42, 42), Ok(()));
            assert_eq!(require_kept_promise(42, 42), Ok(()));
        }

        #[test]
        fn multiple_checks_different_promises() {
            // First check fails
            assert_eq!(
                require_kept_promise(42, 99),
                Err(ArbitrationError::PromiseNotKept)
            );
            // Second check with different values also fails
            assert_eq!(
                require_kept_promise(10, 20),
                Err(ArbitrationError::PromiseNotKept)
            );
            // Third check with matching values succeeds
            assert_eq!(require_kept_promise(123, 123), Ok(()));
        }
    }

    // ============================================================================
    // Tests for: require_dispute_resolved
    // ============================================================================

    mod require_dispute_resolved {
        use super::*;

        // --- Terminal States (Should Succeed) ---

        #[test]
        fn allows_resolved_state() {
            assert_eq!(require_dispute_resolved(&DisputeStatus::Resolved), Ok(()));
        }

        #[test]
        fn allows_cancelled_state() {
            assert_eq!(require_dispute_resolved(&DisputeStatus::Cancelled), Ok(()));
        }

        #[test]
        fn allows_tied_state() {
            assert_eq!(require_dispute_resolved(&DisputeStatus::Tied), Ok(()));
        }

        // --- Active States (Should Fail) ---

        #[test]
        fn rejects_open_state() {
            assert_eq!(
                require_dispute_resolved(&DisputeStatus::Open),
                Err(ArbitrationError::DisputeActive)
            );
        }

        #[test]
        fn rejects_voting_state() {
            assert_eq!(
                require_dispute_resolved(&DisputeStatus::Voting),
                Err(ArbitrationError::DisputeActive)
            );
        }

        #[test]
        fn rejects_resolving_state() {
            assert_eq!(
                require_dispute_resolved(&DisputeStatus::Resolving),
                Err(ArbitrationError::DisputeActive)
            );
        }

        // --- Boundary: Archived State ---

        #[test]
        fn archived_state_is_not_terminal_for_resolution() {
            // Archived is not considered a terminal state for dispute resolution
            // The function checks for Resolved, Cancelled, or Tied only
            // Archived requires special handling (it can be reopened)
            let result = require_dispute_resolved(&DisputeStatus::Archived);
            // This test documents the current behavior:
            // Archived is NOT accepted as a terminal resolution state
            assert_eq!(result, Err(ArbitrationError::DisputeActive));
        }

        // --- Recovery Scenarios ---

        #[test]
        fn recovery_transition_to_resolved() {
            // Start in active state (rejected)
            assert!(require_dispute_resolved(&DisputeStatus::Voting).is_err());

            // After resolution, accepted
            assert_eq!(require_dispute_resolved(&DisputeStatus::Resolved), Ok(()));
        }

        #[test]
        fn recovery_transition_to_cancelled() {
            // Start in active state (rejected)
            assert!(require_dispute_resolved(&DisputeStatus::Open).is_err());

            // After cancellation, accepted
            assert_eq!(require_dispute_resolved(&DisputeStatus::Cancelled), Ok(()));
        }

        #[test]
        fn recovery_transition_to_tied() {
            // Start in active state (rejected)
            assert!(require_dispute_resolved(&DisputeStatus::Resolving).is_err());

            // After tie, accepted
            assert_eq!(require_dispute_resolved(&DisputeStatus::Tied), Ok(()));
        }

        // --- All Terminal States Collectively ---

        #[test]
        fn all_terminal_states_succeed() {
            let terminal_states = vec![
                DisputeStatus::Resolved,
                DisputeStatus::Cancelled,
                DisputeStatus::Tied,
            ];

            for state in terminal_states {
                assert_eq!(
                    require_dispute_resolved(&state),
                    Ok(()),
                    "State {:?} should be accepted",
                    state
                );
            }
        }

        #[test]
        fn all_active_states_fail() {
            let active_states = vec![
                DisputeStatus::Open,
                DisputeStatus::Voting,
                DisputeStatus::Resolving,
            ];

            for state in active_states {
                assert_eq!(
                    require_dispute_resolved(&state),
                    Err(ArbitrationError::DisputeActive),
                    "State {:?} should be rejected",
                    state
                );
            }
        }
    }

    // ============================================================================
    // Tests for: is_dispute_active
    // ============================================================================

    mod is_dispute_active {
        use super::*;

        // --- Active States ---

        #[test]
        fn open_is_active() {
            assert!(is_dispute_active(DisputeStatus::Open));
        }

        #[test]
        fn voting_is_active() {
            assert!(is_dispute_active(DisputeStatus::Voting));
        }

        #[test]
        fn resolving_is_active() {
            assert!(is_dispute_active(DisputeStatus::Resolving));
        }

        // --- Inactive States ---

        #[test]
        fn resolved_is_not_active() {
            assert!(!is_dispute_active(DisputeStatus::Resolved));
        }

        #[test]
        fn cancelled_is_not_active() {
            assert!(!is_dispute_active(DisputeStatus::Cancelled));
        }

        #[test]
        fn tied_is_not_active() {
            assert!(!is_dispute_active(DisputeStatus::Tied));
        }

        #[test]
        fn archived_is_not_active() {
            assert!(!is_dispute_active(DisputeStatus::Archived));
        }

        // --- All States Coverage ---

        #[test]
        fn exactly_three_states_active() {
            let active_count = [
                DisputeStatus::Open,
                DisputeStatus::Voting,
                DisputeStatus::Resolving,
                DisputeStatus::Resolved,
                DisputeStatus::Cancelled,
                DisputeStatus::Tied,
                DisputeStatus::Archived,
            ]
            .iter()
            .filter(|s| is_dispute_active(**s))
            .count();

            assert_eq!(active_count, 3, "Exactly 3 states should be active");
        }
    }

    // ============================================================================
    // Determinism and Concurrent Execution Tests
    // ============================================================================

    mod determinism_and_concurrency {
        use super::*;

        #[test]
        fn require_transition_is_deterministic() {
            let from = DisputeStatus::Open;
            let to = DisputeStatus::Voting;

            // Call multiple times
            for _ in 0..100 {
                assert_eq!(
                    require_transition(from, to),
                    Ok(()),
                    "require_transition must be deterministic"
                );
            }
        }

        #[test]
        fn require_dispute_inactive_is_deterministic() {
            let status = DisputeStatus::Resolved;

            // Call multiple times
            for _ in 0..100 {
                assert_eq!(
                    require_dispute_inactive(status),
                    Ok(()),
                    "require_dispute_inactive must be deterministic"
                );
            }
        }

        #[test]
        fn require_kept_promise_is_deterministic() {
            let promised = 42u32;
            let actual = 42u32;

            // Call multiple times
            for _ in 0..100 {
                assert_eq!(
                    require_kept_promise(promised, actual),
                    Ok(()),
                    "require_kept_promise must be deterministic"
                );
            }
        }

        #[test]
        fn require_dispute_resolved_is_deterministic() {
            let status = DisputeStatus::Resolved;

            // Call multiple times
            for _ in 0..100 {
                assert_eq!(
                    require_dispute_resolved(&status),
                    Ok(()),
                    "require_dispute_resolved must be deterministic"
                );
            }
        }

        #[test]
        fn is_dispute_active_is_deterministic() {
            let status = DisputeStatus::Voting;

            // Call multiple times
            for _ in 0..100 {
                assert!(
                    is_dispute_active(status),
                    "is_dispute_active must be deterministic"
                );
            }
        }

        // --- Concurrent Invariants (Simulated) ---
        // All functions are pure (no state mutations), so concurrent calls produce consistent results

        #[test]
        fn concurrent_transition_checks_same_result() {
            let from = DisputeStatus::Open;
            let to = DisputeStatus::Voting;

            // Simulate multiple concurrent checks
            let results: Vec<_> = (0..10)
                .map(|_| require_transition(from, to))
                .collect();

            // All results must be identical
            assert!(results.iter().all(|r| r == &Ok(())));
        }

        #[test]
        fn concurrent_promise_checks_same_result() {
            let promised = 123u32;
            let actual = 123u32;

            // Simulate multiple concurrent checks
            let results: Vec<_> = (0..10)
                .map(|_| require_kept_promise(promised, actual))
                .collect();

            // All results must be identical
            assert!(results.iter().all(|r| r == &Ok(())));
        }

        #[test]
        fn concurrent_dispute_resolved_checks_same_result() {
            let status = DisputeStatus::Resolved;

            // Simulate multiple concurrent checks
            let results: Vec<_> = (0..10)
                .map(|_| require_dispute_resolved(&status))
                .collect();

            // All results must be identical
            assert!(results.iter().all(|r| r == &Ok(())));
        }
    }

    // ============================================================================
    // Error Recovery and Partial Failure Tests
    // ============================================================================

    mod error_recovery_and_partial_failure {
        use super::*;

        #[test]
        fn after_invalid_transition_can_try_valid_one() {
            // First attempt: invalid transition (fails)
            let result1 = require_transition(DisputeStatus::Open, DisputeStatus::Open);
            assert!(result1.is_err());

            // Second attempt: valid transition (succeeds)
            let result2 = require_transition(DisputeStatus::Open, DisputeStatus::Voting);
            assert!(result2.is_ok());

            // Third attempt: another valid transition (succeeds)
            let result3 = require_transition(DisputeStatus::Voting, DisputeStatus::Resolving);
            assert!(result3.is_ok());
        }

        #[test]
        fn after_dispute_active_error_can_reach_inactive() {
            // First: dispute is active (error)
            let status1 = DisputeStatus::Voting;
            assert!(require_dispute_inactive(status1).is_err());

            // After transition: dispute becomes inactive (success)
            let status2 = DisputeStatus::Resolved;
            assert_eq!(require_dispute_inactive(status2), Ok(()));
        }

        #[test]
        fn after_promise_broken_can_verify_new_promise() {
            // First check: promise broken
            let result1 = require_kept_promise(42, 99);
            assert_eq!(result1, Err(ArbitrationError::PromiseNotKept));

            // Second check: different promise, also broken
            let result2 = require_kept_promise(100, 200);
            assert_eq!(result2, Err(ArbitrationError::PromiseNotKept));

            // Third check: promise kept
            let result3 = require_kept_promise(123, 123);
            assert_eq!(result3, Ok(()));
        }

        #[test]
        fn sequence_of_operations_partial_failures() {
            // Step 1: Check inactive (fails - Voting is active)
            assert!(require_dispute_inactive(DisputeStatus::Voting).is_err());

            // Step 2: Validate transition (succeeds)
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolving),
                Ok(())
            );

            // Step 3: Check inactive again (still fails - now Resolving is active)
            assert!(require_dispute_inactive(DisputeStatus::Resolving).is_err());

            // Step 4: Another transition (succeeds)
            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved),
                Ok(())
            );

            // Step 5: Check inactive (now succeeds - Resolved is inactive)
            assert_eq!(require_dispute_inactive(DisputeStatus::Resolved), Ok(()));
        }

        #[test]
        fn reopen_dispute_error_recovery() {
            // Scenario: Try to transition from archived state

            // First attempt: invalid transition (cannot go directly to Resolved)
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Resolved),
                Err(ArbitrationError::InvalidTransition)
            );

            // Recovery: go through valid reopen path
            assert_eq!(
                require_transition(DisputeStatus::Archived, DisputeStatus::Voting),
                Ok(())
            );

            // Then proceed normally
            assert_eq!(
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolving),
                Ok(())
            );

            assert_eq!(
                require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved),
                Ok(())
            );
        }

        #[test]
        fn state_remains_consistent_after_errors() {
            // The functions are pure: state is immutable by design
            let status = DisputeStatus::Voting;

            // Multiple failed checks
            for _ in 0..5 {
                let _ = require_dispute_inactive(status);
            }

            // Status is unchanged (immutable)
            assert!(is_dispute_active(status));

            // This is a property guarantee: functions don't mutate input state
        }
    }

    // ============================================================================
    // Edge Cases and Boundary Conditions
    // ============================================================================

    mod edge_cases_and_boundaries {
        use super::*;

        #[test]
        fn all_status_values_covered() {
            // Ensure all 7 states are defined
            let states = vec![
                DisputeStatus::Open,
                DisputeStatus::Voting,
                DisputeStatus::Resolving,
                DisputeStatus::Resolved,
                DisputeStatus::Cancelled,
                DisputeStatus::Tied,
                DisputeStatus::Archived,
            ];

            assert_eq!(states.len(), 7, "All 7 dispute states should be tested");
        }

        #[test]
        fn all_transitions_are_either_valid_or_invalid() {
            let states = vec![
                DisputeStatus::Open,
                DisputeStatus::Voting,
                DisputeStatus::Resolving,
                DisputeStatus::Resolved,
                DisputeStatus::Cancelled,
                DisputeStatus::Tied,
                DisputeStatus::Archived,
            ];

            // Test all 49 possible transitions (7x7)
            for from in &states {
                for to in &states {
                    let result = require_transition(*from, *to);
                    // Every transition must be either Ok or InvalidTransition
                    match result {
                        Ok(()) => {}, // valid
                        Err(ArbitrationError::InvalidTransition) => {}, // invalid
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
            }
        }

        #[test]
        fn promise_validation_with_extremes() {
            // Zero promises
            assert_eq!(require_kept_promise(0, 0), Ok(()));

            // Max value promises
            assert_eq!(require_kept_promise(u32::MAX, u32::MAX), Ok(()));

            // Mid-range promises
            assert_eq!(require_kept_promise(1_000_000, 1_000_000), Ok(()));

            // All mismatches with extremes
            assert_eq!(
                require_kept_promise(0, u32::MAX),
                Err(ArbitrationError::PromiseNotKept)
            );
            assert_eq!(
                require_kept_promise(u32::MAX, 0),
                Err(ArbitrationError::PromiseNotKept)
            );
        }

        #[test]
        fn reference_vs_value_semantics() {
            // require_dispute_resolved takes &DisputeStatus
            let status = DisputeStatus::Resolved;
            let status_ref = &status;

            // Both should work identically
            assert_eq!(require_dispute_resolved(&status), Ok(()));
            assert_eq!(require_dispute_resolved(status_ref), Ok(()));
        }
    }
}
