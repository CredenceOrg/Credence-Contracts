//! Regression guard for the on-chain contract interface (`contractspecv0`).
//!
//! The Soroban SDK embeds every public type and function as XDR in the WASM
//! `contractspecv0` custom section (surfacing as `__SPEC_XDR_*` symbols at link
//! time). This test snapshots that byte stream and fails CI when it changes
//! without an explicit [`credence_delegation::CONTRACT_SPEC_VERSION`] bump.
//!
//! Refresh workflow (intentional ABI change only):
//! 1. `cargo build -p credence_delegation --target wasm32-unknown-unknown --release`
//! 2. Copy the new hex from test failure output (or `tests/spec_xdr/`) into
//!    `tests/spec_xdr/credence_delegation.v1.hex`.
//! 3. Increment `CONTRACT_SPEC_VERSION` in `src/lib.rs`.
//! 4. Update `EXPECTED_VERSIONED_MANIFEST` below.

use std::path::PathBuf;
use std::process::Command;

use soroban_spec::read::{raw_from_wasm, FromWasmError};

const WASM_REL: &str = "target/wasm32-unknown-unknown/release/credence_delegation.wasm";
const EXPECTED_SPEC_XDR_HEX: &str = include_str!("spec_xdr/credence_delegation.v1.hex");

/// `{CONTRACT_SPEC_VERSION}:{spec_xdr_hex}` — ties the snapshot to an explicit bump.
const EXPECTED_VERSIONED_MANIFEST: &str =
    concat!("1:", include_str!("spec_xdr/credence_delegation.v1.hex"));

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn wasm_path() -> PathBuf {
    workspace_root().join(WASM_REL)
}

fn ensure_release_wasm_built() {
    if wasm_path().is_file() {
        return;
    }
    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "-p",
            "credence_delegation",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .current_dir(workspace_root())
        .status()
        .expect("failed to spawn wasm build for spec regression test");
    assert!(
        status.success(),
        "release wasm build failed — spec regression test requires {WASM_REL}"
    );
}

fn read_release_wasm() -> Vec<u8> {
    ensure_release_wasm_built();
    std::fs::read(wasm_path()).unwrap_or_else(|err| {
        panic!(
            "could not read {}: {err}. Run `cargo build -p credence_delegation \
             --target wasm32-unknown-unknown --release` first.",
            wasm_path().display()
        )
    })
}

fn current_spec_xdr_hex() -> String {
    let wasm = read_release_wasm();
    let raw = raw_from_wasm(&wasm).unwrap_or_else(|err| match err {
        FromWasmError::NotFound => {
            panic!(
                "contractspecv0 section missing from {}",
                wasm_path().display()
            )
        }
        other => panic!("failed to parse contract spec from wasm: {other}"),
    });
    raw.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn versioned_manifest(spec_hex: &str) -> String {
    format!("{}:{spec_hex}", credence_delegation::CONTRACT_SPEC_VERSION)
}

#[test]
fn contract_spec_xdr_is_pinned() {
    let actual = current_spec_xdr_hex();
    assert_eq!(
        actual, EXPECTED_SPEC_XDR_HEX,
        "contractspecv0 XDR changed. If this ABI change is intentional, refresh \
         tests/spec_xdr/credence_delegation.v1.hex and bump \
         credence_delegation::CONTRACT_SPEC_VERSION. See tests/spec_xdr_regression.rs."
    );
}

#[test]
fn contract_spec_version_matches_pinned_manifest() {
    let actual = versioned_manifest(&current_spec_xdr_hex());
    assert_eq!(
        actual, EXPECTED_VERSIONED_MANIFEST,
        "spec snapshot and CONTRACT_SPEC_VERSION are out of sync — bump \
         CONTRACT_SPEC_VERSION when refreshing the spec XDR pin."
    );
}

#[test]
fn spec_change_without_version_bump_is_rejected() {
    let spec_hex = current_spec_xdr_hex();
    let stale_version = credence_delegation::CONTRACT_SPEC_VERSION.saturating_sub(1);
    let stale_manifest = format!("{stale_version}:{spec_hex}");
    assert_ne!(
        stale_manifest, EXPECTED_VERSIONED_MANIFEST,
        "guard sanity check: a stale version label must not satisfy the pinned manifest"
    );
}

#[test]
fn contract_spec_xdr_detects_single_byte_drift() {
    let actual = current_spec_xdr_hex();
    let mut drifted = actual.into_bytes();
    let idx = drifted.len() / 2;
    drifted[idx] ^= 0x01;
    let drifted_hex = String::from_utf8(drifted).expect("hex stays ascii");
    assert_ne!(
        drifted_hex, EXPECTED_SPEC_XDR_HEX,
        "single-byte drift must fail the snapshot guard"
    );
}
