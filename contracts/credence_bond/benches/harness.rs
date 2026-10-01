//! Shared cost-measurement harness for the `credence_bond` contract.
//!
//! This module is compiled into both the `cost` bench ([cost.rs]) and the
//! `update-cost-baseline` binary ([update_cost_baseline.rs]). It drives every
//! tracked entrypoint through a real Soroban test [`Env`] and reads the modelled
//! resource cost via [`Env::cost_estimate`].
//!
//! The numbers are *modelled* host costs, not wall-clock time, so they are
//! deterministic across machines — which is exactly what makes them usable as a
//! committed regression baseline. See `docs/gas-regression.md` for how to read
//! and triage them.#![allow(dead_code)]
#![allow(clippy::disallowed_macros)]

use std::collections::BTreeMap;

use credence_bond::{CredenceBond, CredenceBondClient};
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger as _},
    Address, Env, String as SorobanString,
};

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

/// The entrypoints we gate, in a stable, deterministic order. Adding an
/// entrypoint here makes it part of the baseline on the next refresh.
pub const ENTRYPOINTS: &[&str] = &[
    "create_bond",
    "top_up",
    "withdraw",
    "withdraw_early",
    "slash_bond",
    "add_attestation",
];

/// Percentage a metric may grow over its baseline before it counts as a
/// regression. Kept in lock-step with the `tolerance_pct` written into the JSON.
pub const TOLERANCE_PCT: f64 = 10.0;

/// Build a fresh metered test env. Snapshot capture is disabled so repeated
/// measurement runs do not litter the working tree with `*.json` snapshots.
fn fresh_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env
}

/// Read the resources metered for the most recent top-level invocation. Must be
/// called immediately after the entrypoint under measurement.
fn measure(env: &Env) -> EntryCost {
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

/// Drive every tracked entrypoint and return its cost, keyed by name.
pub fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();

    // Use realistic bond amounts (minimum is 1e18)
    let bond_amount = 1_000_000_000_000_000_000i128; // 1e18
    let duration = 1_000_u64;

    // create_bond — the bare happy path: one identity bonds.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        out.insert("create_bond".into(), measure(&env));
    }

    // top_up — adds to an existing bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        client.top_up(&identity, &(bond_amount / 2));
        out.insert("top_up".into(), measure(&env));
    }

    // withdraw — non-rolling bond, after the lock-up has elapsed.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(2_000);
        client.withdraw(&identity, &(bond_amount / 10));
        out.insert("withdraw".into(), measure(&env));
    }

    // withdraw_early — bond exited before lock-up end, charging the penalty.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.set_early_exit_config(&admin, &treasury, &500_u32);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(100);
        client.withdraw_early(&identity, &(bond_amount / 10));
        out.insert("withdraw_early".into(), measure(&env));
    }

    // slash_bond — admin slashes part of an active bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        let salt = soroban_sdk::Bytes::new(&env);
        client.slash_bond(&admin, &identity, &(bond_amount / 10), &salt);
        out.insert("slash_bond".into(), measure(&env));
    }

    // add_attestation — a registered attester attests to a subject.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let attester = Address::generate(&env);
        let subject = Address::generate(&env);
        client.initialize(&admin, &None);
        client.register_attester(&attester);
        let data = SorobanString::from_str(&env, "kyc:passed");
        client.add_attestation(&attester, &subject, &data, &client.address, &0_u64, &0_u64);
        out.insert("add_attestation".into(), measure(&env));
    }

    out
}

/// Serialize a baseline snapshot to deterministic, pretty-printed JSON.
pub fn to_json(costs: &BTreeMap<String, EntryCost>) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str("  \"schema\": \"credence_bond.cost_baseline.v1\",\n");
    s.push_str(&format!("  \"tolerance_pct\": {:.1},\n", TOLERANCE_PCT));
    s.push_str("  \"metric_help\": \"cpu_insns and mem_bytes are modelled host costs; read/write_entries and read/write_bytes are the ledger rw-unit footprint. See docs/gas-regression.md.\",\n");
    s.push_str("  \"entrypoints\": {\n");
    // Emit in the canonical ENTRYPOINTS order for stable diffs.
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
        self.i += 1; // consume '{'
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
            self.i += 1; // consume ':'
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
        self.i += 1; // consume opening '"'
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i] != b'"' {
            self.i += 1;
        }
        if self.i >= self.b.len() {
            return Err("unterminated string literal".to_string());
        }
        let s = std::str::from_utf8(&self.b[start..self.i])
            .map_err(|e| format!("invalid utf-8 in string: {e}"))?
            .to_string();
        self.i += 1; // consume closing '"'
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
        let s = std::str::from_utf8(&self.b[start..self.i])
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

impl std::fmt::Display for BaselineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            eprintln!("warning: baseline fallback: {e}");
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
/// regressed by more than `tolerance_pct`. Only growth is flagged; improvements
/// (and new entrypoints absent from the baseline) are reported separately by the
/// caller.
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
            (
                "write_entries",
                b.write_entries as i64,
                c.write_entries as i64,
            ),
            ("read_bytes", b.read_bytes as i64, c.read_bytes as i64),
            ("write_bytes", b.write_bytes as i64, c.write_bytes as i64),
        ];
        for (metric, base, cur) in metrics {
            // A metric regresses when it exceeds baseline * (1 + tolerance).
            // Guard the zero-baseline case where any growth is a regression.
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
