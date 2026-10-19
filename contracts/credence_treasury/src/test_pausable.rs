use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    // Give admin some tokens so they can deposit
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &(i128::MAX / 2));

    (client, admin)
}

#[test]
fn test_pause_blocks_state_changes_but_allows_reads() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    // Read should still work
    assert_eq!(client.get_balance(), 0);

    // State changes should fail
    assert!(client
        .try_receive_fee(&admin, &100_i128, &FundSource::ProtocolFee)
        .is_err());

    let depositor = Address::generate(&e);
    assert!(client.try_add_depositor(&depositor).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    client.receive_fee(&admin, &100_i128, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 100);
}

#[test]
fn test_pause_multisig_flow() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    assert!(!client.is_paused());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    let pid2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid2);
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

#[test]
fn sanity_check_pause_state() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // ── All writable (non-pause) entrypoints are blocked while paused ──

    assert!(client
        .try_receive_fee(&admin, &100_i128, &FundSource::ProtocolFee)
        .is_err());

    let depositor = Address::generate(&e);
    assert!(client.try_add_depositor(&depositor).is_err());
    assert!(client.try_remove_depositor(&depositor).is_err());

    let signer = Address::generate(&e);
    assert!(client.try_add_signer(&signer).is_err());
    assert!(client.try_remove_signer(&signer).is_err());

    assert!(client.try_set_threshold(&1u32).is_err());

    assert!(client
        .try_propose_withdrawal(&admin, &admin, &100_i128)
        .is_err());
    assert!(client.try_approve_withdrawal(&admin, &0u64).is_err());
    assert!(client.try_execute_withdrawal(&0u64, &0_i128).is_err());

    let new_token = Address::generate(&e);
    assert!(client.try_set_token(&admin, &new_token).is_err());
    assert!(client.try_set_min_liquidity(&admin, &100_i128).is_err());
    assert!(client.try_set_proposal_ttl(&admin, &1000_u64).is_err());
    assert!(client
        .try_rescue_native(&admin, &admin, &100_i128)
        .is_err());

    // ── Pause-system entrypoints remain accessible while paused ──
    let s1 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    assert!(client.is_paused());

    // ── Reads are unaffected ──
    assert_eq!(client.get_balance(), 0);

    // ── Happy path: unpause then write succeeds ──
    client.unpause(&admin);
    assert!(!client.is_paused());

    client.receive_fee(&admin, &100_i128, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 100);
}

#[test]
fn test_execute_requires_threshold() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    assert!(client.try_execute_pause_proposal(&pid).is_err());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}
