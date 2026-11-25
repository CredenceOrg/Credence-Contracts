#![cfg(test)]

use credence_arbitration::status::DisputeStatus;
use credence_arbitration::{CredenceArbitrationClient, CredenceArbitrationClient as ArbClient};
use credence_bond::CredenceBondClient;
use credence_registry::{CredenceRegistry, CredenceRegistryClient, IFACE_CREDENCE_BOND_V1};
use credence_treasury::{CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _, LedgerInfo};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Symbol, TryFromVal, Val,
    Vec as SorobanVec,
};

const ONE_TOKEN: i128 = 10_000_000;
const BOND_AMOUNT: i128 = 10 * ONE_TOKEN;
const SLASH_AMOUNT: i128 = 4 * ONE_TOKEN;
const VOTING_WINDOW: u64 = 3_600;
const BOND_DURATION: u64 = 86_400;
const SLASH_OUTCOME: u32 = 1;

#[contracttype]
#[derive(Clone)]
struct OrchestratorConfig {
    admin: Address,
    arbitration: Address,
    bond: Address,
    treasury: Address,
    identity: Address,
    slash_amount: i128,
    expected_outcome: u32,
}

#[contracttype]
enum OrchestratorKey {
    Config,
    SlashPending,
}

#[contract]
struct LifecycleOrchestrator;

#[contractimpl]
impl LifecycleOrchestrator {
    pub fn initialize(e: Env, config: OrchestratorConfig) {
        assert!(
            !e.storage().instance().has(&OrchestratorKey::Config),
            "orchestrator already initialized"
        );
        e.storage()
            .instance()
            .set(&OrchestratorKey::Config, &config);
        e.storage()
            .instance()
            .set(&OrchestratorKey::SlashPending, &false);
    }

    pub fn resolve_and_slash(e: Env, dispute_id: u64) -> bool {
        let config: OrchestratorConfig = e
            .storage()
            .instance()
            .get(&OrchestratorKey::Config)
            .expect("orchestrator not initialized");

        let outcome = ArbClient::new(&e, &config.arbitration).resolve_dispute(&dispute_id);
        if outcome != config.expected_outcome {
            return false;
        }

        CredenceBondClient::new(&e, &config.bond).slash(
            &config.admin,
            &config.identity,
            &config.slash_amount,
        );
        e.storage()
            .instance()
            .set(&OrchestratorKey::SlashPending, &true);
        true
    }

    pub fn route_slashed_funds(e: Env) {
        let pending: bool = e
            .storage()
            .instance()
            .get(&OrchestratorKey::SlashPending)
            .unwrap_or(false);
        assert!(pending, "no resolved slash is awaiting treasury routing");

        let config: OrchestratorConfig = e
            .storage()
            .instance()
            .get(&OrchestratorKey::Config)
            .expect("orchestrator not initialized");

        CredenceTreasuryClient::new(&e, &config.treasury).receive_fee(
            &e.current_contract_address(),
            &config.slash_amount,
            &FundSource::SlashedFunds,
        );
        e.storage()
            .instance()
            .set(&OrchestratorKey::SlashPending, &false);
    }
}

struct Setup<'a> {
    bond: CredenceBondClient<'a>,
    treasury: CredenceTreasuryClient<'a>,
    arbitration: CredenceArbitrationClient<'a>,
    orchestrator: LifecycleOrchestratorClient<'a>,
    orchestrator_address: Address,
    token: Address,
    identity: Address,
    arbitrator: Address,
}

fn set_initial_ledger(e: &Env) {
    e.ledger().set(LedgerInfo {
        timestamp: 1_700_000_000,
        protocol_version: 22,
        sequence_number: 100,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1_000_000,
    });
}

fn advance(e: &Env, seconds: u64, ledgers: u32) {
    e.ledger().with_mut(|info| {
        info.timestamp += seconds;
        info.sequence_number += ledgers;
    });
}

fn setup(e: &Env) -> Setup<'_> {
    e.mock_all_auths_allowing_non_root_auth();
    set_initial_ledger(e);

    let admin = Address::generate(e);
    let identity = Address::generate(e);
    let arbitrator = Address::generate(e);

    let registry_id = e.register(CredenceRegistry, ());
    let registry = CredenceRegistryClient::new(e, &registry_id);
    registry.initialize(&admin);

    let bond_id = e.register(credence_bond::CredenceBond, ());
    let bond = CredenceBondClient::new(e, &bond_id);
    bond.initialize(&admin, &None);
    assert!(bond.supports_interface(&IFACE_CREDENCE_BOND_V1));

    let token = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let mut accepted = SorobanVec::new(e);
    accepted.push_back(token.clone());
    bond.set_accepted_tokens(&admin, &accepted);
    bond.set_token(&admin, &token);

    let treasury_id = e.register(credence_treasury::CredenceTreasury, ());
    let treasury = CredenceTreasuryClient::new(e, &treasury_id);
    treasury.initialize(&admin, &token);

    let arbitration_id = e.register(credence_arbitration::CredenceArbitration, ());
    let arbitration = ArbClient::new(e, &arbitration_id);
    arbitration.initialize(&admin);
    arbitration.register_arbitrator(&arbitrator, &10);

    let orchestrator_address = e.register(LifecycleOrchestrator, ());
    let orchestrator = LifecycleOrchestratorClient::new(e, &orchestrator_address);
    orchestrator.initialize(&OrchestratorConfig {
        admin: admin.clone(),
        arbitration: arbitration_id.clone(),
        bond: bond_id.clone(),
        treasury: treasury_id.clone(),
        identity: identity.clone(),
        slash_amount: SLASH_AMOUNT,
        expected_outcome: SLASH_OUTCOME,
    });

    bond.set_slash_treasury(&admin, &orchestrator_address);
    treasury.add_depositor(&orchestrator_address);

    let entry = registry.register(&identity, &bond_id, &false);
    assert!(entry.active);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond_id);
    assert_eq!(count_events(e, &registry.address, "identity_registered"), 1);
    assert_eq!(registry.get_bond_contract(&identity).bond_contract, bond_id);

    StellarAssetClient::new(e, &token).mint(&identity, &BOND_AMOUNT);
    TokenClient::new(e, &token).approve(&identity, &bond_id, &i128::MAX, &999_999_u32);

    Setup {
        bond,
        treasury,
        arbitration,
        orchestrator,
        orchestrator_address,
        token,
        identity,
        arbitrator,
    }
}

type TestEvent = (Address, SorobanVec<Val>, Val);

fn event_name(e: &Env, event: &TestEvent) -> Option<Symbol> {
    event
        .1
        .get(0)
        .and_then(|topic| Symbol::try_from_val(e, &topic).ok())
}

fn event_data(e: &Env, event: &TestEvent) -> SorobanVec<Val> {
    match SorobanVec::<Val>::try_from_val(e, &event.2) {
        Ok(values) => values,
        Err(_) => SorobanVec::from_slice(e, std::slice::from_ref(&event.2)),
    }
}

fn event_data_at(e: &Env, event: &TestEvent, index: u32) -> Val {
    event_data(e, event)
        .get(index)
        .unwrap_or_else(|| panic!("missing event data at index {index}"))
}

fn count_events(e: &Env, contract: &Address, name: &str) -> usize {
    let wanted = Symbol::new(e, name);
    e.events()
        .all()
        .iter()
        .filter(|event| event.0 == *contract && event_name(e, event) == Some(wanted.clone()))
        .count()
}

fn find_event(e: &Env, contract: &Address, name: &str) -> TestEvent {
    let wanted = Symbol::new(e, name);
    e.events()
        .all()
        .iter()
        .find(|event| event.0 == *contract && event_name(e, event) == Some(wanted.clone()))
        .unwrap_or_else(|| panic!("missing {name} event from {:?}", contract))
}

fn balances(e: &Env, token: &Address, addresses: &[Address; 4]) -> [i128; 4] {
    let client = TokenClient::new(e, token);
    std::array::from_fn(|index| client.balance(&addresses[index]))
}

fn assert_conserved(e: &Env, token: &Address, addresses: &[Address; 4], stage: &str) {
    let balances = balances(e, token, addresses);
    assert_eq!(
        balances.iter().sum::<i128>(),
        BOND_AMOUNT,
        "funds not conserved at {stage}: {balances:?}"
    );
}

#[test]
fn e2e_protocol_lifecycle_conserves_funds() {
    let e = Env::default();
    let setup = setup(&e);
    let addresses = [
        setup.identity.clone(),
        setup.bond.address.clone(),
        setup.orchestrator_address.clone(),
        setup.treasury.address.clone(),
    ];

    let bond = setup
        .bond
        .create_bond(&setup.identity, &BOND_AMOUNT, &BOND_DURATION, &false, &0);
    assert!(bond.active);
    assert_eq!(bond.bonded_amount, BOND_AMOUNT);
    assert_eq!(
        TokenClient::new(&e, &setup.token).balance(&setup.bond.address),
        BOND_AMOUNT
    );
    assert_conserved(&e, &setup.token, &addresses, "bond creation");

    advance(&e, 1, 1);
    let description = String::from_str(&e, "slash identity for invalid attestation");
    let dispute_id =
        setup
            .arbitration
            .create_dispute(&setup.identity, &description, &VOTING_WINDOW);
    setup
        .arbitration
        .vote(&setup.arbitrator, &dispute_id, &SLASH_OUTCOME);
    assert_eq!(setup.arbitration.get_tally(&dispute_id, &SLASH_OUTCOME), 10);
    assert_conserved(&e, &setup.token, &addresses, "dispute opened");

    advance(&e, VOTING_WINDOW + 1, 5);

    assert!(setup.orchestrator.resolve_and_slash(&dispute_id));

    let dispute_event = find_event(&e, &setup.arbitration.address, "dispute_resolved");
    assert_eq!(
        u32::try_from_val(&e, &event_data_at(&e, &dispute_event, 0)).unwrap(),
        SLASH_OUTCOME
    );
    assert_eq!(count_events(&e, &setup.bond.address, "bond_slashed_v2"), 1);

    let resolved = setup.arbitration.get_dispute(&dispute_id);
    assert_eq!(resolved.status, DisputeStatus::Resolved);
    assert_eq!(resolved.outcome, SLASH_OUTCOME);

    let slashed = setup.bond.describe_bond(&setup.identity).unwrap();
    assert_eq!(slashed.slashed_amount, SLASH_AMOUNT);
    assert_eq!(
        slashed.bonded_amount - slashed.slashed_amount,
        BOND_AMOUNT - SLASH_AMOUNT
    );
    assert_eq!(
        TokenClient::new(&e, &setup.token).balance(&setup.orchestrator_address),
        SLASH_AMOUNT
    );
    assert_conserved(&e, &setup.token, &addresses, "outcome-gated slash");

    setup.orchestrator.route_slashed_funds();

    let deposit = find_event(&e, &setup.treasury.address, "treasury_deposit");
    assert_eq!(
        i128::try_from_val(&e, &event_data_at(&e, &deposit, 0)).unwrap(),
        SLASH_AMOUNT
    );
    assert_eq!(
        count_events(&e, &setup.treasury.address, "treasury_deposit"),
        1
    );

    assert_eq!(
        TokenClient::new(&e, &setup.token).balance(&setup.treasury.address),
        SLASH_AMOUNT
    );
    assert_eq!(setup.treasury.get_balance(), SLASH_AMOUNT);
    assert_eq!(
        setup
            .treasury
            .get_balance_by_source(&FundSource::SlashedFunds),
        SLASH_AMOUNT
    );
    assert_conserved(&e, &setup.token, &addresses, "treasury routing");
}

#[test]
fn e2e_protocol_lifecycle_ignores_unconfigured_outcome() {
    let e = Env::default();
    let setup = setup(&e);
    let addresses = [
        setup.identity.clone(),
        setup.bond.address.clone(),
        setup.orchestrator_address.clone(),
        setup.treasury.address.clone(),
    ];

    setup
        .bond
        .create_bond(&setup.identity, &BOND_AMOUNT, &BOND_DURATION, &false, &0);
    advance(&e, 1, 1);

    let description = String::from_str(&e, "outcome without a configured slash action");
    let dispute_id =
        setup
            .arbitration
            .create_dispute(&setup.identity, &description, &VOTING_WINDOW);
    setup
        .arbitration
        .vote(&setup.arbitrator, &dispute_id, &(SLASH_OUTCOME + 1));
    advance(&e, VOTING_WINDOW + 1, 5);

    assert!(!setup.orchestrator.resolve_and_slash(&dispute_id));

    let dispute_event = find_event(&e, &setup.arbitration.address, "dispute_resolved");
    assert_eq!(
        u32::try_from_val(&e, &event_data_at(&e, &dispute_event, 0)).unwrap(),
        SLASH_OUTCOME + 1
    );
    assert_eq!(count_events(&e, &setup.bond.address, "bond_slashed_v2"), 0);
    assert_eq!(
        count_events(&e, &setup.treasury.address, "treasury_deposit"),
        0
    );
    assert_eq!(
        setup
            .bond
            .describe_bond(&setup.identity)
            .unwrap()
            .slashed_amount,
        0
    );
    assert_conserved(&e, &setup.token, &addresses, "unconfigured outcome");
}
