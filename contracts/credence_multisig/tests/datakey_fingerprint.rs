#![allow(clippy::disallowed_macros)]

use credence_multisig::DataKey;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env};

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
        ("Signer", fp(DataKey::Signer(address.clone()))),
        ("SignerCount", fp(DataKey::SignerCount)),
        ("Threshold", fp(DataKey::Threshold)),
        ("ProposalCounter", fp(DataKey::ProposalCounter)),
        ("Proposal", fp(DataKey::Proposal(0))),
        ("Signature", fp(DataKey::Signature(0, address.clone()))),
        ("SignatureCount", fp(DataKey::SignatureCount(0))),
        ("SignerList", fp(DataKey::SignerList)),
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
        ("MaxPauseSigners", fp(DataKey::MaxPauseSigners)),
        (
            "ExecutedOp",
            fp(DataKey::ExecutedOp(BytesN::from_array(env, &[0; 32]))),
        ),
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
Signer = 0000001000000001000000020000000f000000065369676e6572000000000012000000010000000000000000000000000000000000000000000000000000000000000001
SignerCount = 0000001000000001000000010000000f0000000b5369676e6572436f756e7400
Threshold = 0000001000000001000000010000000f000000095468726573686f6c64000000
ProposalCounter = 0000001000000001000000010000000f0000000f50726f706f73616c436f756e74657200
Proposal = 0000001000000001000000020000000f0000000850726f706f73616c000000050000000000000000
Signature = 0000001000000001000000030000000f000000095369676e617475726500000000000005000000000000000000000012000000010000000000000000000000000000000000000000000000000000000000000001
SignatureCount = 0000001000000001000000020000000f0000000e5369676e6174757265436f756e740000000000050000000000000000
SignerList = 0000001000000001000000010000000f0000000a5369676e65724c6973740000
Paused = 0000001000000001000000010000000f000000065061757365640000
PauseSigner = 0000001000000001000000020000000f0000000b50617573655369676e65720000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseSignerCount = 0000001000000001000000010000000f0000001050617573655369676e6572436f756e74
PauseThreshold = 0000001000000001000000010000000f0000000e50617573655468726573686f6c640000
PauseProposalCounter = 0000001000000001000000010000000f00000014506175736550726f706f73616c436f756e746572
PauseProposal = 0000001000000001000000020000000f0000000d506175736550726f706f73616c000000000000050000000000000000
PauseApproval = 0000001000000001000000030000000f0000000d5061757365417070726f76616c00000000000005000000000000000000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseApprovalCount = 0000001000000001000000020000000f000000125061757365417070726f76616c436f756e740000000000050000000000000000
MaxPauseSigners = 0000001000000001000000010000000f0000000f4d617850617573655369676e65727300
ExecutedOp = 0000001000000001000000020000000f0000000a45786563757465644f7000000000000d000000200000000000000000000000000000000000000000000000000000000000000000
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
