#![allow(clippy::disallowed_macros)]

use fixed_duration_bond::DataKey;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, Env};

fn hex(bytes: &Bytes) -> String {
    let mut result = String::with_capacity(bytes.len() as usize * 2);
    for byte in bytes.iter() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn data_key_fingerprints(env: &Env) -> Vec<(&'static str, String)> {
    let address = Address::generate(env);
    let fp = |key: DataKey| hex(&key.to_xdr(env));
    vec![
        ("Admin", fp(DataKey::Admin)),
        ("Token", fp(DataKey::Token)),
        ("Bond", fp(DataKey::Bond(address))),
        ("FeeConfig", fp(DataKey::FeeConfig)),
        ("PenaltyConfig", fp(DataKey::PenaltyConfig)),
        ("AccumulatedFees", fp(DataKey::AccumulatedFees)),
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
Bond = 0000001000000001000000020000000f00000004426f6e6400000012000000010000000000000000000000000000000000000000000000000000000000000001
FeeConfig = 0000001000000001000000010000000f00000009466565436f6e666967000000
PenaltyConfig = 0000001000000001000000010000000f0000000d50656e616c7479436f6e666967000000
AccumulatedFees = 0000001000000001000000010000000f0000000f416363756d756c617465644665657300
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
