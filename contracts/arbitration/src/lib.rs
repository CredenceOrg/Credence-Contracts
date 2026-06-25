#![no_std]
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, symbol_short, Address, Env,
};

// 1. Define a strict, typed contract error instead of using heap-allocated strings
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ContractError {
    ArbitratorHasNoBond = 1,
}

// 2. Declare an interface block to allow type-safe cross-contract calling to credence_bond
#[contractclient(name = "CredenceBondClient")]
pub trait CredenceBondInterface {
    fn get_bond_weight(env: Env, identity: Address) -> u32;
}

#[contract]
pub struct Arbitration;

pub struct ArbitratorRegistration {
    pub address: Address,
    pub weight_snapshot: Option<u32>,
}

#[contractimpl]
impl Arbitration {
    pub fn register_arbitrator(_env: Env, _arbitrator: Address) -> bool {
        true
    }

    // Helper method executing a type-safe guest cross-contract call invocation sequence
    fn derive_weight_from_bond(env: Env, arbitrator: Address, bond_contract: Address) -> u32 {
        let client = CredenceBondClient::new(&env, &bond_contract);
        client.get_bond_weight(&arbitrator)
    }

    pub fn submit_vote(
        env: Env,
        dispute_id: u64,
        arbitrator: Address,
        decision: bool,
        bond_contract: Address,
    ) -> Result<(), ContractError> {
        let weight = Self::derive_weight_from_bond(env.clone(), arbitrator.clone(), bond_contract);

        if weight == 0 {
            return Err(ContractError::ArbitratorHasNoBond);
        }

        // Emit arbitration telemetry state update event
        env.events().publish(
            (symbol_short!("vote"), dispute_id),
            (arbitrator, decision, weight),
        );

        Ok(())
    }
}
