//! Storage-key fingerprint snapshot for `credence_bond::DataKey` and `credence_bond::UpgradeKey`.
//!
//! Every `DataKey` variant encodes to a specific byte sequence that becomes the
//! literal ledger key for its entry. This test pins the XDR encoding of each
//! variant so that any change which would move a key — renaming a variant or
//! altering its field shape — fails CI instead of silently orphaning live
//! ledger entries on upgrade.
//!
//! Field values are fixed, deterministic placeholders: the fingerprint is about
//! the variant *tag and shape*, not the runtime data stored under it.

use credence_bond::{DataKey, UpgradeKey};
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
fn data_key_fingerprints(env: &Env) -> Vec<(&'static str, String)> {
    let a = Address::generate(env);
    let fp = |k: DataKey| hex(&k.to_xdr(env));

    vec![
        ("Admin", fp(DataKey::Admin)),
        ("Bond", fp(DataKey::Bond)),
        ("Attester", fp(DataKey::Attester(a.clone()))),
        ("Attestation", fp(DataKey::Attestation(0))),
        ("AttestationCounter", fp(DataKey::AttestationCounter)),
        ("SubjectAttestations", fp(DataKey::SubjectAttestations(a.clone()))),
        ("SubjectAttestationCount", fp(DataKey::SubjectAttestationCount(a.clone()))),
        ("Nonce", fp(DataKey::Nonce(a.clone()))),
        ("AttesterStake", fp(DataKey::AttesterStake(a.clone()))),
        ("WeightConfig", fp(DataKey::WeightConfig)),
        ("EarlyExitConfig", fp(DataKey::EarlyExitConfig)),
        ("GraceWindow", fp(DataKey::GraceWindow)),
        ("BondToken", fp(DataKey::BondToken)),
        ("TierThresholds", fp(DataKey::TierThresholds)),
        ("LastCollateralIncreaseLedger", fp(DataKey::LastCollateralIncreaseLedger)),
        ("PendingClaims", fp(DataKey::PendingClaims(a.clone()))),
        ("ClaimableAmount", fp(DataKey::ClaimableAmount(a.clone()))),
        ("ClaimCounter", fp(DataKey::ClaimCounter)),
        ("ClaimById", fp(DataKey::ClaimById(0))),
        ("Upgrade", fp(DataKey::Upgrade(UpgradeKey::Admin))),
        ("LiquidationTreasury", fp(DataKey::LiquidationTreasury)),
        ("Liquidated", fp(DataKey::Liquidated(a.clone()))),
        ("SlashTreasury", fp(DataKey::SlashTreasury)),
    ]
}

/// Encode every `UpgradeKey` variant, in declaration order, to `(name, xdr-hex)`.
fn upgrade_key_fingerprints(env: &Env) -> Vec<(&'static str, String)> {
    let a = Address::generate(env);
    let fp = |k: UpgradeKey| hex(&k.to_xdr(env));

    vec![
        ("UpgradeAuth", fp(UpgradeKey::Auth(a.clone()))),
        ("AuthorizedUpgraders", fp(UpgradeKey::AuthorizedUpgraders)),
        ("Implementation", fp(UpgradeKey::Implementation)),
        ("UpgradeAdmin", fp(UpgradeKey::Admin)),
        ("PendingUpgradeAdmin", fp(UpgradeKey::PndgUpgrAdmin)),
        ("UpgradeProposal", fp(UpgradeKey::Proposal(0))),
        ("NextProposalId", fp(UpgradeKey::NextProposalId)),
        ("UpgradeHistory", fp(UpgradeKey::History)),
    ]
}

fn render(fps: &[(&'static str, String)]) -> String {
    let mut out = String::new();
    for (name, hex in fps {
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
const EXPECTED_DATA_KEYS: &str = "\
Admin = 0000001000000001000000010000000f0000000541646d696e000000
Bond = 0000001000000001000000010000000f00000004426f6e6400000000
Attester = 0000001000000001000000020000000f000000084174746573746572000000001200000001000000000000000000000000000000000000000000000000000000000000001
Attestation = 0000001000000001000000020000000f0000000b4174746573746174696f6e000000000000050000000000000000
AttestationCounter = 0000001000000001000000010000000f000000124174746573746174696f6e436f756e746572
SubjectAttestations = 0000001000000001000000020000000f000000125375626a6563744174746573746174696f6e730000000000120000000100000000000000000000000000000000000000000000000000000000000000000000000001
SubjectAttestationCount = 0000001000000001000000020000000f000000175375626a6563744174746573746174696f6e436f756e74000000000012000000010000000000000000000000000000000000000000000000000000000000000000000000000001
Nonce = 0000001000000001000000020000000f000000054e6f6e63650000000000001200000001000000000000000000000000000000000000000000000000000000000000000001
AttesterStake = 0000001000000001000000020000000f0000000d41747465737465725374616b650000000000120000000100000000000000000000000000000000000000000000000000000000000000000000000001
WeightConfig = 0000001000000001000000010000000f0000000c576569676874436f6e666967000000
EarlyExitConfig = 0000001000000001000000010000000f0000000f4561726c7945786974436f6e66696700
GraceWindow = 0000001000000001000000010000000f0000000b477261636557696e646f77000000
BondToken = 0000001000000001000000010000000f00000009426f6e64546f6b656e00000000
TierThresholds = 0000001000000001000000010000000f0000000e546965725468726573686f6c647300
LastCollateralIncreaseLedger = 0000001000000001000000010000000f0000001c4c617374436f6c6c61746572616c496e6372656173654c65646765720000
PendingClaims = 0000001000000001000000020000000f0000000d50656e64696e67436c61696d730000000000120000000100000000000000000000000000000000000000000000000000000000000000000000000001
ClaimableAmount = 0000001000000001000000020000000f0000000e436c61696d61626c65416d6f756e74000000000012000000010000000000000000000000000000000000000000000000000000000000000000000000001
ClaimCounter = 0000001000000001000000010000000f0000000b436c61696d436f756e746572000000
ClaimById = 0000001000000001000000020000000f00000009436c61696d42794964000000000000050000000000000000
Upgrade = 0000001000000001000000020000000f0000000755706772616465000000000000001000000001000000010000000f0000000541646d696e000000
LiquidationTreasury = 0000001000000001000000010000000f000000134c69717569646174696f6e5472656173757279
Liquidated = 0000001000000001000000020000000f000000094c697175696461746564000000000000120000000100000000000000000000000000000000000000000000000000000000000000000000000001
SlashTreasury = 0000001000000001000000010000000f0000000d536c61736854726561737572790000
";

const EXPECTED_UPGRADE_KEYS: &str = "\
UpgradeAuth = 0000001000000001000000020000000f000000044175746800000000000000120000000100000000000000000000000000000000000000000000000000000000000000001
AuthorizedUpgraders = 0000001000000001000000010000000f00000011417574686f72697a6564557067726164657273
Implementation = 0000001000000001000000010000000f0000000e496d706c656d656e746174696f6e00
UpgradeAdmin = 0000001000000001000000010000000f0000000541646d696e000000
PendingUpgradeAdmin = 0000001000000001000000010000000f0000000c506e64675570677241646d696e
UpgradeProposal = 0000001000000001000000020000000f0000000850726f706f73616c000000000000050000000000000000
NextProposalId = 0000001000000001000000010000000f0000000f4e65787450726f706f73616c49640000
UpgradeHistory = 0000001000000001000000010000000f00000007486973746f727900000000
";

#[test]
fn datakey_fingerprints_are_pinned() {
    let env = Env::default();
    let actual_data_keys = render(&data_key_fingerprints(&env));
    let actual_upgrade_keys = render(&upgrade_key_fingerprints(&env));
    // Printed so an intentional change can be copied back into EXPECTED.
    println!("---- DataKey fingerprints ----\n{actual_data_keys}------------------------------");
    println!("---- UpgradeKey fingerprints ----\n{actual_upgrade_keys}------------------------------");
    assert_eq!(
        actual_data_keys, EXPECTED_DATA_KEYS,
        "A DataKey encoding changed — a storage key moved and existing ledger \
         entries would be orphaned. If this change is intentional, review the \
         diff and update EXPECTED_DATA_KEYS."
    );
    assert_eq!(
        actual_upgrade_keys, EXPECTED_UPGRADE_KEYS,
        "An UpgradeKey encoding changed — a storage key moved and existing ledger \
         entries would be orphaned. If this change is intentional, review the \
         diff and update EXPECTED_UPGRADE_KEYS."
    );
}

/// Sanity: no two variants share a fingerprint (which would alias their storage).
#[test]
fn datakey_fingerprints_are_unique() {
    let env = Env::default();
    let fps = data_key_fingerprints(&env);
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

/// Sanity: no two UpgradeKey variants share a fingerprint (which would alias their storage).
#[test]
fn upgrade_key_fingerprints_are_unique() {
    let env = Env::default();
    let fps = upgrade_key_fingerprints(&env);
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
