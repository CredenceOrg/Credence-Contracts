// This is an off-chain integration test binary, not deployed WASM. Issue #713
// disallows `format!`/etc. in production contract code; integration tests never
// run on chain, so we silence the lint locally.
#![allow(clippy::disallowed_macros)]

//! Boundary test coverage for `contracts/credence_bond/benches/cost.rs` (issue #1313).
//!
//! Pins deterministic behavior at exact limits, off-by-one thresholds, zero baselines,
//! negative deltas, extreme numeric values, multi-metric triggers, and set boundaries.

use std::collections::BTreeMap;
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryCost {
    pub cpu_insns: i64,
    pub mem_bytes: i64,
    pub read_entries: u32,
    pub write_entries: u32,
    pub read_bytes: u32,
    pub write_bytes: u32,
}

pub const ENTRYPOINTS: &[&str] = &[
    "create_bond",
    "top_up",
    "withdraw",
    "withdraw_early",
    "slash_bond",
    "add_attestation",
];

pub const TOLERANCE_PCT: f64 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Regression {
    pub entrypoint: String,
    pub metric: &'static str,
    pub baseline: i64,
    pub current: i64,
    pub pct: f64,
}

pub struct Baseline {
    pub tolerance_pct: f64,
    pub costs: BTreeMap<String, EntryCost>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CostGateOutcome {
    Pass {
        tolerance_pct: f64,
        checked_entrypoints: usize,
    },
    MissingBaseline {
        path: std::path::PathBuf,
        remediation: &'static str,
    },
    Regression {
        tolerance_pct: f64,
        regressions: Vec<Regression>,
    },
}

pub fn diff(baseline: &Baseline, current: &BTreeMap<String, EntryCost>) -> Vec<Regression> {
    let mut regressions = Vec::new();
    let factor = 1.0 + baseline.tolerance_pct / 100.0;
    for name in ENTRYPOINTS {
        let (Some(b), Some(c)) = (baseline.costs.get(*name), current.get(*name)) else {
            continue;
        };
        let metrics: [(&'static str, i64, i64); 6] = [
            ("cpu_insns", b.cpu_insns, c.cpu_insns),
            ("mem_bytes", b.mem_bytes, c.mem_bytes),
            ("read_entries", b.read_entries as i64, c.read_entries as i64),
            ("write_entries", b.write_entries as i64, c.write_entries as i64),
            ("read_bytes", b.read_bytes as i64, c.read_bytes as i64),
            ("write_bytes", b.write_bytes as i64, c.write_bytes as i64),
        ];
        for (metric, base, cur) in metrics {
            let limit = (base as f64) * factor;
            if (cur as f64) > limit && cur > base {
                let pct = if base == 0 {
                    100.0
                } else {
                    (cur - base) as f64 / base as f64 * 100.0
                };
                regressions.push(Regression {
                    entrypoint: (*name).to_string(),
                    metric,
                    baseline: base,
                    current: cur,
                    pct,
                });
            }
        }
    }
    regressions
}

pub fn evaluate_cost_gate(
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> (CostGateOutcome, ExitCode) {
    let regressions = diff(baseline, current);
    let checked = ENTRYPOINTS
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

pub fn format_table_rows(
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> Vec<(&'static str, i64, String, String)> {
    let mut rows = Vec::new();
    for name in ENTRYPOINTS {
        let Some(c) = current.get(*name) else {
            continue;
        };
        let delta = baseline
            .costs
            .get(*name)
            .map(|b| format!("{:+}", c.cpu_insns - b.cpu_insns))
            .unwrap_or_else(|| "new".to_string());
        rows.push((
            *name,
            c.cpu_insns,
            delta,
            format!("{}/{}", c.read_entries, c.write_entries),
        ));
    }
    rows
}

fn sample_cost(cpu: i64) -> EntryCost {
    EntryCost {
        cpu_insns: cpu,
        mem_bytes: 50_000,
        read_entries: 5,
        write_entries: 2,
        read_bytes: 1_000,
        write_bytes: 400,
    }
}

fn sample_baseline(tolerance_pct: f64) -> Baseline {
    let mut costs = BTreeMap::new();
    for ep in ENTRYPOINTS {
        costs.insert((*ep).to_string(), sample_cost(100_000));
    }
    Baseline {
        tolerance_pct,
        costs,
    }
}

#[test]
fn test_b1_exact_tolerance_limit_does_not_regress() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    // 100_000 * 1.10 = 110_000: exact inclusive limit
    current.get_mut("create_bond").unwrap().cpu_insns = 110_000;
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(matches!(outcome, CostGateOutcome::Pass { .. }));
}

#[test]
fn test_b2_off_by_one_over_tolerance_triggers_regression() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = 110_001;
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].current, 110_001);
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b3_zero_baseline_with_positive_current_flags_100_pct_regression() {
    let mut baseline = sample_baseline(10.0);
    baseline.costs.get_mut("top_up").unwrap().read_bytes = 0;
    let mut current = baseline.costs.clone();
    current.get_mut("top_up").unwrap().read_bytes = 1;
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions[0].pct, 100.0);
        assert_eq!(regressions[0].metric, "read_bytes");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b4_zero_baseline_with_zero_current_passes() {
    let mut baseline = sample_baseline(10.0);
    baseline.costs.get_mut("top_up").unwrap().write_bytes = 0;
    let current = baseline.costs.clone();
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(matches!(outcome, CostGateOutcome::Pass { .. }));
}

#[test]
fn test_b5_zero_current_with_positive_baseline_is_improvement() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("withdraw").unwrap().cpu_insns = 0;
    let (_, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_b6_cost_improvement_negative_delta_never_regresses() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("withdraw_early").unwrap().cpu_insns = 50_000;
    let (_, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_b7_identical_cost_boundary_passes() {
    let baseline = sample_baseline(10.0);
    let current = baseline.costs.clone();
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(matches!(
        outcome,
        CostGateOutcome::Pass {
            checked_entrypoints: 6,
            ..
        }
    ));
}

#[test]
fn test_b8_zero_tolerance_boundary_exact_match_passes() {
    let baseline = sample_baseline(0.0);
    let current = baseline.costs.clone();
    let (_, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_b9_zero_tolerance_boundary_plus_one_fails() {
    let baseline = sample_baseline(0.0);
    let mut current = baseline.costs.clone();
    current.get_mut("slash_bond").unwrap().cpu_insns = 100_001;
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(matches!(outcome, CostGateOutcome::Regression { .. }));
}

#[test]
fn test_b10_high_tolerance_boundary_100_pct_exact_double_passes() {
    let baseline = sample_baseline(100.0);
    let mut current = baseline.costs.clone();
    current.get_mut("add_attestation").unwrap().cpu_insns = 200_000;
    let (_, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_b11_high_tolerance_boundary_100_pct_double_plus_one_fails() {
    let baseline = sample_baseline(100.0);
    let mut current = baseline.costs.clone();
    current.get_mut("add_attestation").unwrap().cpu_insns = 200_001;
    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(matches!(outcome, CostGateOutcome::Regression { .. }));
}

#[test]
fn test_b12_large_numeric_values_without_overflow() {
    let large_cpu = i64::MAX / 2;
    let mut baseline = sample_baseline(10.0);
    baseline.costs.get_mut("create_bond").unwrap().cpu_insns = large_cpu;

    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = large_cpu + 10;
    let (_, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn test_b13_metric_cpu_insns_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = 150_000;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "cpu_insns");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b14_metric_mem_bytes_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().mem_bytes = 60_000;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "mem_bytes");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b15_metric_read_entries_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().read_entries = 10;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "read_entries");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b16_metric_write_entries_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().write_entries = 5;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "write_entries");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b17_metric_read_bytes_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().read_bytes = 1_500;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "read_bytes");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b18_metric_write_bytes_independent_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().write_bytes = 600;
    let (outcome, _) = evaluate_cost_gate(&baseline, &current);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].metric, "write_bytes");
    } else {
        panic!("expected regression");
    }
}

#[test]
fn test_b19_all_six_metrics_simultaneous_trigger() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    let c = current.get_mut("create_bond").unwrap();
    c.cpu_insns = 200_000;
    c.mem_bytes = 100_000;
    c.read_entries = 10;
    c.write_entries = 5;
    c.read_bytes = 2_000;
    c.write_bytes = 800;

    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 6);
    } else {
        panic!("expected 6 regressions");
    }
}

#[test]
fn test_b20_multiple_entrypoints_simultaneous_regression() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = 150_000;
    current.get_mut("withdraw").unwrap().cpu_insns = 150_000;

    let (outcome, code) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code, ExitCode::FAILURE);
    if let CostGateOutcome::Regression { regressions, .. } = outcome {
        assert_eq!(regressions.len(), 2);
        assert!(regressions.iter().any(|r| r.entrypoint == "create_bond"));
        assert!(regressions.iter().any(|r| r.entrypoint == "withdraw"));
    } else {
        panic!("expected 2 regressions");
    }
}
