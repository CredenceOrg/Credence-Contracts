//! Integration tests for the `credence-admin` CLI.
//!
//! All tests exercise the binary's dry-run path (no `--submit`).
//! A dry-run prints a JSON object containing:
//!   { "status": "dry_run", "envelope_xdr": "<base64>", "tx_hash": "<hex>", ... }
//! Tests assert both the exit status and the presence of the expected JSON keys
//! so the contract invocation XDR is actually being built (not just printed as
//! an empty stub).

use std::process::Command;

use stellar_baselib::{
    address::{Address, AddressTrait},
    xdr::{HostFunction, Limits, OperationBody, ReadXdr, ScVal, ScVec, TransactionEnvelope},
};

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

    #[allow(clippy::disallowed_macros)]
    assert_ne!(
        xdr_early_exit, xdr_weights,
        "early_exit and weights XDR should differ"
    );
    #[allow(clippy::disallowed_macros)]
    assert_ne!(
        xdr_weights, xdr_pause,
        "weights and pause XDR should differ"
    );
    #[allow(clippy::disallowed_macros)]
    assert_ne!(
        xdr_early_exit, xdr_pause,
        "early_exit and pause XDR should differ"
    );
}

// ---------------------------------------------------------------------------
// Timelock / arbitration dry-run golden tests
// ---------------------------------------------------------------------------

const SALT: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HASH_ARG: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

/// Run a dry-run command and parse its JSON report.
fn dry_run(args: &[&str]) -> serde_json::Value {
    let mut full_args = vec!["--contract", CONTRACT_ADDR];
    full_args.extend_from_slice(args);

    let output = Command::new(bin())
        .args(&full_args)
        .output()
        .expect("failed to run credence-admin");
    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("output is not valid JSON: {stdout}"));
    assert_eq!(report["status"], "dry_run");
    report
}

/// Decode the envelope back into the invoked function name and its `ScVal` args.
/// This proves the dry-run emits a real, decodable `InvokeHostFunction` XDR.
fn decoded_invoke(report: &serde_json::Value) -> (String, Vec<ScVal>) {
    let xdr = report["envelope_xdr"]
        .as_str()
        .expect("envelope_xdr is not a string");
    let envelope = TransactionEnvelope::from_xdr_base64(xdr, Limits::none())
        .expect("envelope_xdr must be decodable XDR");
    let TransactionEnvelope::Tx(v1) = envelope else {
        panic!("expected a v1 transaction envelope");
    };
    let operation = v1
        .tx
        .operations
        .into_iter()
        .next()
        .expect("transaction must contain one operation");
    let OperationBody::InvokeHostFunction(invoke) = operation.body else {
        panic!("expected an InvokeHostFunction operation");
    };
    let HostFunction::InvokeContract(call) = invoke.host_function else {
        panic!("expected an InvokeContract host function");
    };
    let function = String::from_utf8_lossy(call.function_name.as_ref()).to_string();
    (function, call.args.to_vec())
}

fn address(address: &str) -> ScVal {
    Address::new(address)
        .expect("valid address")
        .to_sc_val()
        .expect("address ScVal")
}

fn bytes(hex_value: &str) -> ScVal {
    ScVal::Bytes(
        hex::decode(hex_value)
            .expect("valid hex")
            .try_into()
            .expect("valid ScBytes"),
    )
}

fn symbol(name: &str) -> ScVal {
    ScVal::Symbol(name.as_bytes().to_vec().try_into().expect("valid ScSymbol"))
}

fn vec_scval(values: Vec<ScVal>) -> ScVal {
    ScVal::Vec(Some(ScVec::try_from(values).expect("valid ScVec")))
}

#[test]
fn timelock_queue_call_dry_run_decodes_to_expected_scvals() {
    let report = dry_run(&[
        "timelock-queue-call",
        "--proposer",
        ADMIN_ADDR,
        "--target",
        CONTRACT_ADDR,
        "--func",
        "set_quorum",
        "--arg",
        &format!("address:{ADMIN_ADDR}"),
        "--arg",
        &format!("bytes32:{HASH_ARG}"),
        "--arg",
        "vec:[u32:1,u32:2]",
        "--salt",
        SALT,
        "--delay",
        "300",
    ]);

    assert_eq!(
        report["function"], "queue_call",
        "unexpected function: {report}"
    );
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "queue_call");
    assert_eq!(args.len(), 6);
    assert_eq!(args[0], address(ADMIN_ADDR));
    assert_eq!(args[1], address(CONTRACT_ADDR));
    assert_eq!(args[2], symbol("set_quorum"));

    let ScVal::Vec(Some(call_args)) = &args[3] else {
        panic!("queue_call args must be ScVal::Vec");
    };
    assert_eq!(
        call_args.to_vec(),
        vec![
            address(ADMIN_ADDR),
            bytes(HASH_ARG),
            vec_scval(vec![ScVal::U32(1), ScVal::U32(2)]),
        ]
    );
    assert_eq!(args[4], bytes(SALT));
    assert_eq!(args[5], ScVal::U64(300));
}

#[test]
fn timelock_execute_and_cancel_dry_run_decode_expected_args() {
    let report = dry_run(&["timelock-execute", "--op-id", "42"]);
    assert_eq!(report["function"], "execute_operation");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "execute_operation");
    assert_eq!(args, vec![ScVal::U64(42)]);

    let report = dry_run(&["timelock-cancel", "--admin", ADMIN_ADDR, "--op-id", "42"]);
    assert_eq!(report["function"], "cancel_operation");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "cancel_operation");
    assert_eq!(args, vec![address(ADMIN_ADDR), ScVal::U64(42)]);
}

#[test]
fn arbitration_register_and_unregister_dry_run_decode_expected_args() {
    let report = dry_run(&[
        "arbitration-register-arbitrator",
        "--arbitrator",
        TREASURY_ADDR,
        "--weight",
        "-7",
    ]);
    assert_eq!(report["function"], "register_arbitrator");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "register_arbitrator");
    assert_eq!(args, vec![address(TREASURY_ADDR), ScVal::from(-7_i128)]);

    let report = dry_run(&[
        "arbitration-unregister-arbitrator",
        "--arbitrator",
        TREASURY_ADDR,
    ]);
    assert_eq!(report["function"], "unregister_arbitrator");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "unregister_arbitrator");
    assert_eq!(args, vec![address(TREASURY_ADDR)]);
}

#[test]
fn arbitration_set_quorum_and_pause_dry_run_decode_expected_args() {
    let report = dry_run(&[
        "arbitration-set-quorum",
        "--admin",
        ADMIN_ADDR,
        "--min-total-weight",
        "100",
        "--min-voters",
        "3",
    ]);
    assert_eq!(report["function"], "set_quorum");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "set_quorum");
    assert_eq!(
        args,
        vec![address(ADMIN_ADDR), ScVal::from(100_i128), ScVal::U32(3)]
    );

    let report = dry_run(&["arbitration-pause", "--caller", ADMIN_ADDR]);
    assert_eq!(report["function"], "pause");
    let (function, args) = decoded_invoke(&report);
    assert_eq!(function, "pause");
    assert_eq!(args, vec![address(ADMIN_ADDR)]);
}

#[test]
fn new_commands_reject_invalid_typed_args_offline() {
    let mut full_args = vec![
        "--contract",
        CONTRACT_ADDR,
        "timelock-queue-call",
        "--proposer",
        ADMIN_ADDR,
        "--target",
        CONTRACT_ADDR,
        "--func",
        "set_quorum",
        "--arg",
        "unsupported:1",
        "--salt",
        SALT,
        "--delay",
        "300",
    ];
    let output = Command::new(bin())
        .args(&full_args)
        .output()
        .expect("failed to run credence-admin");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported --arg type"));

    full_args[10] = "bytes32:abcd";
    let output = Command::new(bin())
        .args(&full_args)
        .output()
        .expect("failed to run credence-admin");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("32 bytes"));
}

// ---------------------------------------------------------------------------
// --submit guards: missing contract, bad signer, secret redaction
// ---------------------------------------------------------------------------

#[test]
fn submit_without_contract_fails_and_never_leaks_secret() {
    let secret = "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
    let output = Command::new(bin())
        .args([
            "--submit",
            "--signer",
            secret,
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
        "expected non-zero exit when --submit used without --contract"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--contract") || stderr.contains("CREDENCE_CONTRACT"),
        "expected contract-related error message; got: {stderr}"
    );
    assert!(!stdout.contains(secret), "secret leaked to stdout");
    assert!(!stderr.contains(secret), "secret leaked to stderr");
}

#[test]
fn submit_with_bad_signer_fails_and_never_leaks_secret() {
    let bad_secret = "SBADSECRETKEYVALUE000000000000000000000000000000000000";
    let output = Command::new(bin())
        .args([
            "--contract",
            CONTRACT_ADDR,
            "--submit",
            "--signer",
            bad_secret,
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
        "expected non-zero exit when --submit used with bad --signer"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid signer key"),
        "expected invalid signer key error; got: {stderr}"
    );
    assert!(!stdout.contains(bad_secret), "bad signer leaked to stdout");
    assert!(!stderr.contains(bad_secret), "bad signer leaked to stderr");
}
