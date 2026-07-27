use soroban_sdk::{contracttype, Address, Env};

pub const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Represents a registry entry mapping an identity to their bond contract
#[contracttype]
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    /// The identity address
    pub identity: Address,
    /// The bond contract address for this identity
    pub bond_contract: Address,
    /// Timestamp when this entry was registered
    pub registered_at: u64,
    /// Whether this registration is currently active
    pub active: bool,
}

/// Storage keys for the registry contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u32),
    PauseApproval(u32, Address),
    PauseApprovalCount(u32),
    IdentityToBond(Address),
    BondToIdentity(Address),
    RegisteredIdentities,
    AllowNonInterface(Address),
    BondCodeHash,
}

pub(crate) fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}
