//! Integration tests for the `credence-admin` CLI.
//!
//! All tests exercise the binary's dry-run path (no `--submit`).
//! A dry-run prints a JSON object containing:
//!   { "status": "dry_run", "envelope_xdr": "<base64>", "tx_hash": "<hex>", ... }
//! Tests assert both the exit status and the presence of the expected JSON keys
//! so the contract invocation XDR is actually being built (not just printed as
//! an empty stub).

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_credence-admin")
}

/// Shared dummy addresses used across tests.
const ADMIN_ADDR: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const TREASURY_ADDR: &str = "GBBM6BKZPEHWYO3E3YKREDPQXMS4VK35YLNU7NFBRI26RAN7GI5POFBB";
// A valid Soroban contract C-address (all-zeros payload).
const CONTRACT_ADDR: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

// ---------------------------------------------------------------------------
// bond-set-early-exit-config
// ---------------------------------------------------------------------------

#[test]
fn bond_set_early_exit_dry_run_outputs_xdr() {
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "bond-set-early-exit-config",
            "--admin",
            ADMIN_ADDR,
            "--treasury",
            TREASURY_ADDR,
            "--bps",
            "500",
        ])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Must emit a JSON dry-run report with real XDR.
    assert!(
        stdout.contains("\"status\": \"dry_run\""),
        "missing status: {stdout}"
    );
    assert!(
        stdout.contains("\"envelope_xdr\""),
        "missing envelope_xdr: {stdout}"
    );
    assert!(stdout.contains("\"tx_hash\""), "missing tx_hash: {stdout}");
    assert!(
        stdout.contains("\"set_early_exit_config\""),
        "missing function name: {stdout}"
    );
}

#[test]
fn bond_set_early_exit_dry_run_xdr_is_nonempty() {
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "bond-set-early-exit-config",
            "--admin",
            ADMIN_ADDR,
            "--treasury",
            TREASURY_ADDR,
            "--bps",
            "100",
        ])
        .output()
        .expect("failed to run credence-admin");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("output is not valid JSON: {stdout}"));
    let xdr = v["envelope_xdr"]
        .as_str()
        .expect("envelope_xdr is not a string");
    assert!(!xdr.is_empty(), "envelope_xdr must not be empty");
    assert!(xdr.len() > 20, "envelope_xdr looks too short: {xdr}");
}

// ---------------------------------------------------------------------------
// bond-set-weights
// ---------------------------------------------------------------------------

#[test]
fn bond_set_weights_dry_run_outputs_xdr() {
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "bond-set-weights",
            "--admin",
            ADMIN_ADDR,
            "--multiplier-bps",
            "10000",
            "--max-weight",
            "100",
        ])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"status\": \"dry_run\""),
        "missing status: {stdout}"
    );
    assert!(
        stdout.contains("\"envelope_xdr\""),
        "missing envelope_xdr: {stdout}"
    );
    assert!(
        stdout.contains("\"set_weight_config\""),
        "missing function name: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// delegation-set-pause-signer
// ---------------------------------------------------------------------------

#[test]
fn delegation_set_pause_signer_dry_run_outputs_xdr() {
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "delegation-set-pause-signer",
            "--admin",
            ADMIN_ADDR,
            "--pause-signer",
            TREASURY_ADDR,
            "--enabled=true",
        ])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"status\": \"dry_run\""),
        "missing status: {stdout}"
    );
    assert!(
        stdout.contains("\"envelope_xdr\""),
        "missing envelope_xdr: {stdout}"
    );
    assert!(
        stdout.contains("\"set_pause_signer\""),
        "missing function name: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// --submit guard: missing signer must fail with a clear message
// ---------------------------------------------------------------------------

#[test]
fn submit_without_signer_fails() {
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "--submit",
            "bond-set-weights",
            "--admin",
            ADMIN_ADDR,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "50",
        ])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        !output.status.success(),
        "expected non-zero exit when --submit used without --signer"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CREDENCE_SIGNER") || stderr.contains("signer"),
        "expected signer-related error message; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// XDR differs between commands (each builder encodes different args)
// ---------------------------------------------------------------------------

#[test]
fn different_commands_produce_different_xdr() {
    let run = |extra_args: &[&str]| -> String {
        let mut args = vec!["--contract", CONTRACT_ADDR];
        args.extend_from_slice(extra_args);
        let out = Command::new(bin())
            .args(&args)
            .output()
            .expect("failed to run credence-admin");
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("invalid JSON");
        v["envelope_xdr"].as_str().unwrap().to_string()
    };

    let xdr_early_exit = run(&[
        "bond-set-early-exit-config",
        "--admin",
        ADMIN_ADDR,
        "--treasury",
        TREASURY_ADDR,
        "--bps",
        "500",
    ]);
    let xdr_weights = run(&[
        "bond-set-weights",
        "--admin",
        ADMIN_ADDR,
        "--multiplier-bps",
        "9000",
        "--max-weight",
        "200",
    ]);
    let xdr_pause = run(&[
        "delegation-set-pause-signer",
        "--admin",
        ADMIN_ADDR,
        "--pause-signer",
        TREASURY_ADDR,
        "--enabled=true",
    ]);

    assert_ne!(
        xdr_early_exit, xdr_weights,
        "early_exit and weights XDR should differ"
    );
    assert_ne!(
        xdr_weights, xdr_pause,
        "weights and pause XDR should differ"
    );
    assert_ne!(
        xdr_early_exit, xdr_pause,
        "early_exit and pause XDR should differ"
    );
}
