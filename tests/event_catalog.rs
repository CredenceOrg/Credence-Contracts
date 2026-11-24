use admin::{AdminContract, AdminContractClient, AdminRole};
use credence_arbitration::{CredenceArbitration, CredenceArbitrationClient};
use credence_bond::{
    claims::PendingClaim,
    events as bond_events,
    invariants::{BondDriftDetails, BondDriftKind},
    BondTier,
};
use credence_delegation::{CredenceDelegation, CredenceDelegationClient, DelegationType};
use credence_registry::{CredenceRegistry, CredenceRegistryClient};
use credence_treasury::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use timelock::{TimelockContract, TimelockContractClient};

use serde::{Deserialize, Serialize};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    xdr::ScVal,
    Address, Bytes, BytesN, Env, String as SorobanString, Symbol, TryFromVal,
};
use std::collections::BTreeMap;

const OWNERSHIP_TRANSFER_TIMELOCK: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSchema {
    pub topics: Vec<String>,
    pub data: Vec<String>,
}

pub type EventCatalog = BTreeMap<String, BTreeMap<String, EventSchema>>;

fn scval_type_name(sc: &ScVal) -> String {
    match sc {
        ScVal::Bool(_) => "bool".into(),
        ScVal::Void => "void".into(),
        ScVal::Error(_) => "Error".into(),
        ScVal::U32(_) => "u32".into(),
        ScVal::I32(_) => "i32".into(),
        ScVal::U64(_) => "u64".into(),
        ScVal::I64(_) => "i64".into(),
        ScVal::Timepoint(_) => "u64".into(),
        ScVal::Duration(_) => "u64".into(),
        ScVal::U128(_) => "u128".into(),
        ScVal::I128(_) => "i128".into(),
        ScVal::U256(_) => "u256".into(),
        ScVal::I256(_) => "i256".into(),
        ScVal::Bytes(_) => "Bytes".into(),
        ScVal::String(_) => "String".into(),
        ScVal::Symbol(_) => "Symbol".into(),
        ScVal::Vec(_) => "Vec".into(),
        ScVal::Map(_) => "Map".into(),
        ScVal::Address(_) => "Address".into(),
        ScVal::ContractInstance(_) => "ContractInstance".into(),
        ScVal::LedgerKeyContractInstance => "LedgerKeyContractInstance".into(),
        ScVal::LedgerKeyNonce(_) => "LedgerKeyNonce".into(),
    }
}

fn extract_data_types(data: &ScVal) -> Vec<String> {
    match data {
        ScVal::Void => Vec::new(),
        ScVal::Vec(Some(vec)) => vec.0.iter().map(scval_type_name).collect(),
        other => vec![scval_type_name(other)],
    }
}

fn extract_topics(env: &Env, topics: &soroban_sdk::Vec<soroban_sdk::Val>) -> (String, Vec<String>) {
    let mut topic_strings = Vec::new();
    let mut event_name = String::new();
    for (i, t) in topics.iter().enumerate() {
        let sc = ScVal::try_from_val(env, &t).expect("valid ScVal topic");
        if i == 0 {
            match &sc {
                ScVal::Symbol(sym) => {
                    let s = sym.to_string();
                    event_name = s.clone();
                    topic_strings.push(s);
                }
                ScVal::String(st) => {
                    let s = st.to_string();
                    event_name = s.clone();
                    topic_strings.push(s);
                }
                other => {
                    let s = scval_type_name(other);
                    event_name = s.clone();
                    topic_strings.push(s);
                }
            }
        } else {
            topic_strings.push(scval_type_name(&sc));
        }
    }
    (event_name, topic_strings)
}

fn collect_events(env: &Env, catalog: &mut BTreeMap<String, EventSchema>) {
    for (_contract, topics, data) in env.events().all().iter() {
        if topics.is_empty() {
            continue;
        }
        let (event_name, topic_types) = extract_topics(env, &topics);
        let sc_data = ScVal::try_from_val(env, &data).expect("valid ScVal data");
        let data_types = extract_data_types(&sc_data);
        catalog.insert(
            event_name,
            EventSchema {
                topics: topic_types,
                data: data_types,
            },
        );
    }
}

fn drive_admin_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let id = env.register(AdminContract, ());
    let client = AdminContractClient::new(env, &id);
    let super_admin = Address::generate(env);
    client.initialize(&super_admin, &1u32, &100u32);
    collect_events(env, &mut catalog);

    let subject = Address::generate(env);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    collect_events(env, &mut catalog);

    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    collect_events(env, &mut catalog);

    client.deactivate_admin(&super_admin, &subject);
    collect_events(env, &mut catalog);

    client.reactivate_admin(&super_admin, &subject);
    collect_events(env, &mut catalog);

    let now = env.ledger().timestamp();
    client.suspend_admin(&super_admin, &subject, &(now + 1_000));
    collect_events(env, &mut catalog);

    client.remove_admin(&super_admin, &subject);
    collect_events(env, &mut catalog);

    let signer = Address::generate(env);
    client.set_pause_signer(&super_admin, &signer, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&super_admin, &1u32);
    collect_events(env, &mut catalog);

    let pause_id = client.pause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&pause_id);
    collect_events(env, &mut catalog);

    let unpause_id = client.unpause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&unpause_id);
    collect_events(env, &mut catalog);

    let signer2 = Address::generate(env);
    client.set_pause_signer(&super_admin, &signer2, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&super_admin, &2u32);
    collect_events(env, &mut catalog);

    let pause_id2 = client.pause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.approve_pause_proposal(&signer2, &pause_id2);
    collect_events(env, &mut catalog);

    let heir = Address::generate(env);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    collect_events(env, &mut catalog);

    client.transfer_ownership(&super_admin, &heir);
    collect_events(env, &mut catalog);

    env.ledger()
        .with_mut(|l| l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&heir);
    collect_events(env, &mut catalog);

    catalog
}

fn drive_arbitration_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let admin = Address::generate(env);
    let arb1 = Address::generate(env);
    let arb2 = Address::generate(env);
    let creator = Address::generate(env);
    let id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(env, &id);
    client.initialize(&admin);

    client.register_arbitrator(&arb1, &100_i128);
    collect_events(env, &mut catalog);

    client.unregister_arbitrator(&arb1);
    collect_events(env, &mut catalog);

    client.register_arbitrator(&arb2, &100_i128);
    collect_events(env, &mut catalog);

    client.set_quorum(&admin, &50_i128, &1_u32);
    collect_events(env, &mut catalog);

    let dispute_id1 = client.create_dispute(
        &creator,
        &SorobanString::from_str(env, "dispute 1"),
        &3600_u64,
    );
    collect_events(env, &mut catalog);

    client.cancel_dispute(
        &creator,
        &dispute_id1,
        &Some(SorobanString::from_str(env, "cancel")),
    );
    collect_events(env, &mut catalog);

    let dispute_id2 = client.create_dispute(
        &creator,
        &SorobanString::from_str(env, "dispute 2"),
        &3600_u64,
    );
    client.vote(&arb2, &dispute_id2, &1_u32);
    collect_events(env, &mut catalog);

    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    client.resolve_dispute(&dispute_id2);
    collect_events(env, &mut catalog);

    client.archive_dispute(&admin, &dispute_id2);
    collect_events(env, &mut catalog);

    client.reopen_dispute(&admin, &dispute_id2, &3600_u64);
    collect_events(env, &mut catalog);

    let creator3 = Address::generate(env);
    let dispute_id3 = client.create_dispute(
        &creator3,
        &SorobanString::from_str(env, "dispute 3"),
        &100_u64,
    );
    env.ledger().set_timestamp(env.ledger().timestamp() + 101);
    client.resolve_dispute(&dispute_id3);
    collect_events(env, &mut catalog);

    let arb3 = Address::generate(env);
    let arb4 = Address::generate(env);
    let creator4 = Address::generate(env);
    client.register_arbitrator(&arb3, &100_i128);
    client.register_arbitrator(&arb4, &100_i128);
    let dispute_id4 = client.create_dispute(
        &creator4,
        &SorobanString::from_str(env, "dispute 4"),
        &100_u64,
    );
    client.vote(&arb3, &dispute_id4, &1_u32);
    client.vote(&arb4, &dispute_id4, &2_u32);
    env.ledger().set_timestamp(env.ledger().timestamp() + 101);
    client.resolve_dispute(&dispute_id4);
    collect_events(env, &mut catalog);

    let new_admin = Address::generate(env);
    client.transfer_admin(&new_admin);
    collect_events(env, &mut catalog);

    let signer = Address::generate(env);
    client.set_pause_signer(&new_admin, &signer, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&new_admin, &1_u32);
    collect_events(env, &mut catalog);

    let pause_id = client.pause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&pause_id);
    collect_events(env, &mut catalog);

    let unpause_id = client.unpause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&unpause_id);
    collect_events(env, &mut catalog);

    let signer2 = Address::generate(env);
    client.set_pause_signer(&new_admin, &signer2, &true);
    client.set_pause_threshold(&new_admin, &2_u32);
    let pause_id2 = client.pause(&signer).unwrap();
    client.approve_pause_proposal(&signer2, &pause_id2);
    collect_events(env, &mut catalog);

    catalog
}

fn drive_timelock_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let admin = Address::generate(env);
    let id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(env, &id);
    client.initialize(&admin);

    let op_hash = BytesN::from_array(env, &[1; 32]);
    let delay = 86_400u64;
    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    collect_events(env, &mut catalog);

    client.cancel_operation(&admin, &op_id);
    collect_events(env, &mut catalog);

    let op_hash2 = BytesN::from_array(env, &[2; 32]);
    let op_id2 = client.queue_operation(&admin, &op_hash2, &delay);
    let op2 = client.get_operation(&op_id2).unwrap();
    env.ledger().with_mut(|li| li.timestamp = op2.eta);
    client.execute_operation(&op_id2);
    collect_events(env, &mut catalog);

    let new_admin = Address::generate(env);
    client.propose_admin(&new_admin);
    collect_events(env, &mut catalog);

    client.cancel_admin_proposal();
    collect_events(env, &mut catalog);

    let new_admin2 = Address::generate(env);
    client.propose_admin(&new_admin2);
    let pending = client.get_pending_admin().unwrap();
    env.ledger().with_mut(|li| li.timestamp = pending.eta);
    client.accept_admin();
    collect_events(env, &mut catalog);

    catalog
}

#[soroban_sdk::contract]
pub struct MockBond;

#[soroban_sdk::contractimpl]
impl MockBond {
    pub fn get_contract_code_hash(e: Env) -> Bytes {
        Bytes::from_array(&e, &[1; 32])
    }
}

fn drive_registry_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let admin = Address::generate(env);
    let id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(env, &id);
    client.initialize(&admin);
    collect_events(env, &mut catalog);

    let identity = Address::generate(env);
    let bond_contract = Address::generate(env);
    client.register(&identity, &bond_contract, &true);
    collect_events(env, &mut catalog);

    client.deactivate(&identity);
    collect_events(env, &mut catalog);

    client.reactivate(&identity);
    collect_events(env, &mut catalog);

    client.remove(&identity);
    collect_events(env, &mut catalog);

    let code_hash = Bytes::from_array(env, &[1; 32]);
    client.set_bond_code_hash(&code_hash);
    collect_events(env, &mut catalog);

    let identity2 = Address::generate(env);
    let bond_contract2 = env.register(MockBond, ());
    client.register_trustless(&bond_contract2, &identity2);
    collect_events(env, &mut catalog);

    let new_admin = Address::generate(env);
    client.transfer_admin(&new_admin);
    collect_events(env, &mut catalog);

    let signer = Address::generate(env);
    client.set_pause_signer(&new_admin, &signer, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&new_admin, &1_u32);
    collect_events(env, &mut catalog);

    let pause_id = client.pause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&pause_id);
    collect_events(env, &mut catalog);

    let unpause_id = client.unpause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&unpause_id);
    collect_events(env, &mut catalog);

    let signer2 = Address::generate(env);
    client.set_pause_signer(&new_admin, &signer2, &true);
    client.set_pause_threshold(&new_admin, &2_u32);
    let pause_id2 = client.pause(&signer).unwrap();
    client.approve_pause_proposal(&signer2, &pause_id2);
    collect_events(env, &mut catalog);

    catalog
}

fn drive_delegation_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let admin = Address::generate(env);
    let id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(env, &id);
    client.initialize(&admin);

    let verifier_id = Address::generate(env);
    client.register_verifier(&admin, &1u32, &verifier_id);
    collect_events(env, &mut catalog);

    let owner = Address::generate(env);
    let delegate = Address::generate(env);
    let now = env.ledger().timestamp();
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &(now + 100),
        &0u64,
    );
    collect_events(env, &mut catalog);

    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1u64);
    collect_events(env, &mut catalog);

    env.ledger().set_timestamp(now + 1000);
    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
    collect_events(env, &mut catalog);

    client.invalidate_nonce_range(&owner, &10u64);
    collect_events(env, &mut catalog);

    let signer = Address::generate(env);
    client.set_pause_signer(&admin, &signer, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&admin, &1u32);
    collect_events(env, &mut catalog);

    let pause_id = client.pause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&pause_id);
    collect_events(env, &mut catalog);

    let unpause_id = client.unpause(&signer).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&unpause_id);
    collect_events(env, &mut catalog);

    let signer2 = Address::generate(env);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_threshold(&admin, &2u32);
    let pause_id2 = client.pause(&signer).unwrap();
    client.approve_pause_proposal(&signer2, &pause_id2);
    collect_events(env, &mut catalog);

    catalog
}

fn drive_treasury_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let id = env.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(env, &id);
    let initial_token = Address::generate(env);
    client.initialize(&admin, &initial_token);
    collect_events(env, &mut catalog);

    client.set_token(&admin, &token);
    collect_events(env, &mut catalog);

    let depositor = Address::generate(env);
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    stellar_client.mint(&depositor, &1_000_000_i128);
    stellar_client.mint(&id, &500_i128);

    client.add_depositor(&depositor);
    collect_events(env, &mut catalog);

    client.receive_fee(&depositor, &1000_i128, &FundSource::ProtocolFee);
    collect_events(env, &mut catalog);

    client.remove_depositor(&depositor);
    collect_events(env, &mut catalog);

    let signer1 = Address::generate(env);
    let signer2 = Address::generate(env);
    client.add_signer(&signer1);
    collect_events(env, &mut catalog);

    client.add_signer(&signer2);
    client.set_threshold(&2_u32);
    collect_events(env, &mut catalog);

    client.remove_signer(&signer1);
    collect_events(env, &mut catalog);

    client.add_signer(&signer1);
    client.set_threshold(&1_u32);

    let recipient = Address::generate(env);
    let prop_id = client.propose_withdrawal(&signer1, &recipient, &500_i128);
    collect_events(env, &mut catalog);

    client.approve_withdrawal(&signer1, &prop_id);
    collect_events(env, &mut catalog);

    client.execute_withdrawal(&prop_id, &450_i128);
    collect_events(env, &mut catalog);

    let dest = Address::generate(env);
    client.register_corridor(&admin, &dest);
    collect_events(env, &mut catalog);

    client.remove_corridor(&admin, &dest);
    collect_events(env, &mut catalog);

    client.set_min_liquidity(&admin, &100_i128);
    collect_events(env, &mut catalog);

    client.set_proposal_ttl(&admin, &3600_u64);
    collect_events(env, &mut catalog);

    let rescue_to = Address::generate(env);
    client.rescue_native(&admin, &rescue_to, &50_i128);
    collect_events(env, &mut catalog);

    let prop_id2 = client.propose_withdrawal(&signer1, &recipient, &100_i128);
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    let _ = client.try_approve_withdrawal(&signer1, &prop_id2);
    collect_events(env, &mut catalog);

    let new_admin = Address::generate(env);
    client.transfer_admin(&new_admin);
    collect_events(env, &mut catalog);

    let psigner = Address::generate(env);
    client.set_pause_signer(&new_admin, &psigner, &true);
    collect_events(env, &mut catalog);

    client.set_pause_threshold(&new_admin, &1_u32);
    collect_events(env, &mut catalog);

    let pause_id = client.pause(&psigner).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&pause_id);
    collect_events(env, &mut catalog);

    let unpause_id = client.unpause(&psigner).unwrap();
    collect_events(env, &mut catalog);

    client.execute_pause_proposal(&unpause_id);
    collect_events(env, &mut catalog);

    let psigner2 = Address::generate(env);
    client.set_pause_signer(&new_admin, &psigner2, &true);
    client.set_pause_threshold(&new_admin, &2_u32);
    let pause_id2 = client.pause(&psigner).unwrap();
    client.approve_pause_proposal(&psigner2, &pause_id2);
    collect_events(env, &mut catalog);

    catalog
}

#[soroban_sdk::contract]
struct BondEventDriver;

#[soroban_sdk::contractimpl]
impl BondEventDriver {
    pub fn drive(e: Env, a: Address, admin: Address) {
        let ts = e.ledger().timestamp();
        bond_events::emit_bond_created_v2(&e, &a, 1000i128, 3600u64, false, ts);
        bond_events::emit_bond_created(&e, &a, 1000i128, 3600u64, false);
        bond_events::emit_bond_increased_v2(&e, &a, 500i128, 1500i128, ts, true, BondTier::Silver);
        bond_events::emit_bond_increased(&e, &a, 500i128, 1500i128);
        bond_events::emit_bond_withdrawn_v2(&e, &a, 200i128, 800i128, ts, true, 10i128);
        bond_events::emit_bond_withdrawn(&e, &a, 200i128, 800i128);
        bond_events::emit_bond_slashed_v2(
            &e,
            &a,
            100i128,
            100i128,
            ts,
            &admin,
            SorobanString::from_str(&e, "slash reason"),
            true,
        );
        bond_events::emit_bond_slashed(&e, &a, 100i128, 100i128);
        bond_events::emit_tier_changed(&e, &a, BondTier::Gold);
        bond_events::emit_tier_changed_v2(&e, &a, BondTier::Bronze, BondTier::Silver, ts);

        let claim = PendingClaim {
            claim_id: 1,
            claim_type: credence_bond::claims::ClaimType::SlashingReward,
            amount: 50i128,
            created_at: ts,
            expires_at: ts + 1000,
            source_id: 1,
            metadata: Symbol::new(&e, "meta"),
            processed: false,
        };
        bond_events::emit_claim_added(&e, &a, &claim);

        let mut claim_types = soroban_sdk::Vec::new(&e);
        claim_types.push_back(credence_bond::claims::ClaimType::SlashingReward);
        let res = credence_bond::claims::ClaimResult {
            processed_count: 1,
            total_amount: 50i128,
            claim_types,
        };
        let proc_claims = soroban_sdk::Vec::new(&e);
        bond_events::emit_claims_processed(&e, &a, &res, &proc_claims);
        bond_events::emit_claims_expired(&e, &a, 1, 50i128);

        bond_events::emit_upgrade_auth_initialized(&e, &admin);
        bond_events::emit_upgrade_auth_granted(
            &e,
            &admin,
            &a,
            credence_bond::upgrade_auth::UpgradeRole::Upgrader,
        );
        bond_events::emit_upgrade_auth_revoked(&e, &admin, &a);
        bond_events::emit_upgrade_proposed(&e, &admin, 1u64, &a);
        bond_events::emit_upgrade_approved(&e, &admin, 1u64);
        bond_events::emit_upgrade_executed(&e, &admin, &a, Some(1u64));

        bond_events::emit_admin_transfer_started(&e, &admin, &a);
        bond_events::emit_admin_transfer_completed(&e, &admin, &a);
        bond_events::emit_admin_rotated(&e, &admin, &a);
        bond_events::emit_upgrade_admin_transfer_started(&e, &admin, &a);
        bond_events::emit_upgrade_admin_transfer_completed(&e, &admin, &a);
        bond_events::emit_upgrade_admin_transfer_cancelled(&e, &admin, &a);

        bond_events::emit_parameter_updated(
            &e,
            Symbol::new(&e, "param_key"),
            Symbol::new(&e, "category"),
            &admin,
            100i128,
            200i128,
        );

        let drift_details = BondDriftDetails {
            subject: a.clone(),
            kind: BondDriftKind::SlashedExceedsBonded,
            bonded_amount: 1000i128,
            slashed_amount: 0i128,
            attestation_count: 1u32,
            attestation_list_len: 1u32,
        };
        bond_events::emit_bond_drift_detected(&e, &drift_details);

        bond_events::emit_fee_config_updated(&e, &admin, Some(a.clone()), &a, 100u32, 200u32);
        bond_events::emit_bond_liquidated(
            &e,
            &a,
            500i128,
            Symbol::new(&e, "fully_slashed"),
            ts,
            &admin,
        );

        e.events().publish(
            (Symbol::new(&e, "batch_transfer"), admin.clone()),
            (1u32, 1000i128),
        );
        e.events()
            .publish((Symbol::new(&e, "attester_registered"),), a.clone());
        e.events()
            .publish((Symbol::new(&e, "attester_unregistered"),), a.clone());
        e.events().publish(
            (Symbol::new(&e, "attestation_added"), a.clone()),
            (1u64, admin.clone(), Bytes::from_array(&e, &[0; 32])),
        );
        e.events().publish(
            (Symbol::new(&e, "attestations_batch_added"), a.clone()),
            (soroban_sdk::Vec::<u64>::new(&e),),
        );
        e.events().publish(
            (Symbol::new(&e, "attestation_revoked"), a.clone()),
            (1u64, admin.clone()),
        );
        e.events().publish(
            (Symbol::new(&e, "early_exit_penalty"),),
            (a.clone(), 1000i128, 50i128, admin.clone()),
        );
        e.events().publish(
            (Symbol::new(&e, "early_exit_config_set"),),
            (admin.clone(), 500u32),
        );
        e.events().publish(
            (Symbol::new(&e, "liquidation_treasury_set"),),
            (admin.clone(),),
        );
        e.events()
            .publish((Symbol::new(&e, "slash_treasury_set"),), (admin.clone(),));
        e.events().publish(
            (Symbol::new(&e, "admin_transferred"),),
            (admin.clone(), a.clone()),
        );
        e.events()
            .publish((Symbol::new(&e, "pause_signer_set"), a.clone()), true);
        e.events()
            .publish((Symbol::new(&e, "pause_threshold_set"),), 1u32);
        e.events()
            .publish((Symbol::new(&e, "pause_proposed"), 1u64), 1u32);
        e.events()
            .publish((Symbol::new(&e, "pause_approved"), 1u64), a.clone());
        e.events().publish(
            (Symbol::new(&e, "paused"),),
            (Some(1u64), SorobanString::from_str(&e, "emergency")),
        );
        e.events()
            .publish((Symbol::new(&e, "unpaused"),), Some(1u64));
    }
}

fn drive_bond_events(env: &Env) -> BTreeMap<String, EventSchema> {
    let mut catalog = BTreeMap::new();
    env.mock_all_auths();
    let a = Address::generate(env);
    let admin = Address::generate(env);
    let id = env.register(BondEventDriver, ());
    let client = BondEventDriverClient::new(env, &id);
    client.drive(&a, &admin);
    collect_events(env, &mut catalog);
    catalog
}

fn generate_catalog(env: &Env) -> EventCatalog {
    let mut full = EventCatalog::new();
    full.insert("admin".into(), drive_admin_events(env));
    full.insert("arbitration".into(), drive_arbitration_events(env));
    full.insert("bond".into(), drive_bond_events(env));
    full.insert("delegation".into(), drive_delegation_events(env));
    full.insert("registry".into(), drive_registry_events(env));
    full.insert("timelock".into(), drive_timelock_events(env));
    full.insert("treasury".into(), drive_treasury_events(env));
    full
}

#[test]
fn test_event_catalog() {
    let env = Env::default();
    let current_catalog = generate_catalog(&env);

    let catalog_path = std::path::Path::new("docs/event-catalog.json");
    let update = std::env::var("UPDATE_EVENT_CATALOG").as_deref() == Ok("1");

    if update {
        let json =
            serde_json::to_string_pretty(&current_catalog).expect("serialize catalog") + "\n";
        std::fs::write(catalog_path, json).expect("write docs/event-catalog.json");
        println!("Successfully updated {}", catalog_path.display());
    } else {
        assert!(
            catalog_path.exists(),
            "Catalog fixture '{}' does not exist. Run with UPDATE_EVENT_CATALOG=1 to generate.",
            catalog_path.display()
        );
        let content = std::fs::read_to_string(catalog_path).expect("read docs/event-catalog.json");
        let expected_catalog: EventCatalog =
            serde_json::from_str(&content).expect("parse docs/event-catalog.json");

        if current_catalog != expected_catalog {
            let mut diffs = Vec::new();
            for (contract, events) in &current_catalog {
                match expected_catalog.get(contract) {
                    Some(expected_events) => {
                        for (ev, schema) in events {
                            match expected_events.get(ev) {
                                Some(exp_schema) => {
                                    if schema != exp_schema {
                                        diffs.push(format!(
                                            "[{contract}::{ev}] schema mismatch:\n  Actual:   {:?}\n  Expected: {:?}",
                                            schema, exp_schema
                                        ));
                                    }
                                }
                                None => diffs
                                    .push(format!("[{contract}::{ev}] unexpected event in code")),
                            }
                        }
                        for ev in expected_events.keys() {
                            if !events.contains_key(ev) {
                                diffs.push(format!("[{contract}::{ev}] missing event in code"));
                            }
                        }
                    }
                    None => diffs.push(format!("[{contract}] unexpected contract in code")),
                }
            }
            for contract in expected_catalog.keys() {
                if !current_catalog.contains_key(contract) {
                    diffs.push(format!("[{contract}] missing contract in code"));
                }
            }

            panic!(
                "Event catalog drift detected!\n\n{}\n\nTo update the golden fixture, run:\n  UPDATE_EVENT_CATALOG=1 cargo test --test event_catalog\n",
                diffs.join("\n")
            );
        }
    }
}
