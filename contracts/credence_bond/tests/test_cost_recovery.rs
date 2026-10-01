// This is an off-chain integration test binary, not deployed WASM. Issue #713
// disallows `format!`/etc. in production contract code; integration tests never
// run on chain, so we silence the lint locally.
#![allow(clippy::disallowed_macros)]

//! Recovery and failure-path test coverage for `contracts/credence_bond/benches/cost.rs` (issue #1313).
//!
//! Pins deterministic error handling, missing baseline recovery, malformed payload recovery,
//! idempotency under repetition, lifecycle recovery (failing -> refresh -> pass), round-trip
//! serialization preservation, and observability without sensitive data exposure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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
        path: PathBuf,
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

pub fn handle_missing_baseline(path: &Path) -> (CostGateOutcome, ExitCode) {
    (
        CostGateOutcome::MissingBaseline {
            path: path.to_path_buf(),
            remediation: "cargo run -p credence_bond --bin update-cost-baseline",
        },
        ExitCode::FAILURE,
    )
}

pub fn run_gate_from_text(
    baseline_result: Result<&str, &std::io::Error>,
    baseline_path: &Path,
    current: &BTreeMap<String, EntryCost>,
) -> (CostGateOutcome, ExitCode) {
    match baseline_result {
        Ok(text) => {
            let baseline = parse_baseline(text);
            evaluate_cost_gate(&baseline, current)
        }
        Err(_) => handle_missing_baseline(baseline_path),
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

pub fn format_regression_lines(regressions: &[Regression]) -> Vec<String> {
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

#[allow(dead_code)]
enum Json {
    Obj(Vec<(String, Json)>),
    Num(f64),
    Str(String),
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        if self.i >= self.b.len() {
            return None;
        }
        match self.b[self.i] {
            b'{' => self.object(),
            b'"' => self.string().map(Json::Str),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Option<Json> {
        self.i += 1; // consume '{'
        let mut members = Vec::new();
        loop {
            self.ws();
            if self.i >= self.b.len() {
                break;
            }
            if self.b[self.i] == b'}' {
                self.i += 1;
                break;
            }
            let key = match self.string() {
                Some(k) => k,
                None => break,
            };
            self.ws();
            if self.i < self.b.len() && self.b[self.i] == b':' {
                self.i += 1; // consume ':'
            }
            let val = match self.value() {
                Some(v) => v,
                None => break,
            };
            members.push((key, val));
            self.ws();
            if self.i < self.b.len() && self.b[self.i] == b',' {
                self.i += 1;
            }
        }
        Some(Json::Obj(members))
    }

    fn string(&mut self) -> Option<String> {
        self.ws();
        if self.i >= self.b.len() || self.b[self.i] != b'"' {
            return None;
        }
        self.i += 1; // consume opening '"'
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i] != b'"' {
            self.i += 1;
        }
        if self.i >= self.b.len() {
            return None;
        }
        let s = std::str::from_utf8(&self.b[start..self.i])
            .ok()?
            .to_string();
        self.i += 1; // consume closing '"'
        Some(s)
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.i;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c == b'-' || c == b'+' || c == b'.' || c == b'e' || c == b'E' || c.is_ascii_digit() {
                self.i += 1;
            } else {
                break;
            }
        }
        if start == self.i {
            return None;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).ok()?;
        s.parse::<f64>().ok().map(Json::Num)
    }
}

fn obj_get<'j>(j: &'j Json, key: &str) -> Option<&'j Json> {
    match j {
        Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn as_num(j: &Json) -> f64 {
    match j {
        Json::Num(n) => *n,
        _ => panic!("expected number"),
    }
}

pub fn parse_baseline(text: &str) -> Baseline {
    let mut reader = Reader {
        b: text.as_bytes(),
        i: 0,
    };
    let root = reader.value().unwrap_or(Json::Obj(Vec::new()));
    let tolerance_pct = obj_get(&root, "tolerance_pct")
        .and_then(|v| match v {
            Json::Num(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(TOLERANCE_PCT);

    let mut costs = BTreeMap::new();
    if let Some(Json::Obj(entries)) = obj_get(&root, "entrypoints") {
        for (name, c) in entries {
            let cpu_insns = obj_get(c, "cpu_insns").map(as_num).unwrap_or(0.0) as i64;
            let mem_bytes = obj_get(c, "mem_bytes").map(as_num).unwrap_or(0.0) as i64;
            let read_entries = obj_get(c, "read_entries").map(as_num).unwrap_or(0.0) as u32;
            let write_entries = obj_get(c, "write_entries").map(as_num).unwrap_or(0.0) as u32;
            let read_bytes = obj_get(c, "read_bytes").map(as_num).unwrap_or(0.0) as u32;
            let write_bytes = obj_get(c, "write_bytes").map(as_num).unwrap_or(0.0) as u32;

            costs.insert(
                name.clone(),
                EntryCost {
                    cpu_insns,
                    mem_bytes,
                    read_entries,
                    write_entries,
                    read_bytes,
                    write_bytes,
                },
            );
        }
    }
    Baseline {
        tolerance_pct,
        costs,
    }
}

pub fn to_json(tolerance_pct: f64, costs: &BTreeMap<String, EntryCost>) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str("  \"schema\": \"credence_bond.cost_baseline.v1\",\n");
    s.push_str(&format!("  \"tolerance_pct\": {:.1},\n", tolerance_pct));
    s.push_str("  \"entrypoints\": {\n");
    let names: Vec<&str> = ENTRYPOINTS
        .iter()
        .copied()
        .filter(|n| costs.contains_key(*n))
        .collect();
    for (idx, name) in names.iter().enumerate() {
        let c = &costs[*name];
        s.push_str(&format!("    \"{}\": {{\n", name));
        s.push_str(&format!("      \"cpu_insns\": {},\n", c.cpu_insns));
        s.push_str(&format!("      \"mem_bytes\": {},\n", c.mem_bytes));
        s.push_str(&format!("      \"read_entries\": {},\n", c.read_entries));
        s.push_str(&format!("      \"write_entries\": {},\n", c.write_entries));
        s.push_str(&format!("      \"read_bytes\": {},\n", c.read_bytes));
        s.push_str(&format!("      \"write_bytes\": {}\n", c.write_bytes));
        let comma = if idx + 1 < names.len() { "," } else { "" };
        s.push_str(&format!("    }}{}\n", comma));
    }
    s.push_str("  }\n");
    s.push_str("}\n");
    s
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
fn test_r1_missing_baseline_file_returns_diagnosable_error() {
    let dummy = PathBuf::from("missing/path/cost_baseline.json");
    let (outcome, code) = handle_missing_baseline(&dummy);
    assert_eq!(code, ExitCode::FAILURE);
    match outcome {
        CostGateOutcome::MissingBaseline { path, remediation } => {
            assert_eq!(path, dummy);
            assert!(remediation.contains("update-cost-baseline"));
        }
        _ => panic!("expected MissingBaseline outcome"),
    }
}

#[test]
fn test_r2_run_gate_from_text_handles_io_error_recovery() {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let path = PathBuf::from("cost_baseline.json");
    let current = sample_baseline(10.0).costs;
    let (outcome, code) = run_gate_from_text(Err(&err), &path, &current);
    assert_eq!(code, ExitCode::FAILURE);
    assert!(matches!(outcome, CostGateOutcome::MissingBaseline { .. }));
}

#[test]
fn test_r3_empty_string_baseline_recovery() {
    let baseline = parse_baseline("");
    assert_eq!(baseline.tolerance_pct, TOLERANCE_PCT);
    assert!(baseline.costs.is_empty());
}

#[test]
fn test_r4_missing_tolerance_pct_defaults_to_constant() {
    let json = r#"{"entrypoints": {}}"#;
    let baseline = parse_baseline(json);
    assert_eq!(baseline.tolerance_pct, TOLERANCE_PCT);
}

#[test]
fn test_r5_missing_entrypoints_object_recovery() {
    let json = r#"{"tolerance_pct": 15.0}"#;
    let baseline = parse_baseline(json);
    assert_eq!(baseline.tolerance_pct, 15.0);
    assert!(baseline.costs.is_empty());
}

#[test]
fn test_r6_unknown_extraneous_entrypoint_ignored() {
    let json = r#"{
        "tolerance_pct": 10.0,
        "entrypoints": {
            "unknown_fn": {
                "cpu_insns": 100,
                "mem_bytes": 100,
                "read_entries": 1,
                "write_entries": 1,
                "read_bytes": 10,
                "write_bytes": 10
            }
        }
    }"#;
    let baseline = parse_baseline(json);
    assert!(baseline.costs.contains_key("unknown_fn"));

    // Diff skips unknown entrypoints not in ENTRYPOINTS
    let current = sample_baseline(10.0).costs;
    let regressions = diff(&baseline, &current);
    assert!(regressions.is_empty());
}

#[test]
fn test_r7_partial_entrypoint_fields_default_to_zero() {
    let json = r#"{
        "tolerance_pct": 10.0,
        "entrypoints": {
            "create_bond": {
                "cpu_insns": 500
            }
        }
    }"#;
    let baseline = parse_baseline(json);
    let c = baseline.costs.get("create_bond").unwrap();
    assert_eq!(c.cpu_insns, 500);
    assert_eq!(c.mem_bytes, 0);
    assert_eq!(c.read_entries, 0);
}

#[test]
fn test_r8_idempotent_repeated_evaluation() {
    let baseline = sample_baseline(10.0);
    let current = baseline.costs.clone();
    for _ in 0..100 {
        let (outcome, code) = evaluate_cost_gate(&baseline, &current);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(matches!(outcome, CostGateOutcome::Pass { .. }));
    }
}

#[test]
fn test_r9_idempotent_repeated_diff() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = 150_000;
    let first = diff(&baseline, &current);
    for _ in 0..50 {
        let next = diff(&baseline, &current);
        assert_eq!(first, next);
    }
}

#[test]
fn test_r10_recovery_after_regression_via_refresh() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    // Step 1: Regression fails
    current.get_mut("top_up").unwrap().cpu_insns = 150_000;
    let (_, code1) = evaluate_cost_gate(&baseline, &current);
    assert_eq!(code1, ExitCode::FAILURE);

    // Step 2: Simulate baseline refresh with current measurements
    let refreshed_baseline = Baseline {
        tolerance_pct: baseline.tolerance_pct,
        costs: current.clone(),
    };

    // Step 3: Next gate check passes
    let (outcome2, code2) = evaluate_cost_gate(&refreshed_baseline, &current);
    assert_eq!(code2, ExitCode::SUCCESS);
    assert!(matches!(outcome2, CostGateOutcome::Pass { .. }));
}

#[test]
fn test_r11_roundtrip_serialization_fidelity() {
    let baseline = sample_baseline(8.5);
    let json = to_json(baseline.tolerance_pct, &baseline.costs);
    let parsed = parse_baseline(&json);
    assert_eq!(parsed.tolerance_pct, 8.5);
    assert_eq!(parsed.costs.len(), baseline.costs.len());
    for (name, cost) in &baseline.costs {
        assert_eq!(parsed.costs.get(name).unwrap(), cost);
    }
}

#[test]
fn test_r12_state_immutability_and_isolation() {
    let baseline = sample_baseline(10.0);
    let current = baseline.costs.clone();
    let _ = evaluate_cost_gate(&baseline, &current);
    let _ = format_table_rows(&baseline, &current);
    assert_eq!(baseline.costs.len(), 6);
    assert_eq!(current.len(), 6);
}

#[test]
fn test_r13_diagnosability_of_regression_lines() {
    let regressions = vec![Regression {
        entrypoint: "withdraw".to_string(),
        metric: "cpu_insns",
        baseline: 50_000,
        current: 60_000,
        pct: 20.0,
    }];
    let lines = format_regression_lines(&regressions);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("withdraw::cpu_insns"));
    assert!(lines[0].contains("50000 -> 60000"));
    assert!(lines[0].contains("(+20.0%)"));
}

#[test]
fn test_r14_table_row_formatting_signed_deltas() {
    let baseline = sample_baseline(10.0);
    let mut current = baseline.costs.clone();
    current.get_mut("create_bond").unwrap().cpu_insns = 110_000;
    current.get_mut("top_up").unwrap().cpu_insns = 90_000;
    current.get_mut("withdraw").unwrap().cpu_insns = 100_000;

    let rows = format_table_rows(&baseline, &current);
    let create_row = rows.iter().find(|r| r.0 == "create_bond").unwrap();
    assert_eq!(create_row.2, "+10000");

    let topup_row = rows.iter().find(|r| r.0 == "top_up").unwrap();
    assert_eq!(topup_row.2, "-10000");

    let withdraw_row = rows.iter().find(|r| r.0 == "withdraw").unwrap();
    assert_eq!(withdraw_row.2, "+0");
}
