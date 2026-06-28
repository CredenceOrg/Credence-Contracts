//! Storage-key fingerprint snapshot for `arbitration::DataKey`.
//!
//! Every `DataKey` variant encodes to a specific byte sequence that becomes the
//! literal ledger key for its entry. This test pins the XDR encoding of each
//! variant so that any change which would move a key — renaming a variant or
//! altering its field shape — fails CI instead of silently orphaning live
//! ledger entries on upgrade.
//!
//! Field values are fixed, deterministic placeholders: the fingerprint is about
//! the variant *tag and shape*, not the runtime data stored under it.

use arbitration::DataKey;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, Env};

fn hex(bytes: &Bytes) -> String {
    let mut s = String::with_capacity(bytes.len() as usize * 2);
    for byte in bytes.iter() {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// Encode every `DataKey` variant, in declaration order, to `(name, xdr-hex)`.
///
/// Placeholder field values are built from a fresh, deterministically-seeded
/// `Env`, so the two generated addresses are identical on every run.
fn fingerprints(env: &Env) -> Vec<(&'static str, String)> {
    let a = Address::generate(env);
    let fp = |k: DataKey| hex(&k.to_xdr(env));

    vec![
        ("Admin", fp(DataKey::Admin)),
        ("Paused", fp(DataKey::Paused)),
        ("PauseSigner", fp(DataKey::PauseSigner(a.clone()))),
        ("PauseSignerCount", fp(DataKey::PauseSignerCount)),
        ("PauseThreshold", fp(DataKey::PauseThreshold)),
        ("PauseProposalCounter", fp(DataKey::PauseProposalCounter)),
        ("PauseProposal", fp(DataKey::PauseProposal(0))),
        ("PauseApproval", fp(DataKey::PauseApproval(0, a.clone()))),
        ("PauseApprovalCount", fp(DataKey::PauseApprovalCount(0))),
        ("Arbitrator", fp(DataKey::Arbitrator(a.clone()))),
        ("Dispute", fp(DataKey::Dispute(0))),
        ("DisputeCounter", fp(DataKey::DisputeCounter)),
        ("DisputeVotes", fp(DataKey::DisputeVotes(0))),
        ("VoterCasted", fp(DataKey::VoterCasted(0, a.clone()))),
        ("VoterCounter", fp(DataKey::VoterCounter(0))),
        ("ArbitratorRegistry", fp(DataKey::ArbitratorRegistry)),
        ("MinTotalWeight", fp(DataKey::MinTotalWeight)),
        ("MinVoters", fp(DataKey::MinVoters)),
    ]
}

fn render(fps: &[(&'static str, String)]) -> String {
    let mut out = String::new();
    for (name, hex) in fps {

        out.push_str(name);
        out.push_str(" = ");
        out.push_str(hex);
        out.push('\n');
    }
    out
}

/// The pinned snapshot. Regenerate intentionally (and review the diff!) only
/// when a key change is deliberate, by running this test with `--nocapture` and
/// copying the printed block here.
const EXPECTED: &str = "\
Admin = 0000001000000001000000010000000f0000000541646d696e000000
Paused = 0000001000000001000000010000000f00000006506175736564000000
PauseSigner = 0000001000000001000000020000000f0000000b50617573655369676e65720000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseSignerCount = 0000001000000001000000010000000f0000001050617573655369676e6572436f756e74
PauseThreshold = 0000001000000001000000010000000f0000000e50617573655468726573686f6c640000
PauseProposalCounter = 0000001000000001000000010000000f00000014506175736550726f706f73616c436f756e746572
PauseProposal = 0000001000000001000000020000000f0000000d506175736550726f706f73616c000000000000050000000000000000
PauseApproval = 0000001000000001000000030000000f0000000d5061757365417070726f76616c000000000000050000000000000000000000120000000100000000000000000000000000000000000000000000000000000000000000001
PauseApprovalCount = 0000001000000001000000020000000f000000125061757365417070726f76616c436f756e740000000000050000000000000000
Arbitrator = 0000001000000001000000020000000f0000000a41726269747261746f7200000000000012000000010000000000000000000000000000000000000000000000000000000000000000000000000001
Dispute = 0000001000000001000000020000000f000000074469737075746500000000000000050000000000000000
DisputeCounter = 0000001000000001000000010000000f0000000e44697370757465436f756e74657200
DisputeVotes = 0000001000000001000000020000000f0000000c44697370757465566f7465730000000000000050000000000000000
VoterCasted = 0000001000000001000000030000000f0000000b566f7465724361737465640000000000000050000000000000000000000120000000100000000000000000000000000000000000000000000000000000000000000001
VoterCounter = 0000001000000001000000020000000f0000000b566f746572436f756e74657200000000000000050000000000000000
ArbitratorRegistry = 0000001000000001000000010000000f0000001241726269747261746f725265676973747279
MinTotalWeight = 0000001000000001000000010000000f0000000e4d696e546f74616c57656967687400
MinVoters = 0000001000000001000000010000000f000000094d696e566f746572730000
";

#[test]
fn datakey_fingerprints_are_pinned() {
    let env = Env::default();
    let actual = render(&fingerprints(&env));
    // Printed so an intentional change can be copied back into EXPECTED.
    println!("---- DataKey fingerprints ----\n{actual}------------------------------");
    assert_eq!(
        actual, EXPECTED,
        "A DataKey encoding changed — a storage key moved and existing ledger \
         entries would be orphaned. If this change is intentional, review the \
         diff and update EXPECTED."
    );
}

/// Sanity: no two variants share a fingerprint (which would alias their storage).
#[test]
fn datakey_fingerprints_are_unique() {
    let env = Env::default();
    let fps = fingerprints(&env);
    for i in 0..fps.len() {
        for j in (i + 1)..fps.len() {
            assert_ne!(
                fps[i].1, fps[j].1,
                "{} and {} encode to the same storage key",
                fps[i].0, fps[j].0
            );
        }
    }
}
