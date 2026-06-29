//! Storage-key fingerprint snapshot for `admin::DataKey`.
//!
//! Every `DataKey` variant encodes to a specific byte sequence that becomes the
//! literal ledger key for its entry. This test pins the XDR encoding of each
//! variant so that any change which would move a key — renaming a variant or
//! altering its field shape — fails CI instead of silently orphaning live
//! ledger entries on upgrade.
//!
//! Field values are fixed, deterministic placeholders: the fingerprint is about
//! the variant *tag and shape*, not the runtime data stored under it.

use admin::{DataKey, AdminRole};
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
        ("AdminList", fp(DataKey::AdminList)),
        ("AdminInfo", fp(DataKey::AdminInfo(a.clone()))),
        ("RoleAdmins", fp(DataKey::RoleAdmins(AdminRole::SuperAdmin))),
        ("Initialized", fp(DataKey::Initialized)),
        ("MinAdmins", fp(DataKey::MinAdmins)),
        ("MaxAdmins", fp(DataKey::MaxAdmins)),
        ("Paused", fp(DataKey::Paused)),
        ("PauseSigner", fp(DataKey::PauseSigner(a.clone()))),
        ("PauseSignerCount", fp(DataKey::PauseSignerCount)),
        ("PauseThreshold", fp(DataKey::PauseThreshold)),
        ("PauseProposalCounter", fp(DataKey::PauseProposalCounter)),
        ("PauseProposal", fp(DataKey::PauseProposal(0))),
        ("PauseApproval", fp(DataKey::PauseApproval(0, a.clone()))),
        ("PauseApprovalCount", fp(DataKey::PauseApprovalCount(0))),
        ("Owner", fp(DataKey::Owner)),
        ("PendingOwner", fp(DataKey::PendingOwner)),
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
AdminList = 0000001000000001000000010000000f0000000941646d696e4c69737400
AdminInfo = 0000001000000001000000020000000f0000000941646d696e496e666f0000000000120000000100000000000000000000000000000000000000000000000000000000000000000001
RoleAdmins = 0000001000000001000000020000000f0000000b526f6c6541646d696e73000000000000001000000001000000010000000f0000000a537570657241646d696e
Initialized = 0000001000000001000000010000000f0000000b496e697469616c697a65640000
MinAdmins = 0000001000000001000000010000000f000000094d696e41646d696e730000
MaxAdmins = 0000001000000001000000010000000f000000094d617841646d696e730000
Paused = 0000001000000001000000010000000f00000006506175736564000000
PauseSigner = 0000001000000001000000020000000f0000000b50617573655369676e65720000000012000000010000000000000000000000000000000000000000000000000000000000000001
PauseSignerCount = 0000001000000001000000010000000f0000001050617573655369676e6572436f756e74
PauseThreshold = 0000001000000001000000010000000f0000000e50617573655468726573686f6c640000
PauseProposalCounter = 0000001000000001000000010000000f00000014506175736550726f706f73616c436f756e746572
PauseProposal = 0000001000000001000000020000000f0000000d506175736550726f706f73616c000000000000050000000000000000
PauseApproval = 0000001000000001000000030000000f0000000d5061757365417070726f76616c000000000000050000000000000000000000120000000100000000000000000000000000000000000000000000000000000000000000001
PauseApprovalCount = 0000001000000001000000020000000f000000125061757365417070726f76616c436f756e740000000000050000000000000000
Owner = 0000001000000001000000010000000f000000054f776e657200000000
PendingOwner = 0000001000000001000000010000000f0000000c50656e64696e674f776e6572
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
