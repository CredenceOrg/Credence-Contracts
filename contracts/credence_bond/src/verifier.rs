//! Verifier registration system.
//!
//! Allows parties to register as verifiers (attestation providers) by staking the configured
//! token. Tracks per-verifier stake, reputation, and activation status, and supports safe
//! deactivation and stake withdrawal.
//!
//! ## Storage
//! - `ver_min_stake` (Symbol) -> i128 (minimum required stake to activate)
//! - `(ver_info, verifier)` (tuple) -> `VerifierInfo`
//! - `(verifier, verifier)` (tuple) -> bool (access-control role; shared with access_control.rs)
//!
//! Note: `DataKey::AttesterStake(verifier)` is kept in sync with the staked amount so that
//! weighted attestations can use real stake.

use soroban_sdk::token::TokenClient;
use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::weighted_attestation;
use crate::DataKey;

const KEY_MIN_STAKE: &str = "ver_min_stake";
const KEY_INFO_PREFIX: &str = "ver_info";
const KEY_VERIFIER_ROLE_PREFIX: &str = "verifier";

const EVENT_CONFIG_UPDATED: &str = "verifier_config_updated";
const EVENT_REGISTERED: &str = "verifier_registered";
const EVENT_REACTIVATED: &str = "verifier_reactivated";
const EVENT_STAKE_DEPOSITED: &str = "verifier_stake_deposited";
const EVENT_DEACTIVATED: &str = "verifier_deactivated";
const EVENT_STAKE_WITHDRAWN: &str = "verifier_stake_withdrawn";
const EVENT_REPUTATION_UPDATED: &str = "verifier_reputation_updated";

/// Verifier metadata stored on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierInfo {
    /// Total staked amount locked in this contract.
    pub stake: i128,
    /// Reputation score (implementation-defined). This implementation updates it based on
    /// attestation weight and revocations (see `record_attestation_*`).
    pub reputation: i128,
    /// Whether the verifier is currently active (can provide attestations).
    pub active: bool,
    /// Ledger timestamp at first registration.
    pub registered_at: u64,
    /// Ledger timestamp at deactivation (0 if active).
    pub deactivated_at: u64,
    /// Count of attestations issued by this verifier.
    pub attestations_issued: u32,
    /// Count of attestations revoked by this verifier.
    pub attestations_revoked: u32,
}

/// @notice Returns the minimum required verifier stake.
/// @dev Defaults to 0 if unset.
#[must_use]
pub fn get_min_stake(e: &Env) -> i128 {
    e.storage().instance().get(&min_stake_key(e)).unwrap_or(0)
}

/// @notice Sets the minimum required verifier stake (admin-only; caller must enforce).
/// @param min_stake The minimum amount a verifier must stake to become active.
///
/// # Panics
/// Panics if `min_stake` is negative.
pub fn set_min_stake(e: &Env, min_stake: i128) {
    if min_stake < 0 {
        panic!("min stake cannot be negative");
    }
    e.storage().instance().set(&min_stake_key(e), &min_stake);
    e.events()
        .publish((Symbol::new(e, EVENT_CONFIG_UPDATED),), (min_stake,));
}

/// @notice Get verifier info, if registered.
#[must_use]
pub fn get_verifier_info(e: &Env, verifier: &Address) -> Option<VerifierInfo> {
    e.storage()
        .instance()
        .get::<(Symbol, Address), VerifierInfo>(&info_key(e, verifier))
}

/// @notice Returns whether the verifier is currently active.
#[must_use]
pub fn is_verifier_active(e: &Env, verifier: &Address) -> bool {
    get_verifier_info(e, verifier)
        .map(|i| i.active)
        .unwrap_or(false)
}

/// @notice Registers or reactivates a verifier by depositing stake.
/// @param verifier The verifier address (must have approved this contract for transfer_from).
/// @param stake_deposit Amount to deposit and lock as stake.
///
/// # Returns
/// Updated `VerifierInfo`.
///
/// # Panics
/// - If `stake_deposit` is negative.
/// - If the configured token is not set when `stake_deposit > 0`.
/// - If (new registration or reactivation) and resulting stake is less than `min_stake`.
pub fn register_with_stake(e: &Env, verifier: &Address, stake_deposit: i128) -> VerifierInfo {
    if stake_deposit < 0 {
        panic!("stake deposit cannot be negative");
    }

    let min_stake = get_min_stake(e);
    let now = e.ledger().timestamp();

    // Load existing info (if any) and compute new stake.
    let existing = get_verifier_info(e, verifier);
    let (info, kind) = match existing {
        None => {
            let stake = stake_deposit;
            if stake < min_stake {
                panic!("insufficient verifier stake");
            }
            (
                VerifierInfo {
                    stake,
                    reputation: 0,
                    active: true,
                    registered_at: now,
                    deactivated_at: 0,
                    attestations_issued: 0,
                    attestations_revoked: 0,
                },
                RegistrationKind::New,
            )
        }
        Some(mut i) => {
            if i.active {
                // Active verifier can top-up stake by calling this function.
                if stake_deposit == 0 {
                    panic!("verifier already active");
                }
                i.stake = i.stake.checked_add(stake_deposit).expect("stake overflow");
                (i, RegistrationKind::TopUp)
            } else {
                // Reactivation requires min stake to be satisfied.
                i.stake = i.stake.checked_add(stake_deposit).expect("stake overflow");
                if i.stake < min_stake {
                    panic!("insufficient verifier stake");
                }
                i.active = true;
                i.deactivated_at = 0;
                (i, RegistrationKind::Reactivated)
            }
        }
    };

    // Effects: mark as verifier + persist info first (CEI pattern).
    set_verifier_role(e, verifier, true);
    put_verifier_info(e, verifier, &info);
    weighted_attestation::set_attester_stake(e, verifier, info.stake);

    // Interactions: pull stake from verifier into this contract.
    if stake_deposit > 0 {
        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("token not set"));
        let contract = e.current_contract_address();
        TokenClient::new(e, &token).transfer_from(&contract, verifier, &contract, &stake_deposit);
    }

    emit_registration_event(e, verifier, stake_deposit, info.stake, min_stake, kind);
    info
}

/// @notice Legacy registration path (admin-managed, no stake transfer).
/// @dev This exists for backwards compatibility with `register_attester`.
pub fn register_legacy(e: &Env, verifier: &Address) -> VerifierInfo {
    let now = e.ledger().timestamp();
    let existing = get_verifier_info(e, verifier);
    let mut info = existing.unwrap_or(VerifierInfo {
        stake: 0,
        reputation: 0,
        active: true,
        registered_at: now,
        deactivated_at: 0,
        attestations_issued: 0,
        attestations_revoked: 0,
    });
    info.active = true;
    info.deactivated_at = 0;

    set_verifier_role(e, verifier, true);
    put_verifier_info(e, verifier, &info);
    weighted_attestation::set_attester_stake(e, verifier, info.stake);

    emit_registration_event(
        e,
        verifier,
        0,
        info.stake,
        get_min_stake(e),
        RegistrationKind::Legacy,
    );
    info
}

/// @notice Deactivates a verifier (either self or admin; caller must enforce auth).
/// @param reason A short reason indicator (e.g. "self" or "admin").
///
/// # Panics
/// Panics if verifier is not registered or already inactive.
pub fn deactivate_verifier(e: &Env, verifier: &Address, reason: Symbol) -> VerifierInfo {
    let now = e.ledger().timestamp();
    let mut info = get_verifier_info(e, verifier).unwrap_or_else(|| panic!("verifier not found"));
    if !info.active {
        panic!("verifier already inactive");
    }
    info.active = false;
    info.deactivated_at = now;

    // Effects first.
    set_verifier_role(e, verifier, false);
    put_verifier_info(e, verifier, &info);

    e.events().publish(
        (Symbol::new(e, EVENT_DEACTIVATED), verifier.clone()),
        (reason, now, info.stake),
    );
    info
}

/// @notice Deactivate if verifier info exists, otherwise only clears the role flag.
/// @dev Used to keep legacy admin paths safe.
pub fn deactivate_if_exists(e: &Env, verifier: &Address, reason: Symbol) {
    if let Some(info) = get_verifier_info(e, verifier) {
        if info.active {
            let _ = deactivate_verifier(e, verifier, reason);
        } else {
            set_verifier_role(e, verifier, false);
        }
        return;
    }
    set_verifier_role(e, verifier, false);
}

/// @notice Withdraws staked tokens after deactivation.
///
/// # Panics
/// - If verifier is not found.
/// - If verifier is still active.
/// - If amount is <= 0 or exceeds the available stake.
/// - If token is not set.
pub fn withdraw_stake(e: &Env, verifier: &Address, amount: i128) -> VerifierInfo {
    if amount <= 0 {
        panic!("withdraw amount must be positive");
    }

    let mut info = get_verifier_info(e, verifier).unwrap_or_else(|| panic!("verifier not found"));
    if info.active {
        panic!("verifier must be inactive to withdraw stake");
    }
    if amount > info.stake {
        panic!("insufficient staked balance");
    }

    // Effects first (CEI).
    info.stake = info.stake.checked_sub(amount).expect("stake underflow");
    put_verifier_info(e, verifier, &info);
    weighted_attestation::set_attester_stake(e, verifier, info.stake);

    let token: Address = e
        .storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic!("token not set"));
    let contract = e.current_contract_address();
    TokenClient::new(e, &token).transfer(&contract, verifier, &amount);

    e.events().publish(
        (Symbol::new(e, EVENT_STAKE_WITHDRAWN), verifier.clone()),
        (amount, info.stake),
    );
    info
}

/// @notice Sets verifier reputation (admin-only; caller must enforce).
pub fn set_reputation(e: &Env, verifier: &Address, new_reputation: i128, reason: Symbol) {
    let mut info = get_verifier_info(e, verifier).unwrap_or_else(|| panic!("verifier not found"));
    let old = info.reputation;
    info.reputation = new_reputation;
    put_verifier_info(e, verifier, &info);

    emit_reputation_event(
        e,
        verifier,
        new_reputation.checked_sub(old).unwrap_or(0),
        &info,
        reason,
    );
}

/// @notice Records that an attestation was issued; updates reputation.
/// @dev Called by the main contract after `add_attestation`.
pub fn record_attestation_issued(e: &Env, verifier: &Address, weight: u32) {
    let w = i128::from(weight);
    let now = e.ledger().timestamp();
    let mut info = get_verifier_info(e, verifier).unwrap_or(VerifierInfo {
        stake: 0,
        reputation: 0,
        active: true,
        registered_at: now,
        deactivated_at: 0,
        attestations_issued: 0,
        attestations_revoked: 0,
    });

    info.attestations_issued = info
        .attestations_issued
        .checked_add(1)
        .expect("attestation count overflow");
    info.reputation = info.reputation.checked_add(w).expect("reputation overflow");
    put_verifier_info(e, verifier, &info);

    emit_reputation_event(e, verifier, w, &info, Symbol::new(e, "attestation"));
}

/// @notice Records that an attestation was revoked; updates reputation.
/// @dev Called by the main contract after `revoke_attestation`.
pub fn record_attestation_revoked(e: &Env, verifier: &Address, weight: u32) {
    let w = i128::from(weight);
    // Do not block revocations for legacy verifiers that predate verifier info storage.
    let Some(mut info) = get_verifier_info(e, verifier) else {
        return;
    };

    info.attestations_revoked = info
        .attestations_revoked
        .checked_add(1)
        .expect("attestation count overflow");
    info.reputation = info.reputation.checked_sub(w).expect("reputation overflow");
    put_verifier_info(e, verifier, &info);

    emit_reputation_event(e, verifier, -w, &info, Symbol::new(e, "revocation"));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegistrationKind {
    New,
    Reactivated,
    TopUp,
    Legacy,
}

fn min_stake_key(e: &Env) -> Symbol {
    Symbol::new(e, KEY_MIN_STAKE)
}

fn info_key(e: &Env, verifier: &Address) -> (Symbol, Address) {
    (Symbol::new(e, KEY_INFO_PREFIX), verifier.clone())
}

fn role_key(e: &Env, verifier: &Address) -> (Symbol, Address) {
    (Symbol::new(e, KEY_VERIFIER_ROLE_PREFIX), verifier.clone())
}

fn put_verifier_info(e: &Env, verifier: &Address, info: &VerifierInfo) {
    e.storage().instance().set(&info_key(e, verifier), info);
}

fn set_verifier_role(e: &Env, verifier: &Address, enabled: bool) {
    e.storage().instance().set(&role_key(e, verifier), &enabled);
    if !enabled {
        // Preserve legacy gate used by add_attestation.
        e.storage()
            .instance()
            .remove(&DataKey::Attester(verifier.clone()));
    } else {
        e.storage()
            .instance()
            .set(&DataKey::Attester(verifier.clone()), &true);
    }
}

fn emit_registration_event(
    e: &Env,
    verifier: &Address,
    stake_deposited: i128,
    total_stake: i128,
    min_stake: i128,
    kind: RegistrationKind,
) {
    let topic = match kind {
        RegistrationKind::New | RegistrationKind::Legacy => EVENT_REGISTERED,
        RegistrationKind::Reactivated => EVENT_REACTIVATED,
        RegistrationKind::TopUp => EVENT_STAKE_DEPOSITED,
    };

    let kind_symbol = match kind {
        RegistrationKind::New => Symbol::new(e, "new"),
        RegistrationKind::Reactivated => Symbol::new(e, "reactivated"),
        RegistrationKind::TopUp => Symbol::new(e, "top_up"),
        RegistrationKind::Legacy => Symbol::new(e, "legacy"),
    };

    e.events().publish(
        (Symbol::new(e, topic), verifier.clone()),
        (kind_symbol, stake_deposited, total_stake, min_stake),
    );
}

fn emit_reputation_event(
    e: &Env,
    verifier: &Address,
    delta: i128,
    info: &VerifierInfo,
    reason: Symbol,
) {
    e.events().publish(
        (Symbol::new(e, EVENT_REPUTATION_UPDATED), verifier.clone()),
        (
            delta,
            info.reputation,
            info.attestations_issued,
            info.attestations_revoked,
            reason,
        ),
    );
}
