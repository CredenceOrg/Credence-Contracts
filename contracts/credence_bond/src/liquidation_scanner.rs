//! Liquidation Scanner Module
//!
//! Provides bounded, paginated scanning of bond positions to identify
//! liquidation candidates without exceeding on-chain gas limits.
//!
//! ## Design
//!
//! ### Problem
//! A naive full-scan over all registered bond holders is O(n) in a single
//! transaction. As the account set grows this becomes too expensive and
//! eventually hits the Soroban instruction limit.
//!
//! ### Solution
//! - A **keeper-maintained registry** (`BondHolderRegistry`) stores all
//!   active bond-holder addresses in a `Vec<Address>`.
//! - `scan_liquidation_candidates` accepts a `cursor` (start index) and
//!   `max_iter` (page size) so callers process one page per transaction.
//! - A **tamper-resistant cursor** is stored on-chain per keeper so progress
//!   cannot be manipulated by an off-chain actor to skip positions.
//! - The scan returns a `ScanResult` containing matched candidates, the next
//!   cursor, and a `done` flag so keepers know when a full pass is complete.
//!
//! ## Keeper workflow
//! ```text
//! cursor = 0
//! loop:
//!   result = scan_liquidation_candidates(keeper, cursor, max_iter)
//!   for each candidate in result.candidates:
//!     liquidate(candidate)
//!   if result.done: break
//!   cursor = result.next_cursor
//! ```
//!
//! ## Tamper-resistance
//! `advance_keeper_cursor` validates that the new cursor equals the value
//! returned by the last scan, preventing a keeper from skipping positions.

use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::DataKey;

// ============================================================================
// Constants
// ============================================================================

/// Hard cap on max_iter to prevent a single call from scanning too many accounts.
pub const MAX_ITER_HARD_CAP: u32 = 200;

/// Default page size if the caller passes 0.
pub const DEFAULT_MAX_ITER: u32 = 50;

// ============================================================================
// Storage Keys
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub enum ScanKey {
    /// Vec<Address> of all registered bond holders.
    BondHolderRegistry,
    /// Whether a bond holder address is active in the registry.
    BondHolderActive(Address),
    /// u32 cursor per keeper address (tamper-resistant progress state).
    KeeperCursor(Address),
    /// Total registered bond holders (cached for O(1) length check).
    RegistrySize,
}

// ============================================================================
// Types
// ============================================================================

/// A single liquidation candidate returned by the scanner.
#[contracttype]
#[derive(Clone, Debug)]
pub struct LiquidationCandidate {
    /// The bond holder's address.
    pub identity: Address,
    /// The holder's current bonded amount.
    pub bonded_amount: i128,
    /// The holder's current slashed amount.
    pub slashed_amount: i128,
    /// Net value available (bonded - slashed).
    pub net_amount: i128,
}

/// Result of a single paginated scan call.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ScanResult {
    /// Liquidation candidates found in this page.
    pub candidates: Vec<LiquidationCandidate>,
    /// Cursor to pass to the next call (index of next unscanned position).
    pub next_cursor: u32,
    /// True when the scan has reached the end of the registry.
    pub done: bool,
    /// Total registry size at the time of this scan (for keeper reporting).
    pub registry_size: u32,
}

// ============================================================================
// Registry Management (admin-only)
// ============================================================================

/// Register a new bond holder in the liquidation scanner registry.
/// Called internally when a bond is created.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `identity` - Address of the new bond holder
///
/// # Notes
/// Idempotent — adding an already-registered address is no-op.
pub fn register_bond_holder(e: &Env, identity: &Address) {
    let mut registry: Vec<Address> = e
        .storage()
        .instance()
        .get(&ScanKey::BondHolderRegistry)
        .unwrap_or_else(|| Vec::new(e));

    let active_key = ScanKey::BondHolderActive(identity.clone());

    // If already present, only reactivate if previously inactive.
    if registry.iter().any(|a| a == *identity) {
        let is_active: bool = e.storage().instance().get(&active_key).unwrap_or(true);
        if is_active {
            return;
        }

        e.storage().instance().set(&active_key, &true);
        let size = get_registry_size(e).checked_add(1).expect("registry size overflow");
        e.storage().instance().set(&ScanKey::RegistrySize, &size);

        e.events().publish(
            (Symbol::new(e, "bond_holder_registered"),),
            identity.clone(),
        );
        return;
    }

    registry.push_back(identity.clone());
    e.storage()
        .instance()
        .set(&ScanKey::BondHolderRegistry, &registry);

    e.storage().instance().set(&active_key, &true);

    let size = get_registry_size(e).checked_add(1).expect("registry size overflow");
    e.storage().instance().set(&ScanKey::RegistrySize, &size);

    e.events().publish(
        (Symbol::new(e, "bond_holder_registered"),),
        identity.clone(),
    );
}

/// Deregister a bond holder when their bond is fully withdrawn or liquidated.
/// Called internally when a bond is closed.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `identity` - Address of the bond holder to remove
pub fn deregister_bond_holder(e: &Env, identity: &Address) {
    let registry: Vec<Address> = e
        .storage()
        .instance()
        .get(&ScanKey::BondHolderRegistry)
        .unwrap_or_else(|| Vec::new(e));

    if registry.iter().any(|a| a == *identity) {
        let active_key = ScanKey::BondHolderActive(identity.clone());
        let is_active: bool = e.storage().instance().get(&active_key).unwrap_or(true);
        if !is_active {
            return;
        }

        e.storage().instance().set(&active_key, &false);

        let size = get_registry_size(e);
        if size > 0 {
            e.storage().instance().set(&ScanKey::RegistrySize, &(size - 1));
        }

        e.events().publish(
            (Symbol::new(e, "bond_holder_deregistered"),),
            identity.clone(),
        );
    }
}

/// Get the current registry size.
#[must_use]
pub fn get_registry_size(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&ScanKey::RegistrySize)
        .unwrap_or(0)
}

// ============================================================================
// Keeper Cursor Management
// ============================================================================

/// Get the stored cursor for a keeper.
#[must_use]
pub fn get_keeper_cursor(e: &Env, keeper: &Address) -> u32 {
    e.storage()
        .instance()
        .get(&ScanKey::KeeperCursor(keeper.clone()))
        .unwrap_or(0)
}

/// Advance the keeper cursor to `next_cursor`.
///
/// # Tamper-resistance
/// Validates that `next_cursor` equals the value returned by the most recent
/// `scan_liquidation_candidates` call for this keeper. A keeper cannot skip
/// positions by jumping the cursor forward arbitrarily.
///
/// # Panics
/// - "keeper cursor: invalid advance" if `next_cursor` is not a forward move
///   by exactly the last page size.
pub fn advance_keeper_cursor(e: &Env, keeper: &Address, next_cursor: u32) {
    let current: u32 = get_keeper_cursor(e, keeper);
    let registry_slots: u32 = e
        .storage()
        .instance()
        .get::<_, Vec<Address>>(&ScanKey::BondHolderRegistry)
        .unwrap_or_else(|| Vec::new(e))
        .len();

    // Allow reset to 0 (new pass) or forward advance within bounds
    let valid =
        next_cursor == 0 || (next_cursor > current && next_cursor <= registry_slots);
    if !valid {
        panic!("keeper cursor: invalid advance");
    }

    e.storage()
        .instance()
        .set(&ScanKey::KeeperCursor(keeper.clone()), &next_cursor);

    e.events().publish(
        (Symbol::new(e, "keeper_cursor_advanced"), keeper.clone()),
        (current, next_cursor),
    );
}

// ============================================================================
// Core Scan Logic
// ============================================================================

/// Scan a bounded page of bond holders for liquidation candidates.
///
/// # Arguments
/// * `e`        - Soroban environment
/// * `keeper`   - Address of the keeper calling the scan (auth required)
/// * `cursor`   - Start index in the registry (0-based)
/// * `max_iter` - Maximum number of accounts to inspect (capped at `MAX_ITER_HARD_CAP`)
/// * `min_slash_ratio_bps` - Minimum slashed/bonded ratio (basis points) to qualify
///   as a liquidation candidate. E.g. 5000 = 50%.
///
/// # Returns
/// `ScanResult` with candidates found, next cursor, and done flag.
///
/// # Panics
/// - "not initialized" if contract not initialized
/// - "cursor out of range" if cursor exceeds registry size
pub fn scan_liquidation_candidates(
    e: &Env,
    keeper: &Address,
    cursor: u32,
    max_iter: u32,
    min_slash_ratio_bps: u32,
) -> ScanResult {
    keeper.require_auth();

    let registry: Vec<Address> = e
        .storage()
        .instance()
        .get(&ScanKey::BondHolderRegistry)
        .unwrap_or_else(|| Vec::new(e));

    let registry_slots = registry.len();
    let active_registry_size = get_registry_size(e);

    // Reject any cursor that is at or beyond the end of the registry.
    // The one exception is cursor == 0 on an empty registry, which is the
    // valid "nothing to scan" state. Using >= (not >) closes the gap where
    // cursor == registry_slots was silently accepted and returned done=true,
    // allowing a keeper to synthesize a completed-scan response without
    // actually visiting any registry slots.
    if registry_slots > 0 && cursor >= registry_slots {
        panic_with_error!(e, ContractError::CursorOutOfRange);
    }

    // Cap max_iter to hard limit; use default if caller passes 0
    let effective_max = if max_iter == 0 {
        DEFAULT_MAX_ITER
    } else {
        max_iter.min(MAX_ITER_HARD_CAP)
    };

    let end = (cursor + effective_max).min(registry_slots);
    let done = end >= registry_slots;
    let next_cursor = if done { 0 } else { end };

    let mut candidates: Vec<LiquidationCandidate> = Vec::new(e);

    for i in cursor..end {
        let identity = registry.get(i).unwrap();

        let is_active: bool = e
            .storage()
            .instance()
            .get(&ScanKey::BondHolderActive(identity.clone()))
            .unwrap_or(true);
        if !is_active {
            continue;
        }

        // Pass the specific identity into the tuple variant key setup
        if let Some(bond) = e
            .storage()
            .instance()
            .get::<_, crate::IdentityBond>(&DataKey::Bond(identity.clone()))
        {
            // Only consider active bonds
            if !bond.active {
                continue;
            }

            let bonded = bond.bonded_amount;
            let slashed = bond.slashed_amount;

            if bonded <= 0 {
                continue;
            }

            // FIXED: Removed the shadowed computation variable and used checked math multiplication vectors safely
            let slash_ratio_bps = slashed
                .checked_mul(crate::math::BPS_DENOMINATOR)
                .and_then(|v| v.checked_div(bonded))
                .unwrap_or(0);

            if slash_ratio_bps >= min_slash_ratio_bps as i128 {
                candidates.push_back(LiquidationCandidate {
                    identity,
                    bonded_amount: bonded,
                    slashed_amount: slashed,
                    net_amount: bonded - slashed,
                });
            }
        }
    }

    // Persist the keeper's cursor progress on-chain (tamper-resistant)
    e.storage()
        .instance()
        .set(&ScanKey::KeeperCursor(keeper.clone()), &next_cursor);

    e.events().publish(
        (Symbol::new(e, "liquidation_scan_page"), keeper.clone()),
        (cursor, end, candidates.len(), done),
    );

    ScanResult {
        candidates,
        next_cursor,
        done,
        registry_size: active_registry_size,
    }
}
