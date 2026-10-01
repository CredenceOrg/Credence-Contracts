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

pub mod harness;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub use harness::{Baseline, BaselineError, EntryCost, Regression, ENTRYPOINTS, TOLERANCE_PCT};

/// Outcome of evaluating current gas/cost measurements against baseline.
#[derive(Clone, Debug, PartialEq)]
pub enum GateDecision {
    /// No metrics regressed past tolerance.
    Pass {
        tolerance_pct: f64,
        table: String,
        message: String,
    },
    /// One or more metrics regressed past tolerance.
    Regressed {
        tolerance_pct: f64,
        table: String,
        regressions: Vec<Regression>,
        message: String,
    },
    /// The baseline document could not be read or parsed.
    InvalidBaseline {
        error: String,
    },
}

impl GateDecision {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            GateDecision::Pass { .. } => ExitCode::SUCCESS,
            GateDecision::Regressed { .. } | GateDecision::InvalidBaseline { .. } => {
                ExitCode::FAILURE
            }
        }
    }
}

/// Format the baseline-vs-current comparison table for headline metrics.
pub fn format_table(
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<16} {:>14} {:>14} {:>10}\n",
        "entrypoint", "cpu_insns", "Δ cpu", "rw(r/w)"
    ));
    for name in ENTRYPOINTS {
        let Some(c) = current.get(*name) else {
            continue;
        };
        let delta = baseline
            .costs
            .get(*name)
            .map(|b| format!("{:+}", c.cpu_insns - b.cpu_insns))
            .unwrap_or_else(|| "new".to_string());
        out.push_str(&format!(
            "{:<16} {:>14} {:>14} {:>10}\n",
            name,
            c.cpu_insns,
            delta,
            format!("{}/{}", c.read_entries, c.write_entries)
        ));
    }
    out
}

/// Format the regression error report.
pub fn format_regressions(regressions: &[Regression], tolerance_pct: f64) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n✗ gas regression(s) over {:.1}% tolerance:\n",
        tolerance_pct
    ));
    for r in regressions {
        out.push_str(&format!(
            "  {}::{}  {} -> {}  (+{:.1}%)\n",
            r.entrypoint, r.metric, r.baseline, r.current, r.pct
        ));
    }
    out.push_str(
        "\nIf this change is intended, refresh the baseline:\n  \
         cargo run -p credence_bond --bin update-cost-baseline\n",
    );
    out
}

/// Format the success notification.
pub fn format_success(tolerance_pct: f64) -> String {
    format!("\n✓ no gas regressions (tolerance {:.1}%)\n", tolerance_pct)
}

/// Format an error message when the baseline file does not exist.
pub fn format_missing_baseline(path: &Path) -> String {
    format!(
        "no baseline at {} — create one with `cargo run -p credence_bond --bin update-cost-baseline`",
        path.display()
    )
}

/// Evaluate current measurements against raw baseline JSON text.
pub fn evaluate_baseline_text(
    baseline_text: &str,
    current: &BTreeMap<String, EntryCost>,
) -> GateDecision {
    let baseline = match harness::try_parse_baseline(baseline_text) {
        Ok(b) => b,
        Err(err) => {
            return GateDecision::InvalidBaseline {
                error: err.to_string(),
            };
        }
    };

    let table = format_table(&baseline, current);
    let regressions = harness::diff(&baseline, current);

    if regressions.is_empty() {
        let message = format_success(baseline.tolerance_pct);
        GateDecision::Pass {
            tolerance_pct: baseline.tolerance_pct,
            table,
            message,
        }
    } else {
        let message = format_regressions(&regressions, baseline.tolerance_pct);
        GateDecision::Regressed {
            tolerance_pct: baseline.tolerance_pct,
            table,
            regressions,
            message,
        }
    }
}

/// Run the gate given a baseline path and current measurements.
pub fn run_gate_from_path(
    baseline_path: &Path,
    current: &BTreeMap<String, EntryCost>,
) -> (ExitCode, GateDecision) {
    let baseline_text = match std::fs::read_to_string(baseline_path) {
        Ok(t) => t,
        Err(_) => {
            let error = format_missing_baseline(baseline_path);
            return (
                ExitCode::FAILURE,
                GateDecision::InvalidBaseline { error },
            );
        }
    };

    let decision = evaluate_baseline_text(&baseline_text, current);
    (decision.exit_code(), decision)
}

fn main() -> ExitCode {
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json");
    let current = harness::measure_all();

    let (code, decision) = run_gate_from_path(&baseline_path, &current);
    match decision {
        GateDecision::Pass { table, message, .. } => {
            print!("{table}");
            print!("{message}");
        }
        GateDecision::Regressed { table, message, .. } => {
            print!("{table}");
            eprint!("{message}");
        }
        GateDecision::InvalidBaseline { ref error } => {
            eprintln!("{error}");
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cost(cpu: i64) -> EntryCost {
        EntryCost {
            cpu_insns: cpu,
            mem_bytes: 10_000,
            read_entries: 2,
            write_entries: 2,
            read_bytes: 500,
            write_bytes: 500,
        }
    }

    fn sample_baseline(ep: &str, cpu: i64, tol: f64) -> Baseline {
        let mut costs = BTreeMap::new();
        costs.insert(ep.to_string(), sample_cost(cpu));
        Baseline {
            tolerance_pct: tol,
            costs,
        }
    }

    #[test]
    fn test_gate_passes_when_costs_identical() {
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(100_000));
        let baseline = sample_baseline("create_bond", 100_000, 10.0);
        let json = harness::to_json(&baseline.costs);

        let decision = evaluate_baseline_text(&json, &current);
        assert!(matches!(decision, GateDecision::Pass { .. }));
        assert_eq!(decision.exit_code(), ExitCode::SUCCESS);
    }

    #[test]
    fn test_boundary_exact_tolerance_threshold_passes() {
        // Base: 1,000, Tolerance: 10% -> Limit: 1,100
        // Current: 1,100 -> exactly equal to limit, should pass
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(1_100));
        let baseline = sample_baseline("create_bond", 1_000, 10.0);
        let regressions = harness::diff(&baseline, &current);
        assert!(
            regressions.is_empty(),
            "exact tolerance boundary must not trigger regression"
        );
    }

    #[test]
    fn test_boundary_one_above_tolerance_threshold_fails() {
        // Base: 1,000, Tolerance: 10% -> Limit: 1,100
        // Current: 1,101 -> strictly above limit, must trigger regression
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(1_101));
        let baseline = sample_baseline("create_bond", 1_000, 10.0);
        let regressions = harness::diff(&baseline, &current);
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "cpu_insns");
        assert_eq!(regressions[0].baseline, 1_000);
        assert_eq!(regressions[0].current, 1_101);
        assert!((regressions[0].pct - 10.1).abs() < 1e-6);
    }

    #[test]
    fn test_boundary_cost_reduction_passes_with_negative_delta() {
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(80_000));
        let baseline = sample_baseline("create_bond", 100_000, 10.0);

        let regressions = harness::diff(&baseline, &current);
        assert!(regressions.is_empty());

        let table = format_table(&baseline, &current);
        assert!(table.contains("-20000"));
    }

    #[test]
    fn test_boundary_zero_baseline_cases() {
        // 0 -> 0: no regression
        let mut current = BTreeMap::new();
        let mut c = sample_cost(1_000);
        c.write_entries = 0;
        current.insert("create_bond".to_string(), c);

        let mut b_cost = sample_cost(1_000);
        b_cost.write_entries = 0;
        let mut baseline_map = BTreeMap::new();
        baseline_map.insert("create_bond".to_string(), b_cost);
        let baseline = Baseline {
            tolerance_pct: 10.0,
            costs: baseline_map,
        };

        let regressions = harness::diff(&baseline, &current);
        assert!(regressions.is_empty());

        // 0 -> 1: regression with 100.0% pct reported
        let mut current_grew = BTreeMap::new();
        let mut c_grew = sample_cost(1_000);
        c_grew.write_entries = 1;
        current_grew.insert("create_bond".to_string(), c_grew);

        let regressions = harness::diff(&baseline, &current_grew);
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "write_entries");
        assert_eq!(regressions[0].pct, 100.0);
    }

    #[test]
    fn test_boundary_new_entrypoint_displayed_as_new() {
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(50_000));
        // Baseline has no entrypoints recorded
        let baseline = Baseline {
            tolerance_pct: 10.0,
            costs: BTreeMap::new(),
        };

        let regressions = harness::diff(&baseline, &current);
        assert!(regressions.is_empty());

        let table = format_table(&baseline, &current);
        assert!(table.contains("new"));
    }

    #[test]
    fn test_boundary_stale_baseline_entrypoint_skipped() {
        let current = BTreeMap::new();
        let baseline = sample_baseline("create_bond", 50_000, 10.0);
        let regressions = harness::diff(&baseline, &current);
        assert!(regressions.is_empty());
    }

    #[test]
    fn test_boundary_custom_tolerance_percentage() {
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(1_200));
        // With 25% tolerance, 1000 -> 1200 (+20%) passes
        let baseline_25 = sample_baseline("create_bond", 1_000, 25.0);
        assert!(harness::diff(&baseline_25, &current).is_empty());

        // With 15% tolerance, 1000 -> 1200 (+20%) fails
        let baseline_15 = sample_baseline("create_bond", 1_000, 15.0);
        assert_eq!(harness::diff(&baseline_15, &current).len(), 1);
    }

    #[test]
    fn test_recovery_empty_baseline_text() {
        let current = BTreeMap::new();
        let decision = evaluate_baseline_text("", &current);
        match decision {
            GateDecision::InvalidBaseline { ref error } => {
                assert!(error.contains("empty"));
            }
            _ => panic!("expected InvalidBaseline for empty text"),
        }
        assert_eq!(decision.exit_code(), ExitCode::FAILURE);
    }

    #[test]
    fn test_recovery_whitespace_baseline_text() {
        let current = BTreeMap::new();
        let decision = evaluate_baseline_text("   \n\t  ", &current);
        assert!(matches!(decision, GateDecision::InvalidBaseline { .. }));
    }

    #[test]
    fn test_recovery_malformed_json() {
        let current = BTreeMap::new();
        let decision = evaluate_baseline_text("{ unterminated json", &current);
        match decision {
            GateDecision::InvalidBaseline { ref error } => {
                assert!(error.contains("malformed") || error.contains("unterminated"));
            }
            _ => panic!("expected InvalidBaseline for malformed json"),
        }
    }

    #[test]
    fn test_recovery_missing_entrypoints_section() {
        let current = BTreeMap::new();
        let decision = evaluate_baseline_text("{\"tolerance_pct\": 10.0}", &current);
        match decision {
            GateDecision::InvalidBaseline { ref error } => {
                assert!(error.contains("entrypoints"));
            }
            _ => panic!("expected InvalidBaseline for missing entrypoints"),
        }
    }

    #[test]
    fn test_recovery_missing_individual_metric() {
        let bad_json = r#"{
            "tolerance_pct": 10.0,
            "entrypoints": {
                "create_bond": {
                    "mem_bytes": 100,
                    "read_entries": 1,
                    "write_entries": 1,
                    "read_bytes": 50,
                    "write_bytes": 50
                }
            }
        }"#;
        let current = BTreeMap::new();
        let decision = evaluate_baseline_text(bad_json, &current);
        match decision {
            GateDecision::InvalidBaseline { ref error } => {
                assert!(error.contains("cpu_insns"));
                assert!(error.contains("create_bond"));
            }
            _ => panic!("expected InvalidBaseline for missing metric"),
        }
    }

    #[test]
    fn test_recovery_missing_file_path() {
        let current = BTreeMap::new();
        let (code, decision) = run_gate_from_path(
            Path::new("path/that/does/not/exist/baseline.json"),
            &current,
        );
        assert_eq!(code, ExitCode::FAILURE);
        match decision {
            GateDecision::InvalidBaseline { ref error } => {
                assert!(error.contains("update-cost-baseline"));
            }
            _ => panic!("expected InvalidBaseline for missing file"),
        }
    }

    #[test]
    fn test_table_formatting_and_headers() {
        let mut current = BTreeMap::new();
        current.insert("create_bond".to_string(), sample_cost(50_000));
        let baseline = sample_baseline("create_bond", 50_000, 10.0);

        let table = format_table(&baseline, &current);
        assert!(table.contains("entrypoint"));
        assert!(table.contains("cpu_insns"));
        assert!(table.contains("Δ cpu"));
        assert!(table.contains("rw(r/w)"));
        assert!(table.contains("create_bond"));
        assert!(table.contains("+0"));
    }

    #[test]
    fn test_regression_formatting_output() {
        let regressions = vec![Regression {
            entrypoint: "create_bond".to_string(),
            metric: "cpu_insns",
            baseline: 1_000,
            current: 1_250,
            pct: 25.0,
        }];
        let formatted = format_regressions(&regressions, 10.0);
        assert!(formatted.contains("create_bond::cpu_insns"));
        assert!(formatted.contains("1000 -> 1250"));
        assert!(formatted.contains("+25.0%"));
        assert!(formatted.contains("update-cost-baseline"));
    }

    #[test]
    fn test_roundtrip_to_json_and_evaluate() {
        let mut costs = BTreeMap::new();
        costs.insert("create_bond".to_string(), sample_cost(73_685));
        costs.insert("top_up".to_string(), sample_cost(102_178));

        let json = harness::to_json(&costs);
        let parsed = harness::try_parse_baseline(&json).expect("should parse valid to_json output");
        assert_eq!(parsed.tolerance_pct, TOLERANCE_PCT);
        assert_eq!(parsed.costs.len(), 2);

        let decision = evaluate_baseline_text(&json, &costs);
        assert!(matches!(decision, GateDecision::Pass { .. }));
    }
}
