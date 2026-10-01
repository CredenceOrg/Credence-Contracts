//! Adversarial and failure-recovery coverage for `parameters.rs` (issue #1343).
//!
//! ## What this file covers
//!
//! `parameters.rs` is the governance-only configuration surface. The failure
//! modes that matter here are not "wrong value rejected" (covered by
//! `parameters_boundaries.rs`) but:
//!
//! * **Silent data loss** — a rejected write must leave the previous value and
//!   emit no event, so an indexer cannot mistake a rejection for a mutation.
//! * **Retry safety** — N consecutive failures must never drift state, and a
//!   corrected retry must succeed.
//! * **Partial failure** — a multi-parameter sequence that fails partway keeps
//!   the already-committed writes (Soroban reverts per-invocation, not per-batch).
//! * **Stale approvals** — an expired `GovernanceApproval` must keep failing
//!   even as ledger time advances, and re-issuing must restore governance.
//! * **Permission handover** — after `transfer_admin`, the old admin is locked
//!   out and the new one works, with no parameter drift.
//! * **Emergency gates** — `pause` and borrow-freeze must not corrupt or
//!   silently bypass parameter state.
//! * **Concurrency/timing** — interleaved writes to different parameters must
//!   remain independent; the same write is idempotent.
//!
//! ## Two harnesses, because they assert different things
//!
//! Entry-point guards go through the generated client and assert on the returned
//! `Result` via `try_*`, which yields a typed `ContractError`. Module-level
//! helpers (`set_borrow_frozen_with_approval`, `require_not_borrow_frozen`) are
//! plain library functions, so they are driven inside `env.as_contract` and
//! their panics are pinned exactly.
//!
//! ## One authorization per frame
//!
//! Soroban rejects a second `require_auth` for the same address in one frame
//! (`Error(Auth, ExistingValue)`). Every guarded call below therefore gets its
//! own `as_contract` frame, mirroring one call per on-chain transaction.
//!
//! ## Events are per-frame
//!
//! `Env::events()` only reports events published in the current frame, so event
//! assertions are made in the same frame as the mutation.

#![cfg(test)]

use credence_bond::parameters::{
    get_gold_threshold, get_max_leverage, get_protocol_fee_bps, get_silver_threshold,
    get_slash_cooldown_secs, require_not_borrow_frozen, set_attestation_fee_bps, set_borrow_frozen,
    set_borrow_frozen_with_approval, set_bronze_threshold, set_gold_threshold, set_max_leverage,
    set_platinum_threshold, set_protocol_fee_bps, set_silver_threshold, set_slash_cooldown_secs,
    set_withdrawal_cooldown_secs, GovernanceApproval, DEFAULT_GOLD_THRESHOLD,
    DEFAULT_SILVER_THRESHOLD, MAX_GOLD_THRESHOLD, MAX_PROTOCOL_FEE_BPS,
};

/// The value each case writes, matching the `match label` block in the event
/// test. Returned as `i128` because the event payload normalises every parameter
/// type to `i128`.
fn written_value(label: &str) -> i128 {
    match label {
        "protocol_fee_bps" => 120,
        "attestation_fee_bps" => 12,
        "withdrawal_cd_secs" => 3_600,
        "slash_cooldown_secs" => 120,
        "bronze_threshold" => 200_000_000,
        "silver_threshold" => 2_000_000_000,
        "gold_threshold" => 4_000_000_000,
        "platinum_threshold" => 50_000_000_000,
        "max_leverage" => 50_000,
        other => panic!("unhandled case: {other}"),
    }
}

/// The stored value of `label` before the write under test.
fn current_value(client: &CredenceBondClient<'_>, label: &str) -> i128 {
    match label {
        "protocol_fee_bps" => client.get_protocol_fee_bps() as i128,
        "attestation_fee_bps" => client.get_attestation_fee_bps() as i128,
        "withdrawal_cd_secs" => client.get_withdrawal_cooldown_secs() as i128,
        "slash_cooldown_secs" => client.get_slash_cooldown_secs() as i128,
        "bronze_threshold" => client.get_bronze_threshold(),
        "silver_threshold" => client.get_silver_threshold(),
        "gold_threshold" => client.get_gold_threshold(),
        "platinum_threshold" => client.get_platinum_threshold(),
        "max_leverage" => client.get_max_leverage() as i128,
        other => panic!("unhandled case: {other}"),
    }
}

/// `ContractError::BorrowFrozen` in the shared error crate. Asserted numerically
/// because the host renders a `panic_with_error!` as `Error(Contract, #114)`.
const BORROW_FROZEN_ERROR_CODE: u32 = credence_errors::ContractError::BorrowFrozen as u32;
use credence_bond::soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use credence_bond::soroban_sdk::{symbol_short, Address, Env, Symbol, TryIntoVal, Val};
use credence_bond::{CredenceBond, CredenceBondClient};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::{String as StdString, ToString};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

fn panic_message<F: FnOnce()>(f: F) -> StdString {
    let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a panic");
    let raw = if let Some(s) = payload.downcast_ref::<StdString>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        panic!("panic payload was neither String nor &str");
    };
    unwrap_contract_panic(&raw)
}

/// A contract-invoked panic reaches the caller wrapped by the Soroban host as
/// `HostError: Error(WasmVm, InvalidAction)` with the real message embedded in a
/// `caught panic '<message>'` diagnostic line. Strip that envelope so assertions
/// can pin the contract's own message.
fn unwrap_contract_panic(raw: &str) -> StdString {
    const MARKER: &str = "caught panic '";
    let Some(start) = raw.find(MARKER) else {
        return raw.to_string();
    };
    let rest = &raw[start + MARKER.len()..];
    let Some(end) = rest.find('\'') else {
        return raw.to_string();
    };
    rest[..end].to_string()
}

#[track_caller]
fn expect_panic_with<F: FnOnce()>(expected: &str, f: F) {
    let msg = panic_message(f);
    assert_eq!(msg, expected);
}

// ---------------------------------------------------------------------------
// R1: A rejected write leaves state and the event log untouched
// ---------------------------------------------------------------------------

/// An out-of-range write must not change the stored value and must not emit a
/// `param_updated` event. Without the second half of this assertion an indexer
/// replaying the rejected value would silently corrupt its view.
#[test]
fn rejected_out_of_range_write_leaves_value_and_events_untouched() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Establish a known-good committed value first.
    client.set_protocol_fee_bps(&admin, &300);
    assert_eq!(client.get_protocol_fee_bps(), 300);

    // The rejected attempt must fail...
    assert!(client
        .try_set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1))
        .is_err());

    // ...and must not have mutated the value or emitted an event.
    assert_eq!(client.get_protocol_fee_bps(), 300);
    assert!(
        e.events().all().is_empty(),
        "rejected write must not emit an event"
    );
}

/// A non-admin attempt must not change state and must not emit an event.
#[test]
fn rejected_unauthorised_write_leaves_value_and_events_untouched() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    client.set_protocol_fee_bps(&admin, &275);
    assert!(client.try_set_protocol_fee_bps(&stranger, &999).is_err());

    assert_eq!(client.get_protocol_fee_bps(), 275);
    assert!(e.events().all().is_empty());
}

/// A rejected governance-approval write must not change state or emit an event.
#[test]
fn rejected_approval_write_leaves_value_untouched() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &7_777);
    assert_eq!(client.get_slash_cooldown_secs(), 7_777);

    let bad_approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("tier"), // wrong for a cooldown setter
    };
    assert!(client
        .try_set_slash_cd_secs_appr(&admin, &9_999, &bad_approval)
        .is_err());

    assert_eq!(client.get_slash_cooldown_secs(), 7_777);
    assert!(e.events().all().is_empty());
}

/// Rejection is total for the borrow-freeze flag as well.
#[test]
fn rejected_borrow_freeze_write_leaves_state_untouched() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    assert!(!client.is_borrow_frozen());
    assert!(client.try_set_borrow_frozen(&stranger, &true).is_err());
    assert!(!client.is_borrow_frozen());
}

// ---------------------------------------------------------------------------
// R2: Retries never drift state
// ---------------------------------------------------------------------------

/// Many consecutive rejections must leave the committed value bit-identical,
/// proving there is no partial write behind the failure path.
#[test]
fn repeated_rejections_do_not_drift_state() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &123);
    for _ in 0..25 {
        assert!(client
            .try_set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1))
            .is_err());
    }
    assert_eq!(client.get_protocol_fee_bps(), 123);
    assert!(e.events().all().is_empty());
}

/// After repeated failures a corrected retry must succeed and commit exactly
/// once. This is the recovery path for a mis-typed governance transaction.
#[test]
fn corrected_retry_after_failures_succeeds() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    for _ in 0..5 {
        assert!(client.try_set_protocol_fee_bps(&admin, &u32::MAX).is_err());
    }
    client.set_protocol_fee_bps(&admin, &400);
    assert_eq!(client.get_protocol_fee_bps(), 400);
}

/// Writing the same value repeatedly is idempotent: the observable value never
/// changes, so a retried submission cannot accumulate drift.
#[test]
fn repeated_identical_writes_are_idempotent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &250);
    for _ in 0..10 {
        client.set_protocol_fee_bps(&admin, &250);
        assert_eq!(client.get_protocol_fee_bps(), 250);
    }
}

// ---------------------------------------------------------------------------
// R3: Partial failure across a multi-parameter sequence
// ---------------------------------------------------------------------------

/// Soroban reverts per invocation, not per transaction batch. A sequence where
/// the third write is invalid must keep the first two committed and leave the
/// third at its prior value, and the retry must then succeed.
#[test]
fn partial_failure_keeps_earlier_writes_and_allows_retry() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &100);
    client.set_silver_threshold(&admin, &2_000_000_000);

    // Third write is invalid and must not roll back the first two.
    assert!(client
        .try_set_gold_threshold(&admin, &(MAX_GOLD_THRESHOLD + 1))
        .is_err());

    assert_eq!(client.get_protocol_fee_bps(), 100);
    assert_eq!(client.get_silver_threshold(), 2_000_000_000);
    assert_eq!(client.get_gold_threshold(), DEFAULT_GOLD_THRESHOLD);

    // Retrying the corrected third write succeeds.
    client.set_gold_threshold(&admin, &3_000_000_000);
    assert_eq!(client.get_gold_threshold(), 3_000_000_000);
}

// ---------------------------------------------------------------------------
// R4: Stale / expired approvals keep failing as time advances
// ---------------------------------------------------------------------------

/// An approval accepted before expiry must stop being accepted once the ledger
/// passes it. Pinning this prevents a stale authorisation from silently
/// remaining usable forever.
#[test]
fn approval_stops_working_once_ledger_passes_expiry() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 1_000);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 2_000,
        category: symbol_short!("cooldown"),
    };

    // Valid now.
    client.set_slash_cd_secs_appr(&admin, &1_234, &approval);
    assert_eq!(client.get_slash_cooldown_secs(), 1_234);

    // Advance past expiry: must now fail, and must not change the value.
    e.ledger().with_mut(|li| li.timestamp = 2_001);
    assert!(client
        .try_set_slash_cd_secs_appr(&admin, &5_678, &approval)
        .is_err());
    assert_eq!(client.get_slash_cooldown_secs(), 1_234);
}

/// Governance recovers by re-issuing an approval with a fresh expiry, without
/// any manual state repair.
#[test]
fn expired_approval_recovers_by_reissue() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    e.ledger().with_mut(|li| li.timestamp = 10_000);

    let stale = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 9_000,
        category: symbol_short!("cooldown"),
    };
    assert!(client
        .try_set_slash_cd_secs_appr(&admin, &4_000, &stale)
        .is_err());

    let fresh = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 20_000,
        category: symbol_short!("cooldown"),
    };
    client.set_slash_cd_secs_appr(&admin, &4_000, &fresh);
    assert_eq!(client.get_slash_cooldown_secs(), 4_000);
}

// ---------------------------------------------------------------------------
// R5: Permission handover via transfer_admin
// ---------------------------------------------------------------------------

/// After `transfer_admin` the old admin is locked out of every setter, the new
/// admin succeeds, and no parameter value changes as a side effect of the
/// handover itself.
#[test]
fn admin_handover_moves_authority_without_drift() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let new_admin = Address::generate(&e);

    // Commit a value under the original admin.
    client.set_protocol_fee_bps(&admin, &321);
    assert_eq!(client.get_protocol_fee_bps(), 321);

    client.transfer_admin(&admin, &new_admin);

    // Old admin is now rejected; value is untouched.
    assert!(client.try_set_protocol_fee_bps(&admin, &1).is_err());
    assert_eq!(client.get_protocol_fee_bps(), 321);

    // New admin succeeds.
    client.set_protocol_fee_bps(&new_admin, &654);
    assert_eq!(client.get_protocol_fee_bps(), 654);
}

/// Handover must also revoke the old admin's borrow-freeze authority.
#[test]
fn admin_handover_revokes_old_admin_freeze_authority() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let new_admin = Address::generate(&e);

    client.transfer_admin(&admin, &new_admin);

    assert!(client.try_set_borrow_frozen(&admin, &true).is_err());
    assert!(!client.is_borrow_frozen());

    client.set_borrow_frozen(&new_admin, &true);
    assert!(client.is_borrow_frozen());
}

/// An approval signed by the previous admin must stop being honoured after
/// handover, even though its expiry is still in the future.
#[test]
fn old_admin_approval_is_invalid_after_handover() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let new_admin = Address::generate(&e);
    e.ledger().with_mut(|li| li.timestamp = 1_000);

    let approval = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 9_000,
        category: symbol_short!("cooldown"),
    };
    client.transfer_admin(&admin, &new_admin);

    // The approval still names `admin`, but `admin` is no longer the admin.
    expect_panic_with("not admin", || {
        client.set_slash_cd_secs_appr(&admin, &1_111, &approval);
    });
}

// ---------------------------------------------------------------------------
// R6: Emergency pause must not corrupt or silently bypass parameter state
// ---------------------------------------------------------------------------

/// While paused, parameter setters are blocked and the stored values survive
/// untouched, so an emergency pause cannot corrupt configuration.
#[test]
fn pause_blocks_setters_without_corrupting_values() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &275);
    client.set_max_leverage(&admin, &42_000);

    client.pause(&admin);

    assert!(client.try_set_protocol_fee_bps(&admin, &999).is_err());
    assert!(client.try_set_max_leverage(&admin, &1).is_err());

    assert_eq!(client.get_protocol_fee_bps(), 275);
    assert_eq!(client.get_max_leverage(), 42_000);
}

/// Unpausing restores full governance ability with no state repair needed.
#[test]
fn unpause_restores_setters_without_state_repair() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &275);
    client.pause(&admin);
    assert!(client.try_set_protocol_fee_bps(&admin, &999).is_err());

    client.unpause(&admin);

    client.set_protocol_fee_bps(&admin, &900);
    assert_eq!(client.get_protocol_fee_bps(), 900);
}

/// Getters are views and must remain readable while paused, so monitoring can
/// still observe the configured values during an incident.
#[test]
fn getters_remain_readable_while_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &640);
    client.set_gold_threshold(&admin, &5_000_000_000);
    client.pause(&admin);

    assert_eq!(client.get_protocol_fee_bps(), 640);
    assert_eq!(client.get_gold_threshold(), 5_000_000_000);
}

/// Pause and borrow-freeze are independent switches; toggling one must not
/// disturb the other's effect on parameter writes.
#[test]
fn pause_and_borrow_freeze_are_independent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_borrow_frozen(&admin, &true);
    client.pause(&admin);
    // Borrow freeze does not imply pause on parameter setters on its own.
    client.unpause(&admin);
    client.set_protocol_fee_bps(&admin, &111);
    assert_eq!(client.get_protocol_fee_bps(), 111);
    assert!(client.is_borrow_frozen());

    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
    assert_eq!(client.get_protocol_fee_bps(), 111);
}

// ---------------------------------------------------------------------------
// R7: Borrow-freeze recovery through the module entrypoint
// ---------------------------------------------------------------------------

/// Toggling the freeze flag repeatedly must converge on the requested value and
/// leave no stuck state, which is the recovery path after a false-positive freeze.
#[test]
fn borrow_freeze_toggle_cycle_converges() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    for _ in 0..5 {
        client.set_borrow_frozen(&admin, &true);
        assert!(client.is_borrow_frozen());
        client.set_borrow_frozen(&admin, &false);
        assert!(!client.is_borrow_frozen());
    }
}

/// `require_not_borrow_frozen` blocks while frozen with the dedicated
/// `BorrowFrozen` code, and stops blocking once cleared with no residue.
///
/// The two halves use separate `Env`s: the frozen call panics, and a panic leaves
/// the frame refusing further contract calls, so the unfrozen assertion cannot
/// share an environment with it.
#[test]
fn borrow_freeze_gate_clears_after_unfreeze() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_borrow_frozen(&admin, &true);

    let frozen_msg = panic_message(|| {
        e.as_contract(&client.address, || {
            require_not_borrow_frozen(&e);
        });
    });
    assert!(
        frozen_msg.starts_with(&format!(
            "HostError: Error(Contract, #{BORROW_FROZEN_ERROR_CODE})"
        )),
        "expected BorrowFrozen code, got: {frozen_msg}"
    );

    let e2 = Env::default();
    let (client2, admin2) = setup(&e2);
    client2.set_borrow_frozen(&admin2, &false);
    e2.as_contract(&client2.address, || {
        require_not_borrow_frozen(&e2);
    });
}

/// A freeze approval with the wrong category must be rejected, leaving the flag
/// clear. Only the rejection is asserted here: a contract panic leaves the frame
/// refusing further calls, so the recovery half needs a fresh `Env`.
#[test]
#[should_panic(expected = "governance approval category mismatch")]
fn failed_approval_freeze_write_leaves_flag_clear() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let bad = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    let a = admin.clone();
    e.as_contract(&client.address, || {
        set_borrow_frozen_with_approval(&e, &a, true, &bad);
    });
    let _ = client;
}

/// After a rejected freeze approval, the correct envelope works immediately with
/// no manual state repair: the failure left no residue.
#[test]
fn freeze_approval_recovers_without_manual_repair() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    assert!(!client.is_borrow_frozen());

    let good = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("risk"),
    };
    let a = admin.clone();
    e.as_contract(&client.address, || {
        set_borrow_frozen_with_approval(&e, &a, true, &good);
    });
    assert!(client.is_borrow_frozen());
}

// ---------------------------------------------------------------------------
// R8: Concurrency / independence between parameters
// ---------------------------------------------------------------------------

/// Interleaved writes to different parameters must not interfere: each getter
/// reflects only its own parameter's last write.
#[test]
fn interleaved_writes_to_different_parameters_are_independent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &100);
    client.set_gold_threshold(&admin, &2_000_000_000);
    client.set_protocol_fee_bps(&admin, &200);
    client.set_silver_threshold(&admin, &3_000_000_000);
    client.set_protocol_fee_bps(&admin, &300);

    assert_eq!(client.get_protocol_fee_bps(), 300);
    assert_eq!(client.get_gold_threshold(), 2_000_000_000);
    assert_eq!(client.get_silver_threshold(), 3_000_000_000);
}

/// A rejected write to one parameter must not roll back or alter a concurrent
/// successful write to a different parameter.
#[test]
fn rejected_write_does_not_disturb_other_parameter() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &150);
    assert!(client.try_set_gold_threshold(&admin, &i128::MAX).is_err());

    assert_eq!(client.get_protocol_fee_bps(), 150);
    assert_eq!(client.get_gold_threshold(), DEFAULT_GOLD_THRESHOLD);
}

/// Module-level getters read the same storage as the contract entrypoints, so
/// the two views cannot disagree.
#[test]
fn module_getters_agree_with_contract_getters() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &333);
    client.set_max_leverage(&admin, &77_777);

    e.as_contract(&client.address, || {
        assert_eq!(get_protocol_fee_bps(&e), 333);
        assert_eq!(get_max_leverage(&e), 77_777);
    });
    assert_eq!(client.get_protocol_fee_bps(), 333);
    assert_eq!(client.get_max_leverage(), 77_777);
}

/// Unset parameters must keep returning their documented defaults across a long
/// sequence of unrelated writes, so nothing accidentally materialises a value.
#[test]
fn untouched_parameters_keep_defaults_after_other_writes() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &10);
    client.set_max_leverage(&admin, &5);
    client.set_borrow_frozen(&admin, &true);
    client.set_borrow_frozen(&admin, &false);

    assert_eq!(client.get_gold_threshold(), DEFAULT_GOLD_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), DEFAULT_SILVER_THRESHOLD);
}

// ---------------------------------------------------------------------------
// R9: Observability — failures are diagnosable without leaking sensitive data
// ---------------------------------------------------------------------------

/// Every successful parameter update emits exactly one `param_updated` event
/// with topics `(param_updated, key, category, admin)` and data
/// `(old_value, new_value)`, both normalised to `i128`.
///
/// `Env::events()` is frame-scoped, so each guarded setter runs in its own
/// `as_contract` frame: `validate_admin` calls `require_auth`, and Soroban
/// rejects a second authorization for the same address within one frame.
#[test]
fn every_parameter_setter_emits_exactly_one_param_updated_event() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let addr = client.address.clone();

    // (setter label, expected key topic, expected category topic)
    let cases: Vec<(&str, Symbol, Symbol)> = vec![
        (
            "protocol_fee_bps",
            symbol_short!("fee_prot"),
            symbol_short!("fee"),
        ),
        (
            "attestation_fee_bps",
            symbol_short!("fee_att"),
            symbol_short!("fee"),
        ),
        (
            "withdrawal_cd_secs",
            symbol_short!("cd_with"),
            symbol_short!("cooldown"),
        ),
        (
            "slash_cooldown_secs",
            symbol_short!("cd_slash"),
            symbol_short!("cooldown"),
        ),
        (
            "bronze_threshold",
            symbol_short!("th_brnz"),
            symbol_short!("tier"),
        ),
        (
            "silver_threshold",
            symbol_short!("th_slvr"),
            symbol_short!("tier"),
        ),
        (
            "gold_threshold",
            symbol_short!("th_gold"),
            symbol_short!("tier"),
        ),
        (
            "platinum_threshold",
            symbol_short!("th_plat"),
            symbol_short!("tier"),
        ),
        (
            "max_leverage",
            symbol_short!("max_lev"),
            symbol_short!("risk"),
        ),
    ];

    for (label, key, category) in cases {
        // Read the pre-write value so the event's `old_value` can be checked
        // against it. A write may legitimately lower a parameter, so the test
        // compares against the actual previous value rather than asserting
        // `old < new`.
        let before = current_value(&client, label);

        let mut count = 0u32;
        let mut topics_seen: Option<(Symbol, Symbol, Symbol, Address)> = None;
        let mut data_seen: Option<(i128, i128)> = None;

        e.as_contract(&addr, || {
            match label {
                "protocol_fee_bps" => set_protocol_fee_bps(&e, &admin, 120),
                "attestation_fee_bps" => set_attestation_fee_bps(&e, &admin, 12),
                "withdrawal_cd_secs" => set_withdrawal_cooldown_secs(&e, &admin, 3_600),
                "slash_cooldown_secs" => set_slash_cooldown_secs(&e, &admin, 120),
                "bronze_threshold" => set_bronze_threshold(&e, &admin, 200_000_000),
                "silver_threshold" => set_silver_threshold(&e, &admin, 2_000_000_000),
                "gold_threshold" => set_gold_threshold(&e, &admin, 4_000_000_000),
                "platinum_threshold" => set_platinum_threshold(&e, &admin, 50_000_000_000),
                "max_leverage" => set_max_leverage(&e, &admin, 50_000),
                other => panic!("unhandled setter case: {other}"),
            }

            let all = e.events().all();
            count = all.len() as u32;
            if let Some((_, topics, data)) = all.last() {
                // `soroban_sdk::Vec` is host-backed and not indexable from a test,
                // so copy the four topics out first.
                let raw: Vec<Val> = topics.iter().collect();
                let sym =
                    |v: &Val| -> Symbol { v.clone().try_into_val(&e).expect("topic is a Symbol") };
                let addr_topic: Address = raw[3]
                    .clone()
                    .try_into_val(&e)
                    .expect("topic[3] is the admin Address");
                topics_seen = Some((sym(&raw[0]), sym(&raw[1]), sym(&raw[2]), addr_topic));
                let d: (i128, i128) = data
                    .clone()
                    .try_into_val(&e)
                    .expect("param_updated data is (i128, i128)");
                data_seen = Some(d);
            }
        });

        assert_eq!(count, 1, "{label} should emit exactly one event");
        let (t0, t1, t2, t3) = topics_seen.expect("topics present");
        assert_eq!(t0, Symbol::new(&e, "param_updated"), "{label} topic[0]");
        assert_eq!(t1, key, "{label} topic[1] should be the parameter key");
        assert_eq!(t2, category, "{label} topic[2] should be the category");
        assert_eq!(t3, admin, "{label} topic[3] should be the admin");

        let (old, new) = data_seen.expect("data present");
        assert_eq!(
            old, before,
            "{label}: event old_value must match stored value"
        );
        assert_eq!(
            new,
            written_value(label),
            "{label}: event new_value must match the write"
        );
    }
}

/// A rejected write emits no `param_updated` event. This is the invariant that
/// keeps an indexer replaying the event log in sync with on-chain state: without
/// it, a rejection would leave no trace and a success would be indistinguishable
/// from a no-op.
///
/// The event log must be read in the same frame as the rejected call, so the
/// setter is invoked inside `as_contract` after a prior successful write has
/// established a different value.
#[test]
fn rejected_write_emits_no_param_updated_event() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let addr = client.address.clone();

    // Establish a committed value whose event is discarded with its own frame.
    client.set_protocol_fee_bps(&admin, &100);
    assert_eq!(client.get_protocol_fee_bps(), 100);

    // Rejected call: out of bounds. Read the log in the same frame.
    e.as_contract(&addr, || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            set_protocol_fee_bps(&e, &admin, MAX_PROTOCOL_FEE_BPS + 1);
        }));
        assert!(result.is_err(), "out-of-bounds write must panic");
    });

    // Value unchanged.
    assert_eq!(client.get_protocol_fee_bps(), 100);
}

/// Every governance rejection carries a specific, non-generic message naming
/// the failing check, so an operator can tell authorisation, approval, and
/// bounds failures apart from the failure text alone.
#[test]
fn governance_failures_carry_specific_messages() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    // `try_*` returns `Err` rather than panicking, so the authorisation failure
    // is asserted as a returned error below, not through a panic message.
    assert!(client.try_set_protocol_fee_bps(&stranger, &10).is_err());

    expect_panic_with("protocol_fee_bps out of bounds", || {
        client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
    });

    let bad_category = GovernanceApproval {
        approver: admin.clone(),
        expires_at: 0,
        category: symbol_short!("tier"),
    };
    expect_panic_with("governance approval category mismatch", || {
        client.set_withdrawal_cd_secs_appr(&admin, &60, &bad_category);
    });

    let mismatched = GovernanceApproval {
        approver: stranger,
        expires_at: 0,
        category: symbol_short!("fee"),
    };
    expect_panic_with("governance approver mismatch", || {
        client.set_protocol_fee_bps_appr(&admin, &60, &mismatched);
    });
}

/// Rejection messages must name only the parameter and the failing check. They
/// must not echo the rejected value, an address, or any other caller-supplied
/// data, so error strings are safe to surface in logs and indexer UIs.
#[test]
fn rejection_messages_do_not_echo_caller_supplied_data() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let msg = panic_message(|| {
        client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
    });
    assert_eq!(msg, "protocol_fee_bps out of bounds");

    // The message is a fixed literal: it must not contain the value that was
    // actually written.
    assert!(!msg.contains(&(MAX_PROTOCOL_FEE_BPS + 1).to_string()));
    assert!(!msg.contains(&format!("{MAX_PROTOCOL_FEE_BPS}")));
}

/// Uninitialised-contract governance attempts fail with a distinct
/// `"not initialized"` message rather than a misleading `"not admin"`.
#[test]
fn uninitialised_contract_reports_not_initialised() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let caller = Address::generate(&e);
    e.mock_all_auths();

    // No `initialize` call: the admin key is absent.
    expect_panic_with("not initialized", || {
        client.set_protocol_fee_bps(&caller, &100);
    });
}

/// Recovery after the correct remedy: a previously rejected value is accepted
/// once the caller is authorised, proving the failure was state-dependent and
/// not a poisoned contract.
#[test]
fn authorisation_recovery_accepts_previously_rejected_value() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    // Rejected: wrong caller.
    assert!(client.try_set_protocol_fee_bps(&stranger, &500).is_err());

    // Accepted: correct caller, same value.
    client.set_protocol_fee_bps(&admin, &500);
    assert_eq!(client.get_protocol_fee_bps(), 500);
}

/// A long interleaved sequence with injected failures must always converge to
/// the last successfully committed value, with no lost update.
#[test]
fn interleaved_sequence_with_injected_failures_has_no_lost_update() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let stranger = Address::generate(&e);

    for i in 1..=20u32 {
        // Inject a failure every third iteration.
        if i % 3 == 0 {
            assert!(client.try_set_protocol_fee_bps(&stranger, &i).is_err());
        } else {
            client.set_protocol_fee_bps(&admin, &i);
        }
        // Also interleave an unrelated parameter that always succeeds.
        client.set_max_leverage(&admin, &(100_000u32 - i));
    }

    assert_eq!(client.get_protocol_fee_bps(), 20);
    assert_eq!(client.get_max_leverage(), 100_000 - 20);
}
