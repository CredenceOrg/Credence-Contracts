/// Per-entrypoint resource budget ceiling assertions for the `credence_bond` contract.
///
/// # Purpose
///
/// Soroban mainnet enforces hard CPU-instruction and memory-byte limits per
/// transaction. A contract that passes unit tests may still fail on-chain if a
/// future change inflates its resource footprint beyond those limits. This
/// module provides a single assertion helper, [`assert_budget_under`], that
/// captures the real `env.cost_estimate().budget()` snapshot after an
/// entrypoint call and panics if either dimension exceeds its pre-defined
/// ceiling.
///
/// # Tolerance
///
/// Ceilings are set at **2× the baseline** measured on a clean single-identity
/// run. This headroom accommodates minor SDK version fluctuations (typically
/// < 5 % drift per release) while still detecting regressions that would
/// threaten mainnet viability. The factor is intentionally conservative: small
/// enough to catch real regressions, large enough to survive routine SDK
/// upgrades without constant ceiling bumps.
///
/// If a future SDK upgrade legitimately shifts costs beyond the 2× headroom,
/// update the corresponding constant in [`budget_ceilings`] and document the
/// reason in `docs/budget-ceilings.md`.
///
/// # Usage
///
/// ```rust,ignore
/// use crate::test_budget_helper::{assert_budget_under, budget_ceilings};
///
/// let e = Env::default();
/// e.mock_all_auths();
/// // … set up contract …
/// e.cost_estimate().budget().reset_default();
/// client.create_bond(/* … */);
/// assert_budget_under(&e, budget_ceilings::CREATE_BOND);
/// ```

extern crate std;

use soroban_sdk::Env;

/// A pair of `(cpu_instructions_max, memory_bytes_max)` ceilings for one
/// entrypoint.
///
/// Exceeding either dimension on mainnet causes the transaction to be rejected,
/// so both must be enforced independently. Values are chosen as 2× the
/// empirical baseline measured in CI (see `docs/budget-ceilings.md`).
#[derive(Clone, Copy, Debug)]
pub struct BudgetCeiling {
    /// Maximum CPU instruction units for the entrypoint (inclusive).
    pub cpu_max: u64,
    /// Maximum memory bytes for the entrypoint (inclusive).
    pub mem_max: u64,
}

/// Pre-defined ceilings for each write-path entrypoint on `CredenceBond`.
///
/// See `docs/budget-ceilings.md` for the full ceiling table and methodology.
pub mod budget_ceilings {
    use super::BudgetCeiling;

    // -----------------------------------------------------------------------
    // Baseline measurements (single-identity, minimal state) × 2 headroom
    // -----------------------------------------------------------------------

    /// `create_bond` – allocates bond storage, emits tier-change event.
    /// Applies to both rolling and non-rolling variants.
    pub const CREATE_BOND: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `top_up` – reads bond, adds amount, writes bond back.
    pub const TOP_UP: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `extend_duration` – reads bond, extends duration, writes bond back.
    pub const EXTEND_DURATION: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `request_withdrawal` – reads bond, records timestamp, emits event.
    pub const REQUEST_WITHDRAWAL: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `withdraw` – reads bond, validates lockup/notice, subtracts amount.
    pub const WITHDRAW: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `withdraw_early` – reads bond, applies penalty calculation, subtracts.
    pub const WITHDRAW_EARLY: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `slash` / `slash_bond` – admin auth, reads bond, increases
    /// `slashed_amount`, acquires/releases reentrancy lock, optional callback.
    pub const SLASH_BOND: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `add_attestation` – nonce check, dedup check, weight computation,
    /// stores attestation + subject list. Scales with attestation payload size.
    pub const ADD_ATTESTATION: BudgetCeiling = BudgetCeiling {
        cpu_max: 400_000_000,
        mem_max: 6_000_000,
    };

    /// `add_attestation` with a max-size attestation payload (64-byte data)
    /// and a subject that already has many prior attestations (max-list edge
    /// case). The ceiling is higher to reflect larger Vec serialization.
    pub const ADD_ATTESTATION_MAX_SIZE: BudgetCeiling = BudgetCeiling {
        cpu_max: 600_000_000,
        mem_max: 10_000_000,
    };

    /// `withdraw_bond` – reentrancy-guarded withdrawal (partial or full amount).
    pub const WITHDRAW_BOND: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `collect_fees` – reentrancy-guarded fee collection with optional
    /// callback dispatch.
    pub const COLLECT_FEES: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };

    /// `renew_if_rolling` – checks rolling state, applies renewal if period
    /// ended, emits bond-renewed event.
    pub const RENEW_IF_ROLLING: BudgetCeiling = BudgetCeiling {
        cpu_max: 200_000_000,
        mem_max: 4_000_000,
    };
}

/// Assert that the most-recent `env.cost_estimate().budget()` snapshot is
/// within the provided ceiling.
///
/// # Panics
///
/// Panics with a descriptive message if either `cpu_instructions` or
/// `memory_bytes` exceeds its ceiling. The message includes the actual value,
/// the ceiling, and the excess so that CI output pinpoints the regression
/// immediately.
///
/// # Tolerance note
///
/// Ceilings in [`budget_ceilings`] are set at 2× empirical baseline. This
/// function enforces them as hard caps; there is no additional tolerance
/// applied here. If you need a looser check during exploratory work, pass a
/// custom [`BudgetCeiling`] directly.
pub fn assert_budget_under(e: &Env, ceiling: BudgetCeiling) {
    let budget = e.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    if cpu > ceiling.cpu_max {
        std::panic!(
            "budget ceiling EXCEEDED for CPU instructions: \
             actual={cpu} ceiling={max} excess={excess}",
            cpu = cpu,
            max = ceiling.cpu_max,
            excess = cpu - ceiling.cpu_max,
        );
    }
    if mem > ceiling.mem_max {
        std::panic!(
            "budget ceiling EXCEEDED for memory bytes: \
             actual={mem} ceiling={max} excess={excess}",
            mem = mem,
            max = ceiling.mem_max,
            excess = mem - ceiling.mem_max,
        );
    }
}