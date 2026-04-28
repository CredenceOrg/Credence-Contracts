use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use crate::access_control::{
	add_verifier_role, get_admin, is_admin, is_verifier, remove_verifier_role,
	require_admin, require_admin_or_verifier, require_identity_owner, require_verifier,
};
use crate::CredenceBond;

fn set_admin(e: &Env, admin: &Address) {
	e.storage()
		.instance()
		.set(&Symbol::new(e, "admin"), admin);
}

fn with_contract_env<F: FnOnce(&Env)>(env: &Env, f: F) {
	let contract_id = env.register(CredenceBond, ());
	env.as_contract(&contract_id, || f(env));
}

#[test]
fn test_require_admin_success() {
	let e = Env::default();
	let admin = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);
		require_admin(env, &admin);
	});
}

#[test]
#[should_panic(expected = "not admin")]
fn test_require_admin_rejects_non_admin() {
	let e = Env::default();
	let admin = Address::generate(&e);
	let caller = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);
		require_admin(env, &caller);
	});
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_require_admin_not_initialized() {
	let e = Env::default();
	let caller = Address::generate(&e);

	with_contract_env(&e, |env| {
		require_admin(env, &caller);
	});
}

#[test]
fn test_verifier_role_lifecycle() {
	let e = Env::default();
	let admin = Address::generate(&e);
	let verifier = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);

		add_verifier_role(env, &admin, &verifier);
		assert!(is_verifier(env, &verifier));

		require_verifier(env, &verifier);

		remove_verifier_role(env, &admin, &verifier);
		assert!(!is_verifier(env, &verifier));
	});
}

#[test]
#[should_panic(expected = "not verifier")]
fn test_require_verifier_rejects_non_verifier() {
	let e = Env::default();
	let caller = Address::generate(&e);

	with_contract_env(&e, |env| {
		require_verifier(env, &caller);
	});
}

#[test]
fn test_admin_or_verifier_allows_admin() {
	let e = Env::default();
	let admin = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);
		require_admin_or_verifier(env, &admin);
	});
}

#[test]
fn test_admin_or_verifier_allows_verifier() {
	let e = Env::default();
	let admin = Address::generate(&e);
	let verifier = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);
		add_verifier_role(env, &admin, &verifier);
		require_admin_or_verifier(env, &verifier);
	});
}

#[test]
#[should_panic(expected = "not authorized")]
fn test_admin_or_verifier_rejects_unprivileged() {
	let e = Env::default();
	let caller = Address::generate(&e);

	with_contract_env(&e, |env| {
		require_admin_or_verifier(env, &caller);
	});
}

#[test]
fn test_identity_owner_check() {
	let e = Env::default();
	let owner = Address::generate(&e);

	with_contract_env(&e, |env| {
		require_identity_owner(env, &owner, &owner);
	});
}

#[test]
#[should_panic(expected = "not identity owner")]
fn test_identity_owner_rejects_other() {
	let e = Env::default();
	let owner = Address::generate(&e);
	let caller = Address::generate(&e);

	with_contract_env(&e, |env| {
		require_identity_owner(env, &caller, &owner);
	});
}

#[test]
fn test_is_admin_and_get_admin() {
	let e = Env::default();
	let admin = Address::generate(&e);
	let other = Address::generate(&e);

	with_contract_env(&e, |env| {
		set_admin(env, &admin);

		assert!(is_admin(env, &admin));
		assert!(!is_admin(env, &other));
		assert_eq!(get_admin(env), admin);
	});
}
