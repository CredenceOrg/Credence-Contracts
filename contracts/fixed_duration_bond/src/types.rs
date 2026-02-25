use soroban_sdk::{contracttype, Address};

// ─── Bond state ────────────────────────────────────────────────────────────

/// A single fixed-duration USDC bond owned by one address.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FixedBond {
    /// The address that locked the funds.
    pub owner: Address,
    /// Net bonded amount (after creation fee, if any).
    pub amount: i128,
    /// Ledger timestamp at the moment the bond was created.
    pub bond_start: u64,
    /// Lock period in seconds.
    pub bond_duration: u64,
    /// Pre-computed expiry: `bond_start + bond_duration`.
    pub bond_expiry: u64,
    /// Early-exit penalty in basis points (0 = disabled for this bond).
    pub penalty_bps: u32,
    /// false once the bond has been withdrawn.
    pub active: bool,
}

// ─── Fee configuration ─────────────────────────────────────────────────────

/// Optional fee charged at bond creation time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeConfig {
    /// Address that receives the creation fee.
    pub treasury: Address,
    /// Fee in basis points (100 bps = 1 %).
    pub fee_bps: u32,
}

// ─── Storage keys ──────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Contract admin address.
    Admin,
    /// USDC / Stellar asset token address.
    Token,
    /// Optional bond-creation fee config (FeeConfig).
    FeeConfig,
    /// Default early-exit penalty in basis points.
    PenaltyBps,
    /// Per-owner active bond.
    Bond(Address),
    /// Accrued creation fees held in the contract, in strobes/units.
    AccruedFees,
}
