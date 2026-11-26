#![allow(clippy::disallowed_macros)]

//! Reusable cost-measurement and regression-gating harness for Soroban contracts.
//!
//! Drives contract entrypoints through a real Soroban test [`Env`], reads modelled
//! host resource costs via [`Env::cost_estimate`], serializes/parses baselines in
//! the `credence_bond.cost_baseline.v1` JSON schema, and detects regressions
//! exceeding a configurable percentage tolerance.

extern crate std;

use std::collections::BTreeMap;
use std::format;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::string::{String, ToString};
use std::vec::Vec;

use soroban_sdk::{testutils::EnvTestConfig, Env};

/// Canonical baseline JSON schema identifier shared across all contracts.
pub const SCHEMA_VERSION: &str = "credence_bond.cost_baseline.v1";

/// Default percentage a metric may grow over its baseline before it counts as a
/// regression.
pub const TOLERANCE_PCT: f64 = 10.0;

/// Metered resources for a single top-level entrypoint invocation. Field names
/// mirror `soroban_env_host::InvocationResources` so the JSON baseline reads the
/// same as the host's own accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryCost {
    /// Modelled CPU instructions consumed by the invocation.
    pub cpu_insns: i64,
    /// Modelled linear-memory high-water mark, in bytes.
    pub mem_bytes: i64,
    /// Ledger entries read (the storage "read units" half of rw-units).
    pub read_entries: u32,
    /// Ledger entries written (the storage "write units" half of rw-units).
    pub write_entries: u32,
    /// Bytes read across all entries.
    pub read_bytes: u32,
    /// Bytes written across all entries.
    pub write_bytes: u32,
}

/// Build a fresh metered test env. Snapshot capture is disabled so repeated
/// measurement runs do not litter the working tree with `*.json` snapshots.
pub fn fresh_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env
}

/// Read the resources metered for the most recent top-level invocation. Must be
/// called immediately after the entrypoint under measurement.
pub fn measure(env: &Env) -> EntryCost {
    let r = env.cost_estimate().resources();
    EntryCost {
        cpu_insns: r.instructions,
        mem_bytes: r.mem_bytes,
        read_entries: r.read_entries,
        write_entries: r.write_entries,
        read_bytes: r.read_bytes,
        write_bytes: r.write_bytes,
    }
}

/// Serialize a baseline snapshot to deterministic, pretty-printed JSON using
/// the canonical `credence_bond.cost_baseline.v1` schema.
pub fn to_json(
    entrypoints: &[&str],
    costs: &BTreeMap<String, EntryCost>,
    tolerance_pct: f64,
) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"schema\": \"{}\",\n", SCHEMA_VERSION));
    s.push_str(&format!("  \"tolerance_pct\": {:.1},\n", tolerance_pct));
    s.push_str("  \"metric_help\": \"cpu_insns and mem_bytes are modelled host costs; read/write_entries and read/write_bytes are the ledger rw-unit footprint. See docs/gas-benchmark-notes.md.\",\n");
    s.push_str("  \"entrypoints\": {\n");
    let names: Vec<&str> = entrypoints
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

// ---------------------------------------------------------------------------
// Resilient JSON reader for the baseline file.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
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

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        if self.i >= self.b.len() {
            return Err("unexpected end of input".to_string());
        }
        match self.b[self.i] {
            b'{' => self.object(),
            b'"' => self.string().map(Json::Str),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        if self.i >= self.b.len() || self.b[self.i] != b'{' {
            return Err("expected '{'".to_string());
        }
        self.i += 1;
        let mut members = Vec::new();
        loop {
            self.ws();
            if self.i >= self.b.len() {
                return Err("unterminated object: expected '}'".to_string());
            }
            if self.b[self.i] == b'}' {
                self.i += 1;
                break;
            }
            let key = self.string()?;
            self.ws();
            if self.i >= self.b.len() || self.b[self.i] != b':' {
                return Err("expected ':' after object key".to_string());
            }
            self.i += 1;
            let val = self.value()?;
            members.push((key, val));
            self.ws();
            if self.i < self.b.len() && self.b[self.i] == b',' {
                self.i += 1;
            }
        }
        Ok(Json::Obj(members))
    }

    fn string(&mut self) -> Result<String, String> {
        self.ws();
        if self.i >= self.b.len() || self.b[self.i] != b'"' {
            return Err("expected string starting with '\"'".to_string());
        }
        self.i += 1;
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i] != b'"' {
            self.i += 1;
        }
        if self.i >= self.b.len() {
            return Err("unterminated string literal".to_string());
        }
        let s = core::str::from_utf8(&self.b[start..self.i])
            .map_err(|e| format!("invalid utf-8 in string: {e}"))?
            .to_string();
        self.i += 1;
        Ok(s)
    }

    fn number(&mut self) -> Result<Json, String> {
        self.ws();
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
            return Err(format!("expected number at byte {}", self.i));
        }
        let s = core::str::from_utf8(&self.b[start..self.i])
            .map_err(|e| format!("invalid utf-8 in number: {e}"))?;
        let num: f64 = s
            .parse()
            .map_err(|e| format!("failed to parse number '{s}': {e}"))?;
        Ok(Json::Num(num))
    }
}

fn obj_get<'j>(j: &'j Json, key: &str) -> Option<&'j Json> {
    match j {
        Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn as_num_opt(j: &Json) -> Option<f64> {
    match j {
        Json::Num(n) => Some(*n),
        _ => None,
    }
}

fn get_metric_i64(c: &Json, ep: &str, metric: &'static str) -> Result<i64, BaselineError> {
    let val_json = obj_get(c, metric).ok_or_else(|| BaselineError::InvalidEntrypointMetric {
        entrypoint: ep.to_string(),
        metric,
        reason: "metric field is missing".to_string(),
    })?;
    let num = as_num_opt(val_json).ok_or_else(|| BaselineError::InvalidEntrypointMetric {
        entrypoint: ep.to_string(),
        metric,
        reason: "expected numeric value".to_string(),
    })?;
    Ok(num as i64)
}

fn get_metric_u32(c: &Json, ep: &str, metric: &'static str) -> Result<u32, BaselineError> {
    let num = get_metric_i64(c, ep, metric)?;
    if num < 0 {
        return Err(BaselineError::InvalidEntrypointMetric {
            entrypoint: ep.to_string(),
            metric,
            reason: format!("negative count {num} not allowed for {metric}"),
        });
    }
    Ok(num as u32)
}

/// Baseline parsing error details for failure recovery and diagnosis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaselineError {
    EmptyText,
    MalformedJson(String),
    MissingEntrypoints,
    InvalidEntrypointMetric {
        entrypoint: String,
        metric: &'static str,
        reason: String,
    },
}

impl core::fmt::Display for BaselineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BaselineError::EmptyText => write!(f, "baseline text is empty"),
            BaselineError::MalformedJson(e) => write!(f, "malformed baseline JSON: {e}"),
            BaselineError::MissingEntrypoints => {
                write!(f, "missing 'entrypoints' object in baseline JSON")
            }
            BaselineError::InvalidEntrypointMetric {
                entrypoint,
                metric,
                reason,
            } => {
                write!(
                    f,
                    "entrypoint '{entrypoint}' metric '{metric}' invalid: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for BaselineError {}

/// A parsed baseline: the per-entrypoint costs plus the tolerance recorded with
/// the snapshot (so the gate uses the tolerance the baseline was written with).
#[derive(Clone, Debug, PartialEq)]
pub struct Baseline {
    pub tolerance_pct: f64,
    pub costs: BTreeMap<String, EntryCost>,
}

/// Parse a baseline JSON document with structured error recovery.
pub fn try_parse_baseline(text: &str) -> Result<Baseline, BaselineError> {
    if text.trim().is_empty() {
        return Err(BaselineError::EmptyText);
    }
    let mut reader = Reader {
        b: text.as_bytes(),
        i: 0,
    };
    let root = reader.value().map_err(BaselineError::MalformedJson)?;
    let tolerance_pct = obj_get(&root, "tolerance_pct")
        .and_then(as_num_opt)
        .unwrap_or(TOLERANCE_PCT);

    let Some(Json::Obj(entries)) = obj_get(&root, "entrypoints") else {
        return Err(BaselineError::MissingEntrypoints);
    };

    let mut costs = BTreeMap::new();
    for (name, c) in entries {
        let cpu_insns = get_metric_i64(c, name, "cpu_insns")?;
        let mem_bytes = get_metric_i64(c, name, "mem_bytes")?;
        let read_entries = get_metric_u32(c, name, "read_entries")?;
        let write_entries = get_metric_u32(c, name, "write_entries")?;
        let read_bytes = get_metric_u32(c, name, "read_bytes")?;
        let write_bytes = get_metric_u32(c, name, "write_bytes")?;

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

    Ok(Baseline {
        tolerance_pct,
        costs,
    })
}

/// Parse a baseline JSON document produced by [`to_json`]. Recovers with an
/// empty baseline and logs a warning if parsing fails.
pub fn parse_baseline(text: &str) -> Baseline {
    match try_parse_baseline(text) {
        Ok(b) => b,
        Err(e) => {
            std::eprintln!("warning: baseline fallback: {e}");
            Baseline {
                tolerance_pct: TOLERANCE_PCT,
                costs: BTreeMap::new(),
            }
        }
    }
}

/// A single metric that grew past the tolerance.
#[derive(Clone, Debug, PartialEq)]
pub struct Regression {
    pub entrypoint: String,
    pub metric: &'static str,
    pub baseline: i64,
    pub current: i64,
    pub pct: f64,
}

/// Compare a fresh measurement against the baseline and return every metric that
/// regressed by more than `baseline.tolerance_pct`.
pub fn diff(
    entrypoints: &[&str],
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> Vec<Regression> {
    let mut regressions = Vec::new();
    let factor = 1.0 + baseline.tolerance_pct / 100.0;
    for name in entrypoints {
        let (Some(b), Some(c)) = (baseline.costs.get(*name), current.get(*name)) else {
            continue;
        };
        let metrics: [(&'static str, i64, i64); 6] = [
            ("cpu_insns", b.cpu_insns, c.cpu_insns),
            ("mem_bytes", b.mem_bytes, c.mem_bytes),
            ("read_entries", b.read_entries as i64, c.read_entries as i64),
            (
                "write_entries",
                b.write_entries as i64,
                c.write_entries as i64,
            ),
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
        regressions: Vec<Regression>,
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
pub fn evaluate_cost_gate(
    entrypoints: &[&str],
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> (CostGateOutcome, ExitCode) {
    let regressions = diff(entrypoints, baseline, current);
    let checked = entrypoints
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

/// Format the baseline-vs-current comparison rows for display or verification.
pub fn format_table_rows(
    entrypoints: &[&str],
    baseline: &Baseline,
    current: &BTreeMap<String, EntryCost>,
) -> Vec<TableRow> {
    let mut rows = Vec::new();
    for name in entrypoints {
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

/// Assert that `current` costs do not regress beyond `baseline.tolerance_pct`
/// compared to `<manifest_dir>/cost_baseline.json`.
///
/// If `UPDATE_COST_BASELINE=1` is set in the environment, overwrites
/// `<manifest_dir>/cost_baseline.json` with `current` instead of checking.
pub fn assert_cost_baseline(
    manifest_dir: &str,
    entrypoints: &[&str],
    current: &BTreeMap<String, EntryCost>,
    update_cmd: &str,
) {
    let baseline_path = Path::new(manifest_dir).join("cost_baseline.json");

    if std::env::var("UPDATE_COST_BASELINE").ok().as_deref() == Some("1") {
        let json = to_json(entrypoints, current, TOLERANCE_PCT);
        std::fs::write(&baseline_path, json).expect("failed to write cost_baseline.json");
        std::println!("updated cost baseline -> {}", baseline_path.display());
        return;
    }

    let baseline_text = std::fs::read_to_string(&baseline_path).unwrap_or_else(|_| {
        panic!(
            "cost_baseline.json not found at {}; generate it with `{}`",
            baseline_path.display(),
            update_cmd
        )
    });
    let baseline = try_parse_baseline(&baseline_text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", baseline_path.display()));

    for name in entrypoints {
        assert!(
            baseline.costs.contains_key(*name),
            "entrypoint `{name}` missing from {}; refresh with `{update_cmd}`",
            baseline_path.display()
        );
        assert!(
            current.contains_key(*name),
            "entrypoint `{name}` was not measured by the test harness"
        );
    }

    let regressions = diff(entrypoints, &baseline, current);

    std::println!(
        "\n{:<22} {:>12} {:>12} {:>8} {:>8}",
        "entrypoint",
        "cpu_insns",
        "mem_bytes",
        "rw(r/w)",
        "bytes(r/w)"
    );
    for name in entrypoints {
        if let Some(c) = current.get(*name) {
            std::println!(
                "{:<22} {:>12} {:>12} {:>3}/{:<4} {:>4}/{:<4}",
                name,
                c.cpu_insns,
                c.mem_bytes,
                c.read_entries,
                c.write_entries,
                c.read_bytes,
                c.write_bytes
            );
        }
    }

    if !regressions.is_empty() {
        let mut msg = format!(
            "\nCost regression(s) exceeded {:.1}% tolerance:\n",
            baseline.tolerance_pct
        );
        for line in format_regression_lines(&regressions) {
            msg.push_str(&line);
            msg.push('\n');
        }
        msg.push_str(&format!(
            "\nIf this change is intended, refresh the baseline:\n  {update_cmd}\n"
        ));
        panic!("{}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cost(cpu: i64) -> EntryCost {
        EntryCost {
            cpu_insns: cpu,
            mem_bytes: 50_000,
            read_entries: 3,
            write_entries: 2,
            read_bytes: 1_000,
            write_bytes: 500,
        }
    }

    #[test]
    fn round_trip_json_and_diff_tolerance() {
        let eps = &["op_a", "op_b"];
        let mut costs = BTreeMap::new();
        costs.insert("op_a".to_string(), sample_cost(100_000));
        costs.insert("op_b".to_string(), sample_cost(200_000));

        let json = to_json(eps, &costs, TOLERANCE_PCT);
        assert!(json.contains(SCHEMA_VERSION));

        let parsed = try_parse_baseline(&json).expect("valid baseline JSON");
        assert_eq!(parsed.tolerance_pct, TOLERANCE_PCT);
        assert_eq!(parsed.costs, costs);

        // Exact 10% limit passes
        let mut at_limit = costs.clone();
        at_limit.get_mut("op_a").unwrap().cpu_insns = 110_000;
        assert!(diff(eps, &parsed, &at_limit).is_empty());

        // One instruction above 10% fails
        let mut over_limit = costs;
        over_limit.get_mut("op_a").unwrap().cpu_insns = 110_001;
        let regressions = diff(eps, &parsed, &over_limit);
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].entrypoint, "op_a");
        assert_eq!(regressions[0].metric, "cpu_insns");
    }

    #[test]
    fn parse_baseline_reports_errors() {
        assert_eq!(try_parse_baseline(""), Err(BaselineError::EmptyText));
        assert_eq!(
            try_parse_baseline("{\"tolerance_pct\": 10.0}"),
            Err(BaselineError::MissingEntrypoints)
        );
    }
}
