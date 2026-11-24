//! TTL regression tests for bounty-escrow persistent entries (issue #1720).
//!
//! `bump_persistent` runs on lock, release, refund and reads, but nothing
//! proved the extension actually happened. These tests follow the pattern in
//! `credence_delegation/src/test_delegation_ttl.rs`:
//!
//! 1. Create an escrow, then advance the ledger until the record's remaining
//!    TTL falls below `PERSISTENT_TTL_THRESHOLD`.
//! 2. Call the entry point under test.
//! 3. Assert `storage().persistent().get_ttl` is back at
//!    `PERSISTENT_TTL_EXTEND_TO`.
//!
//! Without the bump the remaining TTL stays at its decayed value, so each test
//! below fails when the corresponding `bump_persistent` call is removed.
//!
//! A minimal in-test token stands in for the Stellar Asset Contract: the SAC's
//! balance entries get the default minimum persistent TTL (a few thousand
//! ledgers), so they would be archived during the multi-week advance and the
//! transfer would fail before the escrow TTL assertion could run.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

const DAY_SECONDS: u64 = 86_400;
/// Enough to push a freshly bumped 30-day record below the 7-day bump
/// threshold while leaving the record itself live.
const DECAY_SECONDS: u64 = 25 * DAY_SECONDS;

// ---------------------------------------------------------------------------
// Minimal token used by the fixture
// ---------------------------------------------------------------------------

#[contract]
pub struct MockToken;

fn balance_key(env: &Env, id: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "balance"), id.clone())
}

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = balance_key(&env, &to);
        let balance: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(balance + amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let from_key = balance_key(&env, &from);
        let from_balance: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let to_key = balance_key(&env, &to);
        let to_balance: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&from_key, &(from_balance - amount));
        env.storage()
            .instance()
            .set(&to_key, &(to_balance + amount));
    }
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    env: Env,
    client: BountyEscrowClient<'static>,
    token_id: Address,
    depositor: Address,
    beneficiary: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let token_id = env.register(MockToken, ());
    MockTokenClient::new(&env, &token_id).mint(&depositor, &100_000);

    let contract_id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &token_id);

    Fixture {
        env,
        client,
        token_id,
        depositor,
        beneficiary,
    }
}

/// Advance the ledger by `secs` seconds. The escrow and token instances are
/// extended first so both contracts stay callable across the advance.
fn advance_ledger(f: &Fixture, secs: u64) {
    let ledgers = (secs / 5) as u32;
    for contract_id in [f.client.address.clone(), f.token_id.clone()].iter() {
        f.env.as_contract(contract_id, || {
            f.env
                .storage()
                .instance()
                .extend_ttl(ledgers + 17_280, ledgers + 17_280);
        });
    }
    f.env.ledger().with_mut(|info| {
        info.timestamp += secs;
        info.sequence_number += ledgers;
    });
}

/// Remaining TTL (in ledgers) of an escrow record.
fn escrow_ttl(f: &Fixture, id: u64) -> u32 {
    f.env.as_contract(&f.client.address, || {
        f.env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Escrow(id))
    })
}

/// Assert the record's TTL was clamped by ledger advancement and then restored
/// by the call under test.
fn assert_bumped_from(f: &Fixture, id: u64, before: u32) {
    assert!(
        before < PERSISTENT_TTL_EXTEND_TO,
        "fixture must decay TTL below {PERSISTENT_TTL_EXTEND_TO}, got {before}"
    );
    let after = escrow_ttl(f, id);
    assert!(
        after >= PERSISTENT_TTL_EXTEND_TO,
        "TTL was not bumped: {after} < {PERSISTENT_TTL_EXTEND_TO}"
    );
}

// ---------------------------------------------------------------------------
// lock
// ---------------------------------------------------------------------------

#[test]
fn test_lock_bumps_escrow_ttl() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 60 * DAY_SECONDS;

    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &expires_at);

    let ttl = escrow_ttl(&f, id);
    assert!(
        ttl >= PERSISTENT_TTL_EXTEND_TO,
        "lock must bump escrow TTL: {ttl} < {PERSISTENT_TTL_EXTEND_TO}"
    );
}

// ---------------------------------------------------------------------------
// release
// ---------------------------------------------------------------------------

#[test]
fn test_release_bumps_escrow_ttl() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 10 * DAY_SECONDS;
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &expires_at);

    advance_ledger(&f, DECAY_SECONDS);
    let before = escrow_ttl(&f, id);

    f.client.release(&f.depositor, &id);

    assert_eq!(
        f.client.get_escrow(&id).unwrap().status,
        EscrowStatus::Released
    );
    assert_bumped_from(&f, id, before);
}

// ---------------------------------------------------------------------------
// refund
// ---------------------------------------------------------------------------

#[test]
fn test_refund_bumps_escrow_ttl() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 10 * DAY_SECONDS;
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &expires_at);

    advance_ledger(&f, DECAY_SECONDS);
    let before = escrow_ttl(&f, id);

    f.client.refund(&id, &1_000);

    assert_eq!(
        f.client.get_escrow(&id).unwrap().status,
        EscrowStatus::Refunded
    );
    assert_bumped_from(&f, id, before);
}

// ---------------------------------------------------------------------------
// get_escrow
// ---------------------------------------------------------------------------

#[test]
fn test_get_escrow_bumps_escrow_ttl() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 60 * DAY_SECONDS;
    let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &expires_at);

    advance_ledger(&f, DECAY_SECONDS);
    let before = escrow_ttl(&f, id);

    let escrow = f.client.get_escrow(&id).unwrap();

    assert_eq!(escrow.amount, 1_000);
    assert_eq!(escrow.status, EscrowStatus::Locked);
    assert_bumped_from(&f, id, before);
}
