#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, Symbol,
};

// ─── Constants ──────────────────────────────────────────────────────────────

/// Minimum bond duration in seconds (1 second).
pub const MIN_BOND_DURATION: u64 = 1;

/// Maximum bond duration in seconds (365 days = 31_536_000 seconds).
pub const MAX_BOND_DURATION: u64 = 31_536_000;

/// Fixed-point denominator for basis-point calculations.
const BPS_DENOMINATOR: i128 = 10_000;

// ─── Storage Key ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Token,
    Bond(Address),
    FeeConfig,
    PenaltyConfig,
    AccumulatedFees,
}

// ─── Configuration Types ────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct FeeConfig {
    treasury: Address,
    fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PenaltyConfig {
    /// Treasury that receives early-exit penalties.
    treasury: Address,
    /// Default early-exit penalty in basis points (0 = disabled).
    base_penalty_bps: u32,
}

// ─── Bond Type ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedBond {
    pub owner: Address,
    /// Net bonded amount (after any creation fee deduction).
    pub amount: i128,
    /// Ledger timestamp at bond creation.
    pub bond_start: u64,
    /// Lock period in seconds.
    pub bond_duration: u64,
    /// Pre-computed expiry: `bond_start + bond_duration`.
    pub bond_expiry: u64,
    /// Early-exit penalty in basis points (0 = early exit disabled for this bond).
    pub penalty_bps: u32,
    /// `false` once withdrawn.
    pub active: bool,
}

// ─── Duration Validation ────────────────────────────────────────────────────

/// Validate that `duration` is within the allowed range and does not cause
/// timestamp overflow when added to the current ledger timestamp.
///
/// # Security — Two layers of protection
///
/// 1. **Bounds check**: `duration` must be in `[MIN_BOND_DURATION, MAX_BOND_DURATION]`.
/// 2. **Overflow check**: `now + duration` must not overflow `u64`. This is
///    critical at extreme ledger times (near `u64::MAX`) where even a
///    valid duration could cause expiry timestamp wraparound.
///
/// Returns the pre-computed `bond_expiry` on success.
fn validate_duration_and_expiry(e: &Env, duration: u64) -> u64 {
    if duration < MIN_BOND_DURATION {
        panic_with_error!(e, ContractError::InvalidBondDuration);
    }
    if duration > MAX_BOND_DURATION {
        panic_with_error!(e, ContractError::InvalidBondDuration);
    }

    let now = e.ledger().timestamp();
    now.checked_add(duration)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow))
}

// ─── Validation Helpers ─────────────────────────────────────────────────────

fn require_positive_amount(e: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(e, ContractError::InvalidBondAmount);
    }
}

fn require_no_active_bond(e: &Env, owner: &Address) {
    let key = DataKey::Bond(owner.clone());
    if e.storage().instance().has(&key) {
        let bond: FixedBond = e.storage().instance().get(&key).unwrap();
        if bond.active {
            panic_with_error!(e, ContractError::BondAlreadyExists);
        }
    }
}

fn require_valid_penalty_bps(e: &Env, bps: u32) {
    if bps > BPS_DENOMINATOR as u32 {
        panic_with_error!(e, ContractError::InvalidPenaltyBps);
    }
}

fn require_admin(e: &Env, caller: &Address) {
    credence_errors::require_admin!(e, caller, DataKey::Admin);
}

/// Load the token address from storage, panicking if not initialized.
fn load_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

/// Transfer tokens into the contract from `from`. Panics if token not set.
fn transfer_in(e: &Env, from: &Address, amount: i128) {
    let token_addr = load_token(e);
    let token_client = token::Client::new(e, &token_addr);
    token_client.transfer(from, &e.current_contract_address(), &amount);
}

/// Transfer tokens out of the contract to `to`. Panics if token not set.
fn transfer_out(e: &Env, to: &Address, amount: i128) {
    let token_addr = load_token(e);
    let token_client = token::Client::new(e, &token_addr);
    token_client.transfer(&e.current_contract_address(), to, &amount);
}

// ─── Contract ───────────────────────────────────────────────────────────────

#[contract]
pub struct FixedDurationBond;

#[contractimpl]
impl FixedDurationBond {
    /// One-time setup. Stores admin and token. Panics if called again.
    pub fn initialize(e: Env, admin: Address, token: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(e, ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Token, &token);
        e.events()
            .publish((Symbol::new(&e, "initialized"),), (admin, token));
    }

    /// Set an optional bond-creation fee. 0 = disabled.
    pub fn set_fee_config(e: Env, admin: Address, treasury: Address, fee_bps: u32) {
        admin.require_auth();
        require_admin(&e, &admin);
        require_valid_penalty_bps(&e, fee_bps);
        e.storage()
            .instance()
            .set(&DataKey::FeeConfig, &FeeConfig { treasury, fee_bps });
        e.events()
            .publish((Symbol::new(&e, "fee_config_set"),), fee_bps);
    }

    /// Set the default early-exit penalty for bonds created after this call.
    /// `base_penalty_bps` = 0 disables early exit.
    /// `treasury` receives the penalty on early exit.
    pub fn set_penalty_config(e: Env, admin: Address, treasury: Address, base_penalty_bps: u32) {
        admin.require_auth();
        require_admin(&e, &admin);
        require_valid_penalty_bps(&e, base_penalty_bps);
        e.storage().instance().set(
            &DataKey::PenaltyConfig,
            &PenaltyConfig {
                treasury,
                base_penalty_bps,
            },
        );
        e.events()
            .publish((Symbol::new(&e, "penalty_config_set"),), base_penalty_bps);
    }

    /// Transfer all accrued creation fees to `recipient`. Panics if no fees.
    pub fn collect_fees(e: Env, admin: Address, recipient: Address) -> i128 {
        admin.require_auth();
        require_admin(&e, &admin);

        let fees: i128 = e
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0);
        if fees <= 0 {
            panic!("no fees to collect");
        }

        // CEI: zero the accumulator first, then transfer.
        e.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &0_i128);
        transfer_out(&e, &recipient, fees);

        e.events().publish(
            (Symbol::new(&e, "fees_collected"),),
            (admin, recipient, fees),
        );
        fees
    }

    /// Lock `amount` USDC for `duration_secs`. One active bond per address.
    ///
    /// # CEI pattern
    ///
    /// 1. **Checks**: Validate inputs (amount, duration, no existing bond).
    /// 2. **Effects**: Compute bond state, update storage (accumulated fees).
    /// 3. **Interactions**: Transfer tokens from owner to contract.
    ///
    /// State changes happen before external calls so a re-entrant token cannot
    /// observe inconsistent state.
    pub fn create_bond(e: Env, owner: Address, amount: i128, duration_secs: u64) -> FixedBond {
        owner.require_auth();
        require_positive_amount(&e, amount);
        require_no_active_bond(&e, &owner);

        // Validate duration and compute expiry with overflow protection.
        let bond_expiry = validate_duration_and_expiry(&e, duration_secs);
        let bond_start = e.ledger().timestamp();

        // Snapshot penalty config at creation time.
        let penalty_cfg: Option<PenaltyConfig> =
            e.storage().instance().get(&DataKey::PenaltyConfig);
        let penalty_bps = penalty_cfg
            .as_ref()
            .map(|c| c.base_penalty_bps)
            .unwrap_or(0);

        // Compute creation fee deduction.
        let fee_cfg: Option<FeeConfig> = e.storage().instance().get(&DataKey::FeeConfig);
        let (fee, net_amount) = if let Some(ref cfg) = fee_cfg {
            if cfg.fee_bps > 0 {
                let fee = amount
                    .checked_mul(cfg.fee_bps as i128)
                    .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow))
                    .checked_div(BPS_DENOMINATOR)
                    .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
                let net = amount - fee;
                (fee, net)
            } else {
                (0, amount)
            }
        } else {
            (0, amount)
        };

        // Effects: write state BEFORE external calls (CEI).
        if fee > 0 {
            let accumulated: i128 = e
                .storage()
                .instance()
                .get(&DataKey::AccumulatedFees)
                .unwrap_or(0);
            e.storage()
                .instance()
                .set(&DataKey::AccumulatedFees, &(accumulated + fee));
        }

        let bond = FixedBond {
            owner: owner.clone(),
            amount: net_amount,
            bond_start,
            bond_duration: duration_secs,
            bond_expiry,
            penalty_bps,
            active: true,
        };
        e.storage()
            .instance()
            .set(&DataKey::Bond(owner.clone()), &bond);

        e.events().publish(
            (Symbol::new(&e, "bond_created"), owner.clone()),
            (net_amount, bond_expiry),
        );

        // Interactions: transfer tokens AFTER state is committed.
        transfer_in(&e, &owner, amount);

        bond
    }

    /// Withdraw full principal after lock period. Deactivates bond.
    pub fn withdraw(e: Env, owner: Address) -> FixedBond {
        owner.require_auth();
        let key = DataKey::Bond(owner.clone());
        let mut bond: FixedBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

        if !bond.active {
            panic_with_error!(e, ContractError::BondNotActive);
        }

        let now = e.ledger().timestamp();
        if now < bond.bond_expiry {
            panic!("lock period has not elapsed yet");
        }

        // CEI: deactivate bond before external transfer.
        bond.active = false;
        e.storage().instance().set(&key, &bond);

        let amount = bond.amount;
        transfer_out(&e, &owner, amount);

        e.events()
            .publish((Symbol::new(&e, "bond_withdrawn"), owner), amount);

        bond
    }

    /// Withdraw before lock period with penalty deducted.
    pub fn withdraw_early(e: Env, owner: Address) -> FixedBond {
        owner.require_auth();
        let key = DataKey::Bond(owner.clone());
        let mut bond: FixedBond = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));

        if !bond.active {
            panic_with_error!(e, ContractError::BondNotActive);
        }

        let now = e.ledger().timestamp();
        if now >= bond.bond_expiry {
            panic!("lock period has already elapsed; use withdraw");
        }

        if bond.penalty_bps == 0 {
            panic!("cannot exit early: penalty not configured");
        }

        // Snapshot amounts before deactivating.
        let gross = bond.amount;
        let penalty = gross
            .checked_mul(bond.penalty_bps as i128)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow))
            .checked_div(BPS_DENOMINATOR)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        let net_amount = gross
            .checked_sub(penalty)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Underflow));

        // Look up the penalty treasury from the PenaltyConfig.
        // (PenaltyConfig MUST be set if a bond has penalty_bps > 0.)
        let penalty_cfg: PenaltyConfig = e
            .storage()
            .instance()
            .get(&DataKey::PenaltyConfig)
            .expect("penalty config required for early exit penalty routing");

        // CEI: deactivate bond before external transfers.
        bond.active = false;
        e.storage().instance().set(&key, &bond);

        e.events().publish(
            (Symbol::new(&e, "bond_early_exit"), owner.clone()),
            (net_amount, penalty),
        );

        // Transfer net amount to owner.
        if net_amount > 0 {
            transfer_out(&e, &owner, net_amount);
        }
        // Transfer penalty to the penalty treasury.
        if penalty > 0 {
            transfer_out(&e, &penalty_cfg.treasury, penalty);
        }

        bond
    }

    // ── Queries ─────────────────────────────────────────────────────────────

    /// Return the bond state for `owner`. Panics if none exists.
    pub fn get_bond(e: Env, owner: Address) -> FixedBond {
        let key = DataKey::Bond(owner);
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound))
    }

    /// True if the lock period has elapsed.
    pub fn is_matured(e: Env, owner: Address) -> bool {
        let key = DataKey::Bond(owner);
        let bond: Option<FixedBond> = e.storage().instance().get(&key);
        match bond {
            Some(b) => e.ledger().timestamp() >= b.bond_expiry,
            None => false,
        }
    }

    /// Seconds until maturity; 0 if already matured or no bond exists.
    ///
    /// Uses `saturating_sub` — always safe even when `now > bond_expiry`.
    pub fn get_time_remaining(e: Env, owner: Address) -> u64 {
        let key = DataKey::Bond(owner);
        let bond: Option<FixedBond> = e.storage().instance().get(&key);
        match bond {
            Some(b) => {
                let now = e.ledger().timestamp();
                b.bond_expiry.saturating_sub(now)
            }
            None => 0,
        }
    }
}

// ─── Mock Stellar Asset for Tests ───────────────────────────────────────────

#[cfg(test)]
#[contract]
pub struct MockStellarAsset;

#[cfg(test)]
#[contractimpl]
impl MockStellarAsset {
    pub fn decimals(_e: Env) -> u32 {
        6 // USDC standard
    }
    pub fn symbol(e: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&e, "USDC")
    }
    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage()
            .instance()
            .get(&id)
            .unwrap_or(10_000_000_000_000_000_000_i128)
    }
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        let from_bal = Self::balance(e.clone(), from.clone());
        let to_bal = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&from, &(from_bal - amount));
        e.storage().instance().set(&to, &(to_bal + amount));
    }
    pub fn transfer_from(e: Env, _spender: Address, from: Address, to: Address, amount: i128) {
        Self::transfer(e, from, to, amount);
    }
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
        i128::MAX
    }
    pub fn approve(_e: Env, _from: Address, _spender: Address, _amount: i128, _expiration: u32) {}
    pub fn mint(e: Env, to: Address, amount: i128) {
        let current = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&to, &(current + amount));
    }
    pub fn set_authorized(_e: Env, _id: Address, _auth: bool) {}
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::token::StellarAssetClient;

    /// Helper: set the ledger timestamp to an absolute value.
    fn set_timestamp(e: &Env, ts: u64) {
        e.ledger().with_mut(|li| li.timestamp = ts);
    }

    /// Helper: create a test environment with:
    ///  - contract deployed and initialized
    ///  - mock SAC token deployed accordingly
    ///  - fee config set (0 bps by default)
    ///  - penalty config NOT set (call `set_penalty_config` afterward)
    fn setup<'a>() -> (Env, Address, Address, FixedDurationBondClient<'a>) {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);

        // Deploy a mock SAC token. The mock returns a huge default balance
        // for any un-minted address, so token transfers always succeed.
        let token_addr = e.register(MockStellarAsset, ());

        client.initialize(&admin, &token_addr);

        // Fee config with 0 bps (no creation fee).
        let treasury = Address::generate(&e);
        client.set_fee_config(&admin, &treasury, &0_u32);

        (e, admin, token_addr, client)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Duration Bounds Validation Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_duration_at_minimum_bound() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let bond = client.create_bond(&owner, &1000_i128, &MIN_BOND_DURATION);
        assert_eq!(bond.bond_duration, MIN_BOND_DURATION);
        assert!(bond.active);
        assert_eq!(
            bond.bond_expiry,
            bond.bond_start.checked_add(MIN_BOND_DURATION).unwrap()
        );
    }

    #[test]
    fn test_duration_at_maximum_bound() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let bond = client.create_bond(&owner, &1000_i128, &MAX_BOND_DURATION);
        assert_eq!(bond.bond_duration, MAX_BOND_DURATION);
        assert!(bond.active);
        assert_eq!(
            bond.bond_expiry,
            bond.bond_start.checked_add(MAX_BOND_DURATION).unwrap()
        );
    }

    #[test]
    fn test_duration_at_one_day() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let one_day: u64 = 86_400;
        let bond = client.create_bond(&owner, &1000_i128, &one_day);
        assert_eq!(bond.bond_duration, one_day);
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondDuration)")]
    fn test_duration_zero_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &0_u64);
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondDuration)")]
    fn test_duration_above_max_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &(MAX_BOND_DURATION + 1));
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondDuration)")]
    fn test_duration_well_above_max_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &(10 * 365 * 86_400));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Overflow-Safe Timestamp Arithmetic Tests
    // ═══════════════════════════════════════════════════════════════════════════

    /// At epoch zero (default), a valid duration should compute the correct expiry.
    #[test]
    fn test_expiry_at_epoch_zero() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let bond = client.create_bond(&owner, &1000_i128, &86_400_u64);
        assert_eq!(bond.bond_start, 0);
        assert_eq!(bond.bond_expiry, 86_400);
    }

    /// Near u64::MAX: a duration that causes overflow should be rejected.
    #[test]
    #[should_panic(expected = "Error(ContractError::Overflow)")]
    fn test_overflow_near_u64_max() {
        let e = Env::default();
        e.mock_all_auths();

        set_timestamp(&e, u64::MAX - 10);

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        client.initialize(&admin, &token_addr);

        let owner = Address::generate(&e);
        // (u64::MAX - 10) + 11 = u64::MAX + 1 = overflow
        client.create_bond(&owner, &1000_i128, &11_u64);
    }

    /// At u64::MAX, even MIN_BOND_DURATION (1) overflows.
    #[test]
    #[should_panic(expected = "Error(ContractError::Overflow)")]
    fn test_overflow_at_u64_max() {
        let e = Env::default();
        e.mock_all_auths();

        set_timestamp(&e, u64::MAX);

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        client.initialize(&admin, &token_addr);

        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &MIN_BOND_DURATION);
    }

    /// At `u64::MAX - MAX_BOND_DURATION`, a max-duration bond should succeed
    /// (expiry == u64::MAX exactly).
    #[test]
    fn test_max_duration_at_overflow_boundary() {
        let e = Env::default();
        e.mock_all_auths();

        let boundary_ts = u64::MAX - MAX_BOND_DURATION;
        set_timestamp(&e, boundary_ts);

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        client.initialize(&admin, &token_addr);

        let owner = Address::generate(&e);
        let bond = client.create_bond(&owner, &1000_i128, &MAX_BOND_DURATION);
        assert_eq!(bond.bond_expiry, u64::MAX);
        assert!(bond.active);
    }

    /// MAX_BOND_DURATION + 1 is rejected by bounds check (before overflow check).
    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondDuration)")]
    fn test_max_duration_plus_one_at_boundary_rejected() {
        let e = Env::default();
        e.mock_all_auths();

        let boundary_ts = u64::MAX - MAX_BOND_DURATION;
        set_timestamp(&e, boundary_ts);

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        client.initialize(&admin, &token_addr);

        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &(MAX_BOND_DURATION + 1));
    }

    /// At the overflow boundary, a sub-max duration still works.
    #[test]
    fn test_sub_max_duration_at_boundary() {
        let e = Env::default();
        e.mock_all_auths();

        let boundary_ts = u64::MAX - MAX_BOND_DURATION;
        set_timestamp(&e, boundary_ts);

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        client.initialize(&admin, &token_addr);

        let owner = Address::generate(&e);
        let d = MAX_BOND_DURATION - 1;
        let bond = client.create_bond(&owner, &1000_i128, &d);
        assert_eq!(bond.bond_duration, d);
        assert_eq!(bond.bond_expiry, boundary_ts + d);
        assert!(bond.active);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Withdraw / Maturity / Time-Remaining Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic(expected = "lock period has not elapsed yet")]
    fn test_withdraw_before_expiry_panics() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);
        // No time advancement — withdraw should fail.
        client.withdraw(&owner);
    }

    #[test]
    fn test_withdraw_after_expiry_succeeds() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);

        set_timestamp(&e, 86_401);

        let bond = client.withdraw(&owner);
        assert!(!bond.active);
        assert_eq!(bond.amount, 1000);
    }

    #[test]
    fn test_is_matured_before_expiry() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);
        assert!(!client.is_matured(&owner));
    }

    #[test]
    fn test_is_matured_after_expiry() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);

        set_timestamp(&e, 86_401);
        assert!(client.is_matured(&owner));
    }

    #[test]
    fn test_get_time_remaining_positive() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);

        set_timestamp(&e, 43_200); // Halfway.
        let remaining = client.get_time_remaining(&owner);
        assert_eq!(remaining, 43_200);
    }

    #[test]
    fn test_get_time_remaining_zero_after_expiry() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &1000_i128, &86_400_u64);

        set_timestamp(&e, 200_000);
        let remaining = client.get_time_remaining(&owner);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_get_time_remaining_no_bond_returns_zero() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let remaining = client.get_time_remaining(&owner);
        assert_eq!(remaining, 0);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Amount / General Validation
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondAmount)")]
    fn test_amount_zero_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &0_i128, &86_400_u64);
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::InvalidBondAmount)")]
    fn test_amount_negative_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &(-1000_i128), &86_400_u64);
    }

    #[test]
    fn test_create_bond_happy_path() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let bond = client.create_bond(&owner, &5000_i128, &86_400_u64);
        assert!(bond.active);
        assert_eq!(bond.amount, 5000);
        assert_eq!(bond.bond_duration, 86_400);
        assert_eq!(bond.bond_expiry, bond.bond_start + 86_400);
        assert_eq!(bond.owner, owner);
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::BondAlreadyExists)")]
    fn test_duplicate_bond_rejected() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        client.create_bond(&owner, &5000_i128, &86_400_u64);
        client.create_bond(&owner, &5000_i128, &86_400_u64);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Creation Fee Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_creation_fee_deducted() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());

        let token_admin = StellarAssetClient::new(&e, &token_addr);
        let treasury = Address::generate(&e);
        let owner = Address::generate(&e);
        token_admin.set_authorized(&owner, &true);
        token_admin.mint(&owner, &1_000_000_000_i128);

        client.initialize(&admin, &token_addr);

        // 5% creation fee (500 bps).
        client.set_fee_config(&admin, &treasury, &500_u32);

        let bond = client.create_bond(&owner, &100_000_i128, &86_400_u64);
        // 5% of 100_000 = 5_000 fee, net = 95_000.
        assert_eq!(bond.amount, 95_000);
        assert!(bond.active);
    }

    #[test]
    fn test_creation_fee_zero_bps() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());

        let token_admin = StellarAssetClient::new(&e, &token_addr);
        let treasury = Address::generate(&e);
        let owner = Address::generate(&e);
        token_admin.set_authorized(&owner, &true);
        token_admin.mint(&owner, &1_000_000_000_i128);

        client.initialize(&admin, &token_addr);
        client.set_fee_config(&admin, &treasury, &0_u32);

        let bond = client.create_bond(&owner, &100_000_i128, &86_400_u64);
        assert_eq!(bond.amount, 100_000); // No fee deduction.
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Early Exit Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic(expected = "cannot exit early: penalty not configured")]
    fn test_early_exit_without_penalty_panics() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        // No penalty config set — penalty_bps defaults to 0.
        client.create_bond(&owner, &1000_i128, &86_400_u64);
        client.withdraw_early(&owner);
    }

    #[test]
    fn test_early_exit_with_penalty_succeeds() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());

        let token_admin = StellarAssetClient::new(&e, &token_addr);
        let treasury = Address::generate(&e);
        let penalty_treasury = Address::generate(&e);
        let owner = Address::generate(&e);
        token_admin.set_authorized(&owner, &true);
        token_admin.mint(&owner, &1_000_000_000_i128);

        client.initialize(&admin, &token_addr);
        client.set_fee_config(&admin, &treasury, &0_u32);
        // 10% penalty.
        client.set_penalty_config(&admin, &penalty_treasury, &1_000_u32);

        let bond = client.create_bond(&owner, &1000_i128, &86_400_u64);
        assert_eq!(bond.penalty_bps, 1_000);

        let result = client.withdraw_early(&owner);
        assert!(!result.active);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Admin Guard Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic(expected = "Error(ContractError::NotAdmin)")]
    fn test_non_admin_cannot_set_penalty_config() {
        let (e, _admin, _token, client) = setup();
        let impostor = Address::generate(&e);
        let treasury = Address::generate(&e);
        client.set_penalty_config(&impostor, &treasury, &100_u32);
    }

    #[test]
    #[should_panic(expected = "Error(ContractError::AlreadyInitialized)")]
    fn test_double_initialize_panics() {
        let (e, admin, token, client) = setup();
        client.initialize(&admin, &token);
    }

    #[test]
    fn test_get_bond_returns_correct_data() {
        let (e, _admin, _token, client) = setup();
        let owner = Address::generate(&e);
        let created = client.create_bond(&owner, &10_000_i128, &(30 * 86_400_u64));
        let retrieved = client.get_bond(&owner);
        assert_eq!(retrieved.owner, created.owner);
        assert_eq!(retrieved.amount, created.amount);
        assert_eq!(retrieved.bond_expiry, created.bond_expiry);
        assert!(retrieved.active);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Collect Fees Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_collect_fees_transfers_and_zeros() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register(FixedDurationBond, ());
        let client = FixedDurationBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let token_addr = e.register(MockStellarAsset, ());
        let treasury = Address::generate(&e);
        let collector = Address::generate(&e);

        let token_admin = StellarAssetClient::new(&e, &token_addr);
        let owner = Address::generate(&e);
        token_admin.set_authorized(&owner, &true);
        token_admin.mint(&owner, &1_000_000_000_i128);

        client.initialize(&admin, &token_addr);

        // 1% creation fee (100 bps).
        client.set_fee_config(&admin, &treasury, &100_u32);

        // Create bond: fee = 1% of 100_000 = 1_000.
        client.create_bond(&owner, &100_000_i128, &86_400_u64);

        let collected = client.collect_fees(&admin, &collector);
        assert_eq!(collected, 1_000);
    }

    #[test]
    #[should_panic(expected = "no fees to collect")]
    fn test_collect_fees_panics_when_no_fees() {
        let (e, admin, _token, client) = setup();
        let collector = Address::generate(&e);
        client.collect_fees(&admin, &collector);
    }
}
