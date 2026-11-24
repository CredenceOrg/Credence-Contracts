#![allow(clippy::disallowed_macros)]

use bounty_escrow::DataKey;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Bytes, Env};

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
        ("Token", fp(DataKey::Token)),
        ("NextId", fp(DataKey::NextId)),
        ("EscrowIndex", fp(DataKey::EscrowIndex)),
        ("Escrow", fp(DataKey::Escrow(0))),
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
Token = 0000001000000001000000010000000f00000005546f6b656e000000
NextId = 0000001000000001000000010000000f000000064e65787449640000
EscrowIndex = 0000001000000001000000010000000f0000000b457363726f77496e64657800
Escrow = 0000001000000001000000020000000f00000006457363726f770000000000050000000000000000
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
