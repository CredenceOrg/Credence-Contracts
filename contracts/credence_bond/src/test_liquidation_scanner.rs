use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{liquidation_scanner, CredenceBond, DataKey, IdentityBond};

fn set_bond(e: &Env, identity: &Address, bonded: i128, slashed: i128, active: bool) {
	let bond = IdentityBond {
		identity: identity.clone(),
		bonded_amount: bonded,
		bond_start: 0,
		bond_duration: 0,
		slashed_amount: slashed,
		active,
		is_rolling: false,
		withdrawal_requested_at: 0,
		notice_period_duration: 0,
	};

	e.storage().instance().set(&DataKey::Bond, &bond);
}

fn register_contract(env: &Env) -> Address {
	env.register(CredenceBond, ())
}

#[test]
fn test_register_and_deregister_registry() {
	let e = Env::default();
	e.mock_all_auths();

	let contract_id = register_contract(&e);
	let a1 = Address::generate(&e);
	let a2 = Address::generate(&e);

	e.as_contract(&contract_id, || {
		liquidation_scanner::register_bond_holder(&e, &a1);
		liquidation_scanner::register_bond_holder(&e, &a2);
		liquidation_scanner::register_bond_holder(&e, &a1);

		assert_eq!(liquidation_scanner::get_registry_size(&e), 2);

		liquidation_scanner::deregister_bond_holder(&e, &a1);
		assert_eq!(liquidation_scanner::get_registry_size(&e), 1);

		liquidation_scanner::deregister_bond_holder(&e, &a1);
		assert_eq!(liquidation_scanner::get_registry_size(&e), 1);
	});
}

#[test]
fn test_scan_liquidation_candidates_paginates() {
	let e = Env::default();
	e.mock_all_auths();

	let contract_id = register_contract(&e);
	let keeper = Address::generate(&e);
	let a1 = Address::generate(&e);
	let a2 = Address::generate(&e);
	let a3 = Address::generate(&e);

	e.as_contract(&contract_id, || {
		liquidation_scanner::register_bond_holder(&e, &a1);
		liquidation_scanner::register_bond_holder(&e, &a2);
		liquidation_scanner::register_bond_holder(&e, &a3);

		set_bond(&e, &a1, 100, 60, true);

		let page1 = liquidation_scanner::scan_liquidation_candidates(&e, &keeper, 0, 2, 5000);
		assert_eq!(page1.candidates.len(), 2);
		assert_eq!(page1.next_cursor, 2);
		assert_eq!(page1.done, false);
		assert_eq!(page1.registry_size, 3);
		assert_eq!(liquidation_scanner::get_keeper_cursor(&e, &keeper), 2);

	});

	e.as_contract(&contract_id, || {
		let page2 = liquidation_scanner::scan_liquidation_candidates(&e, &keeper, 2, 2, 5000);
		assert_eq!(page2.candidates.len(), 1);
		assert_eq!(page2.next_cursor, 0);
		assert_eq!(page2.done, true);
		assert_eq!(page2.registry_size, 3);
		assert_eq!(liquidation_scanner::get_keeper_cursor(&e, &keeper), 0);
	});
}

#[test]
#[should_panic(expected = "cursor out of range")]
fn test_scan_cursor_out_of_range_panics() {
	let e = Env::default();
	e.mock_all_auths();

	let contract_id = register_contract(&e);
	let keeper = Address::generate(&e);
	let a1 = Address::generate(&e);

	e.as_contract(&contract_id, || {
		liquidation_scanner::register_bond_holder(&e, &a1);

		liquidation_scanner::scan_liquidation_candidates(&e, &keeper, 2, 1, 5000);
	});
}

#[test]
fn test_scan_skips_inactive_or_zero_bond() {
	let e = Env::default();
	e.mock_all_auths();

	let contract_id = register_contract(&e);
	let keeper = Address::generate(&e);
	let a1 = Address::generate(&e);

	e.as_contract(&contract_id, || {
		liquidation_scanner::register_bond_holder(&e, &a1);

		set_bond(&e, &a1, 0, 0, true);
		let result = liquidation_scanner::scan_liquidation_candidates(&e, &keeper, 0, 1, 1);
		assert_eq!(result.candidates.len(), 0);
	});

	e.as_contract(&contract_id, || {
		set_bond(&e, &a1, 100, 10, false);
		let result = liquidation_scanner::scan_liquidation_candidates(&e, &keeper, 0, 1, 1);
		assert_eq!(result.candidates.len(), 0);
	});
}

#[test]
#[should_panic(expected = "keeper cursor: invalid advance")]
fn test_advance_keeper_cursor_invalid_panics() {
	let e = Env::default();
	e.mock_all_auths();

	let contract_id = register_contract(&e);
	let keeper = Address::generate(&e);
	let a1 = Address::generate(&e);

	e.as_contract(&contract_id, || {
		liquidation_scanner::register_bond_holder(&e, &a1);

		liquidation_scanner::advance_keeper_cursor(&e, &keeper, 5);
	});
}
