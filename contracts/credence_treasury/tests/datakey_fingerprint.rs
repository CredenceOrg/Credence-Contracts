#![allow(clippy::disallowed_macros)]

use credence_treasury::{DataKey, FundSource};
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
        ("TotalBalance", fp(DataKey::TotalBalance)),
        (
            "BalanceBySource",
            fp(DataKey::BalanceBySource(FundSource::ProtocolFee)),
        ),
        ("CumulativeReceived", fp(DataKey::CumulativeReceived)),
        (
            "CumulativeReceivedBySource",
            fp(DataKey::CumulativeReceivedBySource(
                FundSource::ProtocolFee,
            )),
        ),
        ("Depositor", fp(DataKey::Depositor(address.clone()))),
        ("Signer", fp(DataKey::Signer(address.clone()))),
        ("SignerCount", fp(DataKey::SignerCount)),
        ("Threshold", fp(DataKey::Threshold)),
        ("ProposalCounter", fp(DataKey::ProposalCounter)),
        ("Proposal", fp(DataKey::Proposal(0))),
        ("Approval", fp(DataKey::Approval(0, address.clone()))),
        ("ApprovalCount", fp(DataKey::ApprovalCount(0))),
        ("MinLiquidity", fp(DataKey::MinLiquidity)),
        ("Token", fp(DataKey::Token)),
        ("ProposalTtl", fp(DataKey::ProposalTtl)),
        ("Corridor", fp(DataKey::Corridor(address))),
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
PauseProposal = 0000001000000001000000020000000f0000000d506175736550726f706f73616c000000000000050000000000000000
PauseApproval = 0000001000000001000000030000000f0000000d5061757365417070726f76616c00000000000005000000000000000000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseApprovalCount = 0000001000000001000000020000000f000000125061757365417070726f76616c436f756e740000000000050000000000000000
TotalBalance = 0000001000000001000000010000000f0000000c546f74616c42616c616e6365
BalanceBySource = 0000001000000001000000020000000f0000000f42616c616e63654279536f75726365000000001000000001000000010000000f0000000b50726f746f636f6c46656500
CumulativeReceived = 0000001000000001000000010000000f0000001243756d756c617469766552656365697665640000
CumulativeReceivedBySource = 0000001000000001000000020000000f0000001a43756d756c617469766552656365697665644279536f7572636500000000001000000001000000010000000f0000000b50726f746f636f6c46656500
Depositor = 0000001000000001000000020000000f000000094465706f7369746f7200000000000012000000010000000000000000000000000000000000000000000000000000000000000001
Signer = 0000001000000001000000020000000f000000065369676e6572000000000012000000010000000000000000000000000000000000000000000000000000000000000001
SignerCount = 0000001000000001000000010000000f0000000b5369676e6572436f756e7400
Threshold = 0000001000000001000000010000000f000000095468726573686f6c64000000
ProposalCounter = 0000001000000001000000010000000f0000000f50726f706f73616c436f756e74657200
Proposal = 0000001000000001000000020000000f0000000850726f706f73616c000000050000000000000000
Approval = 0000001000000001000000030000000f00000008417070726f76616c00000005000000000000000000000012000000010000000000000000000000000000000000000000000000000000000000000001
ApprovalCount = 0000001000000001000000020000000f0000000d417070726f76616c436f756e74000000000000050000000000000000
MinLiquidity = 0000001000000001000000010000000f0000000c4d696e4c6971756964697479
Token = 0000001000000001000000010000000f00000005546f6b656e000000
ProposalTtl = 0000001000000001000000010000000f0000000b50726f706f73616c54746c00
Corridor = 0000001000000001000000020000000f00000008436f727269646f7200000012000000010000000000000000000000000000000000000000000000000000000000000001
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
