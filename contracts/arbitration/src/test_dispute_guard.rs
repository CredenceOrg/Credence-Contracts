#![cfg(test)]

use super::status::{is_dispute_active, require_dispute_inactive, ArbitrationError, DisputeStatus};

#[test]
fn is_dispute_active_returns_true_for_open() {
    assert!(is_dispute_active(DisputeStatus::Open));
}

#[test]
fn is_dispute_active_returns_true_for_voting() {
    assert!(is_dispute_active(DisputeStatus::Voting));
}

#[test]
fn is_dispute_active_returns_true_for_resolving() {
    assert!(is_dispute_active(DisputeStatus::Resolving));
}

#[test]
fn is_dispute_active_returns_false_for_resolved() {
    assert!(!is_dispute_active(DisputeStatus::Resolved));
}

#[test]
fn is_dispute_active_returns_false_for_cancelled() {
    assert!(!is_dispute_active(DisputeStatus::Cancelled));
}

#[test]
fn is_dispute_active_returns_false_for_tied() {
    assert!(!is_dispute_active(DisputeStatus::Tied));
}

#[test]
fn require_dispute_inactive_allows_resolved() {
    assert!(require_dispute_inactive(DisputeStatus::Resolved).is_ok());
}

#[test]
fn require_dispute_inactive_allows_cancelled() {
    assert!(require_dispute_inactive(DisputeStatus::Cancelled).is_ok());
}

#[test]
fn require_dispute_inactive_allows_tied() {
    assert!(require_dispute_inactive(DisputeStatus::Tied).is_ok());
}

#[test]
fn require_dispute_inactive_blocks_open() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Open),
        Err(ArbitrationError::DisputeActive)
    );
}

#[test]
fn require_dispute_inactive_blocks_voting() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Voting),
        Err(ArbitrationError::DisputeActive)
    );
}

#[test]
fn require_dispute_inactive_blocks_resolving() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Resolving),
        Err(ArbitrationError::DisputeActive)
    );
}
