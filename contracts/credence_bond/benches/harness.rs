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
//! and triage them.

#![allow(dead_code)]

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
pub const TOLERANCE_PCT: f64 = 5.0;

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
///
/// Each entrypoint runs in its own env with the minimal setup required to reach
/// it, and the target call is always the *last* invocation before [`measure`] so
/// `cost_estimate().resources()` reports that call alone.
pub fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();

    // create_bond — the bare happy path: one identity bonds.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
        out.insert("create_bond".into(), measure(&env));
    }

    // top_up — adds to an existing bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
        client.top_up(&identity, &500_i128);
        out.insert("top_up".into(), measure(&env));
    }

    // withdraw — non-rolling bond, after the lock-up has elapsed.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let identity = Address::generate(&env);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
        env.ledger().set_timestamp(2_000);
        client.withdraw(&identity, &100_i128);
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
        client.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
        env.ledger().set_timestamp(100);
        client.withdraw_early(&identity, &100_i128);
        out.insert("withdraw_early".into(), measure(&env));
    }

    // slash_bond — admin slashes part of an active bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(CredenceBond, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &1_000_i128, &1_000_u64, &false, &0_u64);
        client.slash_bond(&admin, &100_i128);
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
        client.add_attestation(&attester, &subject, &data, &0_u64);
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
// Minimal JSON reader for the baseline file.
//
// The schema is fixed and self-produced (no arrays, no escape sequences), so a
// tiny hand-rolled parser keeps the harness dependency-free — adding serde to a
// `#![no_std]` cdylib crate risks breaking the wasm build.
// ---------------------------------------------------------------------------

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

    fn value(&mut self) -> Json {
        self.ws();
        match self.b[self.i] {
            b'{' => self.object(),
            b'"' => Json::Str(self.string()),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Json {
        self.i += 1; // consume '{'
        let mut members = Vec::new();
        loop {
            self.ws();
            if self.b[self.i] == b'}' {
                self.i += 1;
                break;
            }
            let key = self.string();
            self.ws();
            self.i += 1; // consume ':'
            let val = self.value();
            members.push((key, val));
            self.ws();
            if self.b[self.i] == b',' {
                self.i += 1;
            }
        }
        Json::Obj(members)
    }

    fn string(&mut self) -> String {
        self.ws();
        self.i += 1; // consume opening '"'
        let start = self.i;
        while self.b[self.i] != b'"' {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i])
            .unwrap()
            .to_string();
        self.i += 1; // consume closing '"'
        s
    }

    fn number(&mut self) -> Json {
        let start = self.i;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c == b'-' || c == b'+' || c == b'.' || c == b'e' || c == b'E' || c.is_ascii_digit() {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).unwrap();
        Json::Num(s.parse().unwrap())
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

/// A parsed baseline: the per-entrypoint costs plus the tolerance recorded with
/// the snapshot (so the gate uses the tolerance the baseline was written with).
pub struct Baseline {
    pub tolerance_pct: f64,
    pub costs: BTreeMap<String, EntryCost>,
}

/// Parse a baseline JSON document produced by [`to_json`].
pub fn parse_baseline(text: &str) -> Baseline {
    let root = Reader {
        b: text.as_bytes(),
        i: 0,
    }
    .value();
    let tolerance_pct = obj_get(&root, "tolerance_pct")
        .map(as_num)
        .unwrap_or(TOLERANCE_PCT);
    let mut costs = BTreeMap::new();
    if let Some(Json::Obj(entries)) = obj_get(&root, "entrypoints") {
        for (name, c) in entries {
            costs.insert(
                name.clone(),
                EntryCost {
                    cpu_insns: as_num(obj_get(c, "cpu_insns").unwrap()) as i64,
                    mem_bytes: as_num(obj_get(c, "mem_bytes").unwrap()) as i64,
                    read_entries: as_num(obj_get(c, "read_entries").unwrap()) as u32,
                    write_entries: as_num(obj_get(c, "write_entries").unwrap()) as u32,
                    read_bytes: as_num(obj_get(c, "read_bytes").unwrap()) as u32,
                    write_bytes: as_num(obj_get(c, "write_bytes").unwrap()) as u32,
                },
            );
        }
    }
    Baseline {
        tolerance_pct,
        costs,
    }
}

/// A single metric that grew past the tolerance.
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
