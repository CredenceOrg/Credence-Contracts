//! Refresh the committed gas baseline for the `credence_bond` contract.
//!
//! # When to run
//! Run this **only when a cost change is intended and reviewed** — e.g. you
//! deliberately added a storage write or a new code path and the new numbers are
//! the new normal. A baseline refresh is a reviewable diff: the JSON change lands
//! in the same PR as the code change so reviewers see the cost delta.
//!
//! # How to refresh
//! ```sh
//! cargo run -p credence_bond --bin update-cost-baseline
//! ```
//! This overwrites `contracts/credence_bond/cost_baseline.json` with freshly
//! measured numbers. Commit the result. To write somewhere else (the CI gate
//! uses this to produce the "current" snapshot without touching the baseline):
//! ```sh
//! cargo run -p credence_bond --bin update-cost-baseline -- --out /tmp/cost_current.json
//! ```
//!
//! # Do NOT
//! Do not refresh the baseline to silence a CI failure you did not expect — an
//! unexpected jump is the signal the gate exists to catch. See
//! `docs/gas-regression.md` for triage steps.

mod harness;

use std::path::PathBuf;

fn main() {
    // Default target is the in-repo baseline; `--out <path>` overrides it.
    let mut out = default_baseline_path();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" | "-o" => {
                out = PathBuf::from(args.next().expect("--out requires a path argument"));
            }
            other => panic!("unknown argument: {other}"),
        }
    }

    let costs = harness::measure_all();
    let json = harness::to_json(&costs);
    std::fs::write(&out, json).expect("failed to write cost baseline");
    println!("wrote cost baseline -> {}", out.display());
}

/// Resolve `cost_baseline.json` next to this crate's `Cargo.toml`, independent of
/// the directory the binary is launched from.
fn default_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json")
}
