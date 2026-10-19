// Integration and boundary/recovery test suite for benches/cost.rs
#![allow(clippy::disallowed_macros)]

#[path = "../benches/cost.rs"]
mod cost;

use cost::harness::{self, Baseline, EntryCost, Regression, ENTRYPOINTS, TOLERANCE_PCT};
use cost::{format_missing_baseline, format_regressions, format_table, evaluate_baseline_text, run_gate_from_path, GateDecision};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn mock_cost(cpu: i64, mem: i64, re: u32, we: u32, rb: u32, wb: u32) -> EntryCost {
    EntryCost {
        cpu_insns: cpu,
        mem_bytes: mem,
        read_entries: re,
        write_entries: we,
        read_bytes: rb,
        write_bytes: wb,
    }
}

fn mock_baseline(ep: &str, cost: EntryCost, tol: f64) -> Baseline {
    let mut costs = BTreeMap::new();
    costs.insert(ep.to_string(), cost);
    Baseline {
        tolerance_pct: tol,
        costs,
    }
}

#[test]
fn test_live_cost_baseline_file_parses_cleanly() {
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json");
    let content = std::fs::read_to_string(&baseline_path).expect("cost_baseline.json must exist in repo");

    let parsed = harness::try_parse_baseline(&content).expect("live cost_baseline.json must parse without errors");
    assert_eq!(parsed.tolerance_pct, 10.0);

    for ep in ENTRYPOINTS {
        assert!(
            parsed.costs.contains_key(*ep),
            "tracked entrypoint '{ep}' must be in cost_baseline.json"
        );
        let c = &parsed.costs[*ep];
        assert!(c.cpu_insns > 0, "{ep} cpu_insns should be positive");
        assert!(c.mem_bytes > 0, "{ep} mem_bytes should be positive");
    }
}

#[test]
fn test_boundary_tolerance_exact_vs_exceeded() {
    let base_cost = mock_cost(10_000, 5_000, 2, 2, 200, 200);
    let baseline = mock_baseline("create_bond", base_cost, 10.0);

    // Exact limit: 10_000 * 1.10 = 11_000
    let mut current_at_limit = BTreeMap::new();
    current_at_limit.insert("create_bond".to_string(), mock_cost(11_000, 5_000, 2, 2, 200, 200));

    let regressions_at_limit = harness::diff(&baseline, &current_at_limit);
    assert!(
        regressions_at_limit.is_empty(),
        "Cost exactly at tolerance limit must pass"
    );

    // Exceeded limit: 11_001
    let mut current_exceeded = BTreeMap::new();
    current_exceeded.insert("create_bond".to_string(), mock_cost(11_001, 5_000, 2, 2, 200, 200));

    let regressions_exceeded = harness::diff(&baseline, &current_exceeded);
    assert_eq!(regressions_exceeded.len(), 1);
    assert_eq!(regressions_exceeded[0].metric, "cpu_insns");
    assert_eq!(regressions_exceeded[0].baseline, 10_000);
    assert_eq!(regressions_exceeded[0].current, 11_001);
}

#[test]
fn test_boundary_all_six_metrics_tracked_individually() {
    let base = mock_cost(100, 100, 10, 10, 50, 50);
    let baseline = mock_baseline("create_bond", base, 10.0);

    let test_cases = [
        ("cpu_insns", mock_cost(120, 100, 10, 10, 50, 50)),
        ("mem_bytes", mock_cost(100, 120, 10, 10, 50, 50)),
        ("read_entries", mock_cost(100, 100, 12, 10, 50, 50)),
        ("write_entries", mock_cost(100, 100, 10, 12, 50, 50)),
        ("read_bytes", mock_cost(100, 100, 10, 10, 60, 50)),
        ("write_bytes", mock_cost(100, 100, 10, 10, 50, 60)),
    ];

    for (expected_metric, cur_cost) in test_cases {
        let mut cur = BTreeMap::new();
        cur.insert("create_bond".to_string(), cur_cost);
        let diffs = harness::diff(&baseline, &cur);
        assert_eq!(diffs.len(), 1, "Expected single regression for {expected_metric}");
        assert_eq!(diffs[0].metric, expected_metric);
    }
}

#[test]
fn test_boundary_zero_to_positive_growth() {
    let base = mock_cost(100, 100, 0, 0, 0, 0);
    let baseline = mock_baseline("create_bond", base, 10.0);

    let mut cur = BTreeMap::new();
    cur.insert("create_bond".to_string(), mock_cost(100, 100, 1, 0, 0, 0));

    let diffs = harness::diff(&baseline, &cur);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].metric, "read_entries");
    assert_eq!(diffs[0].pct, 100.0);
}

#[test]
fn test_boundary_improvements_produce_no_regressions() {
    let base = mock_cost(10_000, 5_000, 5, 5, 500, 500);
    let baseline = mock_baseline("create_bond", base, 10.0);

    let mut cur = BTreeMap::new();
    cur.insert("create_bond".to_string(), mock_cost(5_000, 2_500, 2, 2, 200, 200));

    let diffs = harness::diff(&baseline, &cur);
    assert!(diffs.is_empty(), "Cost improvements must never be flagged as regressions");

    let table = format_table(&baseline, &cur);
    assert!(table.contains("-5000"));
}

#[test]
fn test_recovery_empty_or_whitespace_baseline() {
    let cur = BTreeMap::new();

    let decision_empty = evaluate_baseline_text("", &cur);
    assert_eq!(decision_empty.exit_code(), ExitCode::FAILURE);

    let decision_ws = evaluate_baseline_text("   \n\t  \n", &cur);
    assert_eq!(decision_ws.exit_code(), ExitCode::FAILURE);
}

#[test]
fn test_recovery_malformed_json_syntax() {
    let bad_samples = [
        "not json at all",
        "{ \"schema\": \"v1\", ",
        "{ \"entrypoints\": { \"create_bond\": { \"cpu_insns\": } } }",
        "{ \"entrypoints\": { \"create_bond\": { \"cpu_insns\": \"not_a_number\" } } }",
        "{ \"entrypoints\": { \"create_bond\": { \"read_entries\": -5 } } }",
    ];

    let cur = BTreeMap::new();
    for sample in bad_samples {
        let decision = evaluate_baseline_text(sample, &cur);
        assert_eq!(
            decision.exit_code(),
            ExitCode::FAILURE,
            "Failed to reject malformed sample: {sample}"
        );
        match decision {
            GateDecision::InvalidBaseline { error } => {
                assert!(!error.is_empty());
            }
            _ => panic!("Expected InvalidBaseline for {sample}"),
        }
    }
}

#[test]
fn test_recovery_missing_baseline_file() {
    let cur = BTreeMap::new();
    let bogus_path = Path::new("non_existent_directory_for_test/cost_baseline.json");
    let (code, decision) = run_gate_from_path(bogus_path, &cur);

    assert_eq!(code, ExitCode::FAILURE);
    let err_msg = format_missing_baseline(bogus_path);
    assert!(err_msg.contains("no baseline at"));
    assert!(err_msg.contains("cargo run -p credence_bond --bin update-cost-baseline"));

    match decision {
        GateDecision::InvalidBaseline { error } => {
            assert_eq!(error, err_msg);
        }
        _ => panic!("Expected InvalidBaseline decision"),
    }
}

#[test]
fn test_gate_reporting_and_table_output() {
    let base = mock_cost(50_000, 10_000, 2, 2, 200, 200);
    let baseline = mock_baseline("create_bond", base, 10.0);
    let json = harness::to_json(&baseline.costs);

    let mut cur = BTreeMap::new();
    cur.insert("create_bond".to_string(), mock_cost(60_000, 10_000, 2, 2, 200, 200));

    let decision = evaluate_baseline_text(&json, &cur);
    assert_eq!(decision.exit_code(), ExitCode::FAILURE);

    match decision {
        GateDecision::Regressed { regressions, message, table, .. } => {
            assert_eq!(regressions.len(), 1);
            assert!(message.contains("gas regression(s)"));
            assert!(message.contains("50000 -> 60000"));
            assert!(table.contains("+10000"));
        }
        _ => panic!("Expected Regressed decision"),
    }
}
