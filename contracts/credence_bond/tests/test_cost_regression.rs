//! Regression test for storage cost baselines.
//!
//! This test measures the cost (read/write entries and bytes) of hot-path
//! entrypoints and asserts that no regression exceeds the committed tolerance
//! (5%). The baseline is embedded in `cost_baseline.json` and refreshed when
//! intentional changes increase costs.
//!
//! Runs as part of `cargo test` and on every CI matrix entry.

use credence_bond::CredenceBondClient;
use std::collections::BTreeMap;
use std::path::PathBuf;
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger as _},
    Address, Env, String as SorobanString,
};

/// Metered resources for a single top-level entrypoint invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryCost {
    pub cpu_insns: i64,
    pub mem_bytes: i64,
    pub read_entries: u32,
    pub write_entries: u32,
    pub read_bytes: u32,
    pub write_bytes: u32,
}

const TOLERANCE_PCT: f64 = 5.0;

const ENTRYPOINTS: &[&str] = &[
    "create_bond",
    "top_up",
    "withdraw",
    "withdraw_early",
    "slash_bond",
    "add_attestation",
];

/// Build a fresh metered test env.
fn fresh_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env
}

/// Read the resources metered for the most recent top-level invocation.
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
fn measure_all() -> BTreeMap<String, EntryCost> {
    let mut out = BTreeMap::new();
    
    // Use realistic bond amounts (minimum is 1e18)
    let bond_amount = 1_000_000_000_000_000_000i128; // 1e18
    let duration = 1_000_u64;

    // create_bond — the bare happy path: one identity bonds.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        out.insert("create_bond".into(), measure(&env));
    }

    // top_up — adds to an existing bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
        let identity = Address::generate(&env);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        client.top_up(&(bond_amount / 2));
        out.insert("top_up".into(), measure(&env));
    }

    // withdraw — non-rolling bond, after the lock-up has elapsed.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
        let identity = Address::generate(&env);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(2_000);
        client.withdraw(&(bond_amount / 10));
        out.insert("withdraw".into(), measure(&env));
    }

    // withdraw_early — bond exited before lock-up end, charging the penalty.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.set_early_exit_config(&admin, &treasury, &500_u32);
        env.ledger().set_timestamp(0);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        env.ledger().set_timestamp(100);
        client.withdraw_early(&(bond_amount / 10));
        out.insert("withdraw_early".into(), measure(&env));
    }

    // slash_bond — admin slashes part of an active bond.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
        let admin = Address::generate(&env);
        let identity = Address::generate(&env);
        client.initialize(&admin, &None);
        client.create_bond(&identity, &bond_amount, &duration, &false, &0_u64);
        client.slash_bond(&admin, &(bond_amount / 10));
        out.insert("slash_bond".into(), measure(&env));
    }

    // add_attestation — a registered attester attests to a subject.
    {
        let env = fresh_env();
        let client = CredenceBondClient::new(&env, &env.register(credence_bond::CredenceBond, ()));
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

/// Parse baseline JSON (minimal inline parser to avoid dependencies).
fn parse_baseline(text: &str) -> BTreeMap<String, EntryCost> {
    let mut costs = BTreeMap::new();
    
    // Simple JSON parser for the known structure
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut current_entrypoint: Option<String> = None;
    let mut current_cost = EntryCost {
        cpu_insns: 0,
        mem_bytes: 0,
        read_entries: 0,
        write_entries: 0,
        read_bytes: 0,
        write_bytes: 0,
    };
    
    while i < lines.len() {
        let line = lines[i].trim();
        
        // Detect entrypoint name: "\"name\": {
        if line.starts_with('"') && line.contains("\": {") {
            if let Some(prev_ep) = current_entrypoint.take() {
                costs.insert(prev_ep, current_cost);
                current_cost = EntryCost {
                    cpu_insns: 0,
                    mem_bytes: 0,
                    read_entries: 0,
                    write_entries: 0,
                    read_bytes: 0,
                    write_bytes: 0,
                };
            }
            let name_end = line[1..].find('"').unwrap_or(0);
            current_entrypoint = Some(line[1..name_end + 1].to_string());
        }
        
        // Parse metrics: "key": value,
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().trim_matches('"');
            let val_part = line[colon_pos + 1..].trim();
            let val_str = val_part.trim_end_matches(',').trim();
            
            if let Ok(val) = val_str.parse::<i64>() {
                if current_entrypoint.is_some() {
                    match key {
                        "cpu_insns" => current_cost.cpu_insns = val,
                        "mem_bytes" => current_cost.mem_bytes = val,
                        "read_entries" => current_cost.read_entries = val as u32,
                        "write_entries" => current_cost.write_entries = val as u32,
                        "read_bytes" => current_cost.read_bytes = val as u32,
                        "write_bytes" => current_cost.write_bytes = val as u32,
                        _ => {}
                    }
                }
            }
        }
        
        i += 1;
    }
    
    // Don't forget the last entrypoint
    if let Some(ep) = current_entrypoint {
        costs.insert(ep, current_cost);
    }
    
    costs
}

/// A single metric that exceeded tolerance.
#[derive(Debug)]
struct Regression {
    entrypoint: String,
    metric: &'static str,
    baseline: i64,
    current: i64,
    pct: f64,
}

/// Compare current measurements against baseline and flag regressions.
fn detect_regressions(
    baseline: &BTreeMap<String, EntryCost>,
    current: &BTreeMap<String, EntryCost>,
) -> Vec<Regression> {
    let mut regressions = Vec::new();
    let factor = 1.0 + TOLERANCE_PCT / 100.0;
    
    for name in ENTRYPOINTS {
        let (Some(b), Some(c)) = (baseline.get(*name), current.get(*name)) else {
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

#[test]
fn test_storage_cost_no_regression() {
    // Load baseline from the committed JSON file
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json");
    let baseline_text = std::fs::read_to_string(&baseline_path).expect(
        "cost_baseline.json not found; run `cargo run -p credence_bond --features gas-bench --bin update-cost-baseline`",
    );
    let baseline = parse_baseline(&baseline_text);
    
    // Measure current costs
    let current = measure_all();
    
    // Detect regressions
    let regressions = detect_regressions(&baseline, &current);
    
    // Print summary table for visibility
    println!("\n{:<16} {:>12} {:>12} {:>10} {:>10}", "entrypoint", "read_e", "write_e", "read_b", "write_b");
    for name in ENTRYPOINTS {
        if let Some(c) = current.get(*name) {
            println!(
                "{:<16} {:>12} {:>12} {:>10} {:>10}",
                name, c.read_entries, c.write_entries, c.read_bytes, c.write_bytes
            );
        }
    }
    
    // Assert no regressions
    if !regressions.is_empty() {
        let mut msg = format!(
            "\n❌ Storage cost regression(s) exceeded {}% tolerance:\n",
            TOLERANCE_PCT
        );
        for r in &regressions {
            msg.push_str(&format!(
                "  {}::{}\n    baseline: {}, current: {} (+{:.1}%)\n",
                r.entrypoint, r.metric, r.baseline, r.current, r.pct
            ));
        }
        msg.push_str("\nTo update baseline (if intentional):\n");
        msg.push_str("  cargo run -p credence_bond --features gas-bench --bin update-cost-baseline\n");
        panic!("{}", msg);
    }
}
