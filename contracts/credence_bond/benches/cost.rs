// This is an off-chain measurement binary, not deployed WASM. Issue #713
// disallows `format!`/etc. in production contract code; benches never run on
// chain, so we silence the lint locally.
#![allow(clippy::disallowed_macros)]

//! Gas-regression gate for the `credence_bond` contract.
//!
//! Run via `cargo bench -p credence_bond --features gas-bench --bench cost` (or
//! in CI). It measures every tracked entrypoint with [`Env::cost_estimate`],
//! compares against the committed `cost_baseline.json`, prints a table, and
//! **exits non-zero if any metric regressed past the baseline's tolerance**.
//! That non-zero exit is what fails the PR.
//!
//! To intentionally accept new numbers, refresh the baseline with
//! `cargo run -p credence_bond --bin update-cost-baseline`. See
//! `docs/gas-regression.md`.

mod harness;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Result of the gas-regression evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum CostGateOutcome {
    /// No metrics regressed beyond the allowed tolerance.
    Pass {
        tolerance_pct: f64,
        checked_entrypoints: usize,
    },
    /// Baseline file could not be found or read; caller should refresh baseline.
    MissingBaseline {
        path: PathBuf,
        remediation: &'static str,
    },
    /// One or more metrics regressed beyond the allowed tolerance.
    Regression {
        tolerance_pct: f64,
        regressions: Vec<harness::Regression>,
    },
}

/// A structured row for tabular output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub entrypoint: String,
    pub cpu_insns: i64,
    pub delta: String,
    pub rw: String,
}

/// Evaluate current entrypoint costs against a parsed baseline snapshot.
///
/// Returns `CostGateOutcome::Pass` and `ExitCode::SUCCESS` if no metrics regressed
/// past `baseline.tolerance_pct`, or `CostGateOutcome::Regression` and
/// `ExitCode::FAILURE` with the list of failing metrics.
pub fn evaluate_cost_gate(
    baseline: &harness::Baseline,
    current: &BTreeMap<String, harness::EntryCost>,
) -> (CostGateOutcome, ExitCode) {
    let regressions = harness::diff(baseline, current);
    let checked = harness::ENTRYPOINTS
        .iter()
        .filter(|name| current.contains_key(**name))
        .count();

    if regressions.is_empty() {
        (
            CostGateOutcome::Pass {
                tolerance_pct: baseline.tolerance_pct,
                checked_entrypoints: checked,
            },
            ExitCode::SUCCESS,
        )
    } else {
        (
            CostGateOutcome::Regression {
                tolerance_pct: baseline.tolerance_pct,
                regressions,
            },
            ExitCode::FAILURE,
        )
    }
}

/// Handle a missing or unreadable baseline path, returning structured diagnostics.
pub fn handle_missing_baseline(path: &Path) -> (CostGateOutcome, ExitCode) {
    (
        CostGateOutcome::MissingBaseline {
            path: path.to_path_buf(),
            remediation: "cargo run -p credence_bond --bin update-cost-baseline",
        },
        ExitCode::FAILURE,
    )
}

/// Run the gate evaluation given raw baseline text (or an I/O error on load).
pub fn run_gate_from_text(
    baseline_result: Result<&str, &std::io::Error>,
    baseline_path: &Path,
    current: &BTreeMap<String, harness::EntryCost>,
) -> (CostGateOutcome, ExitCode) {
    match baseline_result {
        Ok(text) => {
            let baseline = harness::parse_baseline(text);
            evaluate_cost_gate(&baseline, current)
        }
        Err(_) => handle_missing_baseline(baseline_path),
    }
}

/// Format the baseline-vs-current comparison rows for display or verification.
pub fn format_table_rows(
    baseline: &harness::Baseline,
    current: &BTreeMap<String, harness::EntryCost>,
) -> Vec<TableRow> {
    let mut rows = Vec::new();
    for name in harness::ENTRYPOINTS {
        let Some(c) = current.get(*name) else {
            continue;
        };
        let delta = baseline
            .costs
            .get(*name)
            .map(|b| format!("{:+}", c.cpu_insns - b.cpu_insns))
            .unwrap_or_else(|| "new".to_string());
        rows.push(TableRow {
            entrypoint: (*name).to_string(),
            cpu_insns: c.cpu_insns,
            delta,
            rw: format!("{}/{}", c.read_entries, c.write_entries),
        });
    }
    rows
}

/// Format regression error details for diagnostic output.
pub fn format_regression_lines(regressions: &[harness::Regression]) -> Vec<String> {
    regressions
        .iter()
        .map(|r| {
            format!(
                "  {}::{}  {} -> {}  (+{:.1}%)",
                r.entrypoint, r.metric, r.baseline, r.current, r.pct
            )
        })
        .collect()
}

fn main() -> ExitCode {
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json");
    let current = harness::measure_all();

    let baseline_text = match std::fs::read_to_string(&baseline_path) {
        Ok(t) => t,
        Err(_) => {
            let (_, code) = handle_missing_baseline(&baseline_path);
            eprintln!(
                "no baseline at {} — create one with `cargo run -p credence_bond --bin update-cost-baseline`",
                baseline_path.display()
            );
            return code;
        }
    };
    let baseline = harness::parse_baseline(&baseline_text);

    print_table(&baseline, &current);

    let (outcome, exit_code) = evaluate_cost_gate(&baseline, &current);
    match outcome {
        CostGateOutcome::Pass { tolerance_pct, .. } => {
            println!(
                "\n✓ no gas regressions (tolerance {:.1}%)",
                tolerance_pct
            );
        }
        CostGateOutcome::Regression {
            tolerance_pct,
            regressions,
        } => {
            eprintln!(
                "\n✗ gas regression(s) over {:.1}% tolerance:",
                tolerance_pct
            );
            for line in format_regression_lines(&regressions) {
                eprintln!("{}", line);
            }
            eprintln!(
                "\nIf this change is intended, refresh the baseline:\n  \
                 cargo run -p credence_bond --bin update-cost-baseline"
            );
        }
        CostGateOutcome::MissingBaseline { .. } => unreachable!(),
    }
    exit_code
}

/// Print a per-entrypoint baseline-vs-current table for the headline metrics.
fn print_table(
    baseline: &harness::Baseline,
    current: &BTreeMap<String, harness::EntryCost>,
) {
    println!(
        "{:<16} {:>14} {:>14} {:>10}",
        "entrypoint", "cpu_insns", "Δ cpu", "rw(r/w)"
    );
    for row in format_table_rows(baseline, current) {
        println!(
            "{:<16} {:>14} {:>14} {:>10}",
            row.entrypoint, row.cpu_insns, row.delta, row.rw
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cost(cpu: i64) -> harness::EntryCost {
        harness::EntryCost {
            cpu_insns: cpu,
            mem_bytes: 50_000,
            read_entries: 5,
            write_entries: 2,
            read_bytes: 1_000,
            write_bytes: 400,
        }
    }

    fn sample_baseline(tolerance_pct: f64) -> harness::Baseline {
        let mut costs = BTreeMap::new();
        for ep in harness::ENTRYPOINTS {
            costs.insert((*ep).to_string(), sample_cost(100_000));
        }
        harness::Baseline {
            tolerance_pct,
            costs,
        }
    }

    #[test]
    fn test_gate_pass_identical_costs() {
        let baseline = sample_baseline(10.0);
        let current = baseline.costs.clone();
        let (outcome, code) = evaluate_cost_gate(&baseline, &current);
        assert_eq!(code, ExitCode::SUCCESS);
        match outcome {
            CostGateOutcome::Pass {
                tolerance_pct,
                checked_entrypoints,
            } => {
                assert_eq!(tolerance_pct, 10.0);
                assert_eq!(checked_entrypoints, 6);
            }
            _ => panic!("expected Pass outcome"),
        }
    }

    #[test]
    fn test_gate_pass_exact_tolerance_limit() {
        let baseline = sample_baseline(10.0);
        let mut current = baseline.costs.clone();
        current.get_mut("create_bond").unwrap().cpu_insns = 110_000;
        let (outcome, code) = evaluate_cost_gate(&baseline, &current);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(matches!(outcome, CostGateOutcome::Pass { .. }));
    }

    #[test]
    fn test_gate_regression_one_above_limit() {
        let baseline = sample_baseline(10.0);
        let mut current = baseline.costs.clone();
        current.get_mut("create_bond").unwrap().cpu_insns = 110_001;
        let (outcome, code) = evaluate_cost_gate(&baseline, &current);
        assert_eq!(code, ExitCode::FAILURE);
        match outcome {
            CostGateOutcome::Regression {
                tolerance_pct,
                regressions,
            } => {
                assert_eq!(tolerance_pct, 10.0);
                assert_eq!(regressions.len(), 1);
                assert_eq!(regressions[0].entrypoint, "create_bond");
                assert_eq!(regressions[0].metric, "cpu_insns");
                assert_eq!(regressions[0].current, 110_001);
            }
            _ => panic!("expected Regression outcome"),
        }
    }

    #[test]
    fn test_missing_baseline_diagnostics() {
        let path = PathBuf::from("nonexistent/baseline.json");
        let (outcome, code) = handle_missing_baseline(&path);
        assert_eq!(code, ExitCode::FAILURE);
        match outcome {
            CostGateOutcome::MissingBaseline {
                path: p,
                remediation,
            } => {
                assert_eq!(p, path);
                assert!(remediation.contains("update-cost-baseline"));
            }
            _ => panic!("expected MissingBaseline outcome"),
        }
    }

    #[test]
    fn test_table_rows_delta_formatting() {
        let baseline = sample_baseline(10.0);
        let mut current = baseline.costs.clone();
        current.get_mut("create_bond").unwrap().cpu_insns = 105_000;
        current.get_mut("withdraw").unwrap().cpu_insns = 90_000;

        let rows = format_table_rows(&baseline, &current);
        assert_eq!(rows.len(), 6);

        let create = rows.iter().find(|r| r.entrypoint == "create_bond").unwrap();
        assert_eq!(create.delta, "+5000");
        assert_eq!(create.rw, "5/2");

        let withdraw = rows.iter().find(|r| r.entrypoint == "withdraw").unwrap();
        assert_eq!(withdraw.delta, "-10000");
    }

    #[test]
    fn test_regression_lines_formatting() {
        let regressions = vec![harness::Regression {
            entrypoint: "create_bond".to_string(),
            metric: "cpu_insns",
            baseline: 100_000,
            current: 120_000,
            pct: 20.0,
        }];
        let lines = format_regression_lines(&regressions);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            "  create_bond::cpu_insns  100000 -> 120000  (+20.0%)"
        );
    }
}
