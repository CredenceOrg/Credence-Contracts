#![allow(clippy::disallowed_macros)]

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Bytes, BytesN, Env};
use timelock::DataKey;

fn hex(bytes: &Bytes) -> String {
    let mut result = String::with_capacity(bytes.len() as usize * 2);
    for byte in bytes.iter() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn data_key_fingerprints(env: &Env) -> Vec<(&'static str, String)> {
    let fp = |key: DataKey| hex(&key.to_xdr(env));
    vec![
        ("Admin", fp(DataKey::Admin)),
        ("PendingAdmin", fp(DataKey::PendingAdmin)),
        ("OperationCounter", fp(DataKey::OperationCounter)),
        ("Operation", fp(DataKey::Operation(0))),
        (
            "ExecutedOp",
            fp(DataKey::ExecutedOp(BytesN::from_array(env, &[0; 32]))),
        ),
        ("CallBoundOperation", fp(DataKey::CallBoundOperation(0))),
    ]
}

fn render(fingerprints: &[(&'static str, String)]) -> String {
    let mut output = String::new();
    for (name, fingerprint) in fingerprints {
        output.push_str(name);
        output.push_str(" = ");
        output.push_str(fingerprint);
        output.push('\n');
    }
    output
}

const EXPECTED: &str = "\
Admin = 0000001000000001000000010000000f0000000541646d696e000000
PendingAdmin = 0000001000000001000000010000000f0000000c50656e64696e6741646d696e
OperationCounter = 0000001000000001000000010000000f000000104f7065726174696f6e436f756e746572
Operation = 0000001000000001000000020000000f000000094f7065726174696f6e000000000000050000000000000000
ExecutedOp = 0000001000000001000000020000000f0000000a45786563757465644f7000000000000d000000200000000000000000000000000000000000000000000000000000000000000000
CallBoundOperation = 0000001000000001000000020000000f0000001243616c6c426f756e644f7065726174696f6e0000000000050000000000000000
";

#[test]
fn datakey_fingerprints_are_pinned() {
    let env = Env::default();
    assert_eq!(render(&data_key_fingerprints(&env)), EXPECTED);
}

#[test]
fn datakey_fingerprints_are_unique() {
    let env = Env::default();
    let fingerprints = data_key_fingerprints(&env);
    for i in 0..fingerprints.len() {
        for j in (i + 1)..fingerprints.len() {
            assert_ne!(
                fingerprints[i].1, fingerprints[j].1,
                "{} and {} encode to the same storage key",
                fingerprints[i].0, fingerprints[j].0
            );
        }
    }
}
