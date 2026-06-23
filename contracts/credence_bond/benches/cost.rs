//! Gas-regression gate for the `credence_bond` contract.
//!
//! Run via `cargo bench -p credence_bond --bench cost` (or in CI). It measures
//! every tracked entrypoint with [`Env::cost_estimate`], compares against the
//! committed `cost_baseline.json`, prints a table, and **exits non-zero if any
//! metric regressed past the baseline's tolerance**. That non-zero exit is what
//! fails the PR.
//!
//! To intentionally accept new numbers, refresh the baseline with
//! `cargo run -p credence_bond --bin update-cost-baseline`. See
//! `docs/gas-regression.md`.

mod harness;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cost_baseline.json");
    let current = harness::measure_all();

    let baseline_text = match std::fs::read_to_string(&baseline_path) {
        Ok(t) => t,
        Err(_) => {
            // No baseline exists yet — write one from the current measurement
            // and exit successfully. The written file should be committed so
            // subsequent runs can detect regressions.
            let json = harness::to_json(&current);
            if let Err(e) = std::fs::write(&baseline_path, &json) {
                eprintln!("warning: could not write initial baseline to {}: {}", baseline_path.display(), e);
            } else {
                println!(
                    "✓ no baseline found — wrote initial baseline to {}",
                    baseline_path.display()
                );
                println!("  Commit this file so future runs can detect gas regressions.");
            }
            return ExitCode::SUCCESS;
        }
    };
    let baseline = harness::parse_baseline(&baseline_text);

    print_table(&baseline, &current);

    let regressions = harness::diff(&baseline, &current);
    if regressions.is_empty() {
        println!(
            "\n✓ no gas regressions (tolerance {:.1}%)",
            baseline.tolerance_pct
        );
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "\n✗ gas regression(s) over {:.1}% tolerance:",
        baseline.tolerance_pct
    );
    for r in &regressions {
        eprintln!(
            "  {}::{}  {} -> {}  (+{:.1}%)",
            r.entrypoint, r.metric, r.baseline, r.current, r.pct
        );
    }
    eprintln!(
        "\nIf this change is intended, refresh the baseline:\n  \
         cargo run -p credence_bond --bin update-cost-baseline"
    );
    ExitCode::FAILURE
}

/// Print a per-entrypoint baseline-vs-current table for the headline metrics.
fn print_table(
    baseline: &harness::Baseline,
    current: &std::collections::BTreeMap<String, harness::EntryCost>,
) {
    println!(
        "{:<16} {:>14} {:>14} {:>10}",
        "entrypoint", "cpu_insns", "Δ cpu", "rw(r/w)"
    );
    for name in harness::ENTRYPOINTS {
        let Some(c) = current.get(*name) else {
            continue;
        };
        let delta = baseline
            .costs
            .get(*name)
            .map(|b| format!("{:+}", c.cpu_insns - b.cpu_insns))
            .unwrap_or_else(|| "new".to_string());
        println!(
            "{:<16} {:>14} {:>14} {:>10}",
            name,
            c.cpu_insns,
            delta,
            format!("{}/{}", c.read_entries, c.write_entries)
        );
    }
}
