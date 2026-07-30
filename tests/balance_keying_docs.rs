use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn balance_keying_doc_is_linked_from_readme() {
    let readme = fs::read_to_string(root().join("README.md")).expect("README should be readable");
    assert!(
        readme.contains("docs/balance-keying.md"),
        "README should link the balance keying guide"
    );
}

#[test]
fn balance_keying_doc_tracks_bond_and_treasury_keys() {
    let doc = fs::read_to_string(root().join("docs/balance-keying.md"))
        .expect("balance keying doc should be readable");
    let bond = fs::read_to_string(root().join("contracts/credence_bond/src/lib.rs"))
        .expect("bond source should be readable");
    let treasury = fs::read_to_string(root().join("contracts/credence_treasury/src/treasury.rs"))
        .expect("treasury source should be readable");

    for marker in [
        "DataKey::Bond(Address)",
        "DataKey::AttesterStake(Address)",
        "DataKey::ClaimableAmount(Address)",
        "DataKey::PendingClaims(Address)",
        "DataKey::TotalBalance",
        "DataKey::BalanceBySource(FundSource)",
        "DataKey::CumulativeReceivedBySource(FundSource)",
        "cumulative_to_u256",
    ] {
        assert!(doc.contains(marker), "doc should mention {marker}");
    }

    assert!(bond.contains("Bond(Address)"));
    assert!(bond.contains("ClaimableAmount(Address)"));
    assert!(bond.contains("PendingClaims(Address)"));
    assert!(treasury.contains("TotalBalance"));
    assert!(treasury.contains("BalanceBySource(FundSource)"));
    assert!(treasury.contains("CumulativeReceivedBySource(FundSource)"));
    assert!(treasury.contains("pub fn cumulative_to_u256"));
}