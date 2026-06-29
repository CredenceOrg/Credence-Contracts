use crate::{DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Env, IntoVal, Symbol, Val, Vec};

pub fn slash_bond(e: &Env, admin: &Address, slash_amount: i128) -> IdentityBond {
    admin.require_auth();
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
    if stored_admin != *admin {
        panic_with_error!(e, ContractError::NotAdmin);
    }

    let bond_key = DataKey::Bond;
    let bond: IdentityBond = e
        .storage()
        .instance()
        .get(&bond_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

    if !bond.active {
        panic_with_error!(e, ContractError::BondNotActive);
    }

    let new_slashed = bond.slashed_amount + slash_amount;
    if new_slashed > bond.bonded_amount {
        panic_with_error!(e, ContractError::SlashExceedsBond);
    }

    let updated = IdentityBond {
        identity: bond.identity.clone(),
        bonded_amount: bond.bonded_amount,
        bond_start: bond.bond_start,
        bond_duration: bond.bond_duration,
        slashed_amount: new_slashed,
        active: bond.active,
        is_rolling: bond.is_rolling,
        withdrawal_requested_at: bond.withdrawal_requested_at,
        notice_period_duration: bond.notice_period_duration,
    };
    e.storage().instance().set(&bond_key, &updated);
    e.events().publish(
        (Symbol::new(e, "bond_slashed"),),
        (bond.identity.clone(), slash_amount, new_slashed),
    );

    if let Some(cb_addr) = e
        .storage()
        .instance()
        .get::<_, Address>(&Symbol::new(e, "callback"))
    {
        let fn_name = Symbol::new(e, "on_slash");
        let args: Vec<Val> = Vec::from_array(e, [slash_amount.into_val(e)]);
        e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
    }

    updated
}
