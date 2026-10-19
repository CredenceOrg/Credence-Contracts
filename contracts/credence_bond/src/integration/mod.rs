//! Integration tests for bond lifecycle (#47).
///
/// This module aggregates the integration test suites for the bond lifecycle.
/// Each sub-module exercises a distinct area of the bond state machine:
///
/// - `test_bond_lifecycle`: creation, activation, release, and recovery
///   paths including boundary values and duplicate inputs.
/// - `test_governance`: governance-controlled transitions and authorization
///   checks.
///
/// The module is deliberately thin: it only declares the test submodules so
/// that the integration binary can be compiled and run independently of the
/// crate's unit tests. Any shared fixtures or helpers must live in the
/// submodules themselves to avoid hidden coupling between test areas.

/// Bond lifecycle integration tests.
///
/// Covers the full lifecycle from bond creation through release, including
/// boundary cases (zero/max amounts, duplicate creation attempts) and
/// recovery paths (partial failure, retry, stale state).
mod test_bond_lifecycle;

/// Governance integration tests.
///
/// Covers authorization and permission invariants around governance
/// transitions, including unauthorized callers and concurrent execution.
mod test_governance;
