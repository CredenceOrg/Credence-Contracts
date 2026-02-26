use credence_bond::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup(env: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let user = Address::generate(env);
    let attacker = Address::generate(env);

    client.initialize(&admin);

    // Register token
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.set_token(&admin, &token_id);

    // 🔹 Use StellarAssetClient for minting
    let asset = soroban_sdk::token::StellarAssetClient::new(env, &token_id);
    asset.mint(&user, &10_000_i128);

    // 🔹 Use TokenClient for approval
    let token = soroban_sdk::token::TokenClient::new(env, &token_id);
    token.approve(&user, &contract_id, &10_000_i128, &0_u32);

    (client, admin, user, attacker)
}

#[test]
#[should_panic]
fn unauthorized_cannot_add_attestation() {
    let env = Env::default();
    let (client, _admin, user, attacker) = setup(&env);

    let fake = String::from_str(&env, "fake");
    let nonce = client.get_nonce(&attacker);

    client.add_attestation(&attacker, &user, &fake, &nonce);
}

#[test]
fn authorized_attester_can_add_attestation() {
    let env = Env::default();
    let (client, _admin, user, attacker) = setup(&env);

    client.register_attester(&attacker);

    let valid = String::from_str(&env, "valid");
    let nonce = client.get_nonce(&attacker);
    let att = client.add_attestation(&attacker, &user, &valid, &nonce);

    assert_eq!(att.identity, user);
}

#[test]
#[should_panic]
fn wrong_attester_cannot_revoke() {
    let env = Env::default();
    let (client, _admin, user, attacker) = setup(&env);

    client.register_attester(&attacker);

    let valid = String::from_str(&env, "valid");
    let nonce = client.get_nonce(&attacker);
    let att = client.add_attestation(&attacker, &user, &valid, &nonce);

    let other = Address::generate(&env);
    let other_nonce = client.get_nonce(&other);

    client.revoke_attestation(&other, &att.id, &other_nonce);
}

#[test]
fn owner_can_withdraw_bond() {
    let env = Env::default();
    let (client, _admin, user, _) = setup(&env);

    client.create_bond(&user, &1000_i128, &86400_u64, &false, &0_u64);

    // advance time past lock-up period
    env.ledger().with_mut(|l| {
        l.timestamp += 86401;
    });

    let bond = client.withdraw_bond(&1000_i128);
    assert_eq!(bond.bonded_amount, 0);
}
