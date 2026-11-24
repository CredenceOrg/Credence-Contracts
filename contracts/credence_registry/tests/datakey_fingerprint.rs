#![allow(clippy::disallowed_macros)]

use credence_registry::storage::DataKey;
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
        ("Paused", fp(DataKey::Paused)),
        ("PauseSigner", fp(DataKey::PauseSigner(address.clone()))),
        ("PauseSignerCount", fp(DataKey::PauseSignerCount)),
        ("PauseThreshold", fp(DataKey::PauseThreshold)),
        ("PauseProposalCounter", fp(DataKey::PauseProposalCounter)),
        ("PauseProposal", fp(DataKey::PauseProposal(0))),
        (
            "PauseApproval",
            fp(DataKey::PauseApproval(0, address.clone())),
        ),
        ("PauseApprovalCount", fp(DataKey::PauseApprovalCount(0))),
        ("IdentityToBond", fp(DataKey::IdentityToBond(address.clone()))),
        ("BondToIdentity", fp(DataKey::BondToIdentity(address.clone()))),
        ("RegisteredIdentities", fp(DataKey::RegisteredIdentities)),
        (
            "AllowNonInterface",
            fp(DataKey::AllowNonInterface(address.clone())),
        ),
        ("BondCodeHash", fp(DataKey::BondCodeHash)),
        ("IdentityCount", fp(DataKey::IdentityCount)),
        ("IdentityAt", fp(DataKey::IdentityAt(0))),
        ("IdentityIndex", fp(DataKey::IdentityIndex(address))),
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
Paused = 0000001000000001000000010000000f000000065061757365640000
PauseSigner = 0000001000000001000000020000000f0000000b50617573655369676e65720000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseSignerCount = 0000001000000001000000010000000f0000001050617573655369676e6572436f756e74
PauseThreshold = 0000001000000001000000010000000f0000000e50617573655468726573686f6c640000
PauseProposalCounter = 0000001000000001000000010000000f00000014506175736550726f706f73616c436f756e746572
PauseProposal = 0000001000000001000000020000000f0000000d506175736550726f706f73616c0000000000000300000000
PauseApproval = 0000001000000001000000030000000f0000000d5061757365417070726f76616c000000000000030000000000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseApprovalCount = 0000001000000001000000020000000f000000125061757365417070726f76616c436f756e7400000000000300000000
IdentityToBond = 0000001000000001000000020000000f0000000e4964656e74697479546f426f6e64000000000012000000010000000000000000000000000000000000000000000000000000000000000001
BondToIdentity = 0000001000000001000000020000000f0000000e426f6e64546f4964656e74697479000000000012000000010000000000000000000000000000000000000000000000000000000000000001
RegisteredIdentities = 0000001000000001000000010000000f00000014526567697374657265644964656e746974696573
AllowNonInterface = 0000001000000001000000020000000f00000011416c6c6f774e6f6e496e7465726661636500000000000012000000010000000000000000000000000000000000000000000000000000000000000001
BondCodeHash = 0000001000000001000000010000000f0000000c426f6e64436f646548617368
IdentityCount = 0000001000000001000000010000000f0000000d4964656e74697479436f756e74000000
IdentityAt = 0000001000000001000000020000000f0000000a4964656e74697479417400000000000300000000
IdentityIndex = 0000001000000001000000020000000f0000000d4964656e74697479496e64657800000000000012000000010000000000000000000000000000000000000000000000000000000000000001
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
