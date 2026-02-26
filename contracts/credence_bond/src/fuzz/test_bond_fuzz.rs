//! Fuzz-style tests for core bond operations.
//!
//! These tests are deterministic (seeded) and run inside `cargo test`, so they can be executed in
//! CI without requiring `cargo-fuzz`/libFuzzer. They aim to discover edge cases and invariant
//! violations across:
//! - `create_bond`
//! - withdrawals (`withdraw_bond`, `withdraw_early`)
//! - slashing (`slash`, `slash_bond`)
//!
//! ## Configuration
//! Environment variables:
//! - `BOND_FUZZ_SEED` (u64): RNG seed. Default: `0xC0DECE`.
//! - `BOND_FUZZ_ITERS` (usize): Iteration count. Default: `1000`.
//! - `BOND_FUZZ_ACTIONS` (usize): Actions per successful create. Default: `4`.
//! - `BOND_FUZZ_EXTENDED` (any): If set and `BOND_FUZZ_ITERS` is unset, uses `5000`.
//! - `BOND_FUZZ_SILENCE_PANICS` (any): If set, installs a no-op panic hook for the duration of
//!   the fuzz run (useful with `--nocapture`).
//!
//! Run an extended session locally:
//! `BOND_FUZZ_EXTENDED=1 cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture`

#![cfg(test)]

extern crate std;

use crate::test_helpers;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::String;

const DEFAULT_SEED: u64 = 0x00C0_DECE;
const DEFAULT_ITERS: usize = 1000;
const DEFAULT_EXTENDED_ITERS: usize = 5_000;
const DEFAULT_ACTIONS: usize = 4;

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }

    fn gen_range_u64(&mut self, upper_exclusive: u64) -> u64 {
        if upper_exclusive <= 1 {
            return 0;
        }
        self.next_u64() % upper_exclusive
    }

    fn gen_i128_nonneg(&mut self, upper_exclusive: i128) -> i128 {
        if upper_exclusive <= 1 {
            return 0;
        }
        let u = self.gen_range_u64(u64::MAX) as i128;
        u.abs() % upper_exclusive
    }

    fn pick_from_i128(&mut self, values: &[i128]) -> i128 {
        let idx = self.gen_range_u64(values.len() as u64) as usize;
        values[idx]
    }

    fn pick_from_u64(&mut self, values: &[u64]) -> u64 {
        let idx = self.gen_range_u64(values.len() as u64) as usize;
        values[idx]
    }
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| {
        if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).ok()
        } else {
            v.parse().ok()
        }
    })
}

fn fuzz_seed() -> u64 {
    env_u64("BOND_FUZZ_SEED").unwrap_or(DEFAULT_SEED)
}

fn fuzz_iters() -> usize {
    env_usize("BOND_FUZZ_ITERS").unwrap_or_else(|| {
        if std::env::var("BOND_FUZZ_EXTENDED").is_ok() {
            DEFAULT_EXTENDED_ITERS
        } else {
            DEFAULT_ITERS
        }
    })
}

fn fuzz_actions_per_iter() -> usize {
    env_usize("BOND_FUZZ_ACTIONS").unwrap_or(DEFAULT_ACTIONS)
}

fn panic_msg(err: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = err.downcast_ref::<&'static str>() {
        String::from(*s)
    } else if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("<non-string panic>")
    }
}

fn assert_bond_invariants(bond: &crate::IdentityBond) {
    assert!(
        bond.bonded_amount >= 0,
        "invariant violated: bonded_amount < 0"
    );
    assert!(
        bond.slashed_amount >= 0,
        "invariant violated: slashed_amount < 0"
    );
    assert!(
        bond.slashed_amount <= bond.bonded_amount,
        "invariant violated: slashed_amount > bonded_amount"
    );
    assert!(
        bond.bond_start.checked_add(bond.bond_duration).is_some(),
        "invariant violated: bond_end timestamp overflow"
    );
}

fn sample_timestamp(rng: &mut SplitMix64) -> u64 {
    // Mostly "normal" timestamps, with occasional near-max values to exercise overflow paths.
    if rng.gen_range_u64(10) == 0 {
        u64::MAX.saturating_sub(rng.gen_range_u64(10_000))
    } else {
        1_000 + rng.gen_range_u64(1_000_000)
    }
}

fn sample_duration(rng: &mut SplitMix64) -> u64 {
    const EDGE: &[u64] = &[
        0,
        1,
        2,
        10,
        100,
        1_000,
        86_400,
        86_400 * 30,
        86_400 * 365,
        u64::MAX,
        u64::MAX - 1,
    ];
    if rng.gen_range_u64(4) == 0 {
        rng.pick_from_u64(EDGE)
    } else {
        rng.gen_range_u64(86_400 * 365 * 5)
    }
}

fn sample_notice_duration(rng: &mut SplitMix64) -> u64 {
    const EDGE: &[u64] = &[0, 1, 60, 3_600, 86_400, 86_400 * 7];
    if rng.gen_range_u64(4) == 0 {
        rng.pick_from_u64(EDGE)
    } else {
        rng.gen_range_u64(86_400 * 30)
    }
}

fn sample_amount_for_create(rng: &mut SplitMix64) -> i128 {
    const EDGE: &[i128] = &[
        -1,
        0,
        1,
        2,
        10,
        1_000,
        1_000_000,
        1_000_000_000,
        1_000_000_000_000,
        1_000_000_000_000_000,
        i128::MAX,
    ];
    match rng.gen_range_u64(8) {
        0 => rng.pick_from_i128(EDGE),
        1 => -rng.gen_i128_nonneg(1_000_000), // negative (should be rejected)
        _ => rng.gen_i128_nonneg(10_000_000_000_000), // typical range
    }
}

fn sample_amount_for_withdraw(rng: &mut SplitMix64, available: i128) -> i128 {
    if available <= 0 {
        return rng.pick_from_i128(&[-1, 0, 1]);
    }
    match rng.gen_range_u64(8) {
        0 => -1,
        1 => 0,
        2 => 1,
        3 => available,
        4 => available.saturating_add(1),
        _ => rng
            .gen_i128_nonneg(available.saturating_add(1))
            .min(available),
    }
}

fn sample_amount_for_slash(rng: &mut SplitMix64, bonded_amount: i128) -> i128 {
    if bonded_amount <= 0 {
        return rng.pick_from_i128(&[-1, 0, 1]);
    }
    match rng.gen_range_u64(8) {
        0 => -1,
        1 => 0,
        2 => 1,
        3 => bonded_amount,
        4 => bonded_amount.saturating_add(1),
        _ => rng
            .gen_i128_nonneg(bonded_amount.saturating_add(1))
            .min(bonded_amount),
    }
}

#[test]
fn fuzz_bond_operations() {
    let silence_panics = std::env::var("BOND_FUZZ_SILENCE_PANICS").is_ok();
    let prev_hook = if silence_panics {
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
        Some(prev)
    } else {
        None
    };

    let run = catch_unwind(AssertUnwindSafe(|| {
        let seed = fuzz_seed();
        let iters = fuzz_iters();
        let actions = fuzz_actions_per_iter();

        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 1_000);
        let (client, admin, identity, token_id, bond_contract_id) =
            test_helpers::setup_with_token(&e);

        let token_client = TokenClient::new(&e, &token_id);
        let asset_client = StellarAssetClient::new(&e, &token_id);

        // Enable early exit to allow fuzzing `withdraw_early` success paths.
        let treasury = Address::generate(&e);
        client.set_early_exit_config(&admin, &treasury, &500_u32);

        // Keep allowance large across iterations.
        let expiration = e.ledger().sequence().saturating_add(10_000) as u32;
        token_client.approve(&identity, &bond_contract_id, &i128::MAX, &expiration);

        let mut rng = SplitMix64::new(seed);
        let mut panic_counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut create_ok = 0_u32;
        let mut ops_ok = 0_u32;

        // Deterministic seed header (only visible with `--nocapture`).
        std::println!("[bond-fuzz] seed=0x{seed:016x} iters={iters} actions_per_iter={actions}");

        // First, run a few fixed "known good" scenarios so invariants are always exercised.
        let fixed = [
            // Use durations that pass `validation::validate_bond_duration`.
            (
                1_000_i128,
                crate::validation::MIN_BOND_DURATION,
                false,
                0_u64,
            ),
            (
                50_000_i128,
                crate::validation::MIN_BOND_DURATION.saturating_mul(30),
                false,
                0_u64,
            ),
            (
                1_000_i128,
                crate::validation::MIN_BOND_DURATION,
                true,
                60_u64,
            ),
        ];

        for (amount, duration, is_rolling, notice) in fixed {
            e.ledger().with_mut(|li| li.timestamp = 1_000);
            let before_identity = token_client.balance(&identity);
            let before_contract = token_client.balance(&bond_contract_id);

            let bond = client.create_bond(&identity, &amount, &duration, &is_rolling, &notice);
            create_ok = create_ok.saturating_add(1);
            assert_bond_invariants(&bond);

            let after_identity = token_client.balance(&identity);
            let after_contract = token_client.balance(&bond_contract_id);
            assert_eq!(
                before_identity.checked_sub(amount).unwrap(),
                after_identity,
                "create_bond balance mismatch (identity)"
            );
            assert_eq!(
                before_contract.checked_add(amount).unwrap(),
                after_contract,
                "create_bond balance mismatch (contract)"
            );
        }

        for iter in 0..iters {
            // Top up identity balance occasionally so long runs don't starve on token balance.
            if iter % 512 == 0 {
                asset_client.mint(&identity, &10_000_000_000_000_i128);
                let expiration = e.ledger().sequence().saturating_add(10_000) as u32;
                token_client.approve(&identity, &bond_contract_id, &i128::MAX, &expiration);
            }

            let ts = sample_timestamp(&mut rng);
            e.ledger().with_mut(|li| li.timestamp = ts);

            let amount = sample_amount_for_create(&mut rng);
            let duration = sample_duration(&mut rng);
            let is_rolling = rng.next_bool();
            let notice = if is_rolling {
                sample_notice_duration(&mut rng)
            } else {
                0
            };

            let before_identity = token_client.balance(&identity);
            let before_contract = token_client.balance(&bond_contract_id);

            let create_res = catch_unwind(AssertUnwindSafe(|| {
                client.create_bond(&identity, &amount, &duration, &is_rolling, &notice)
            }));

            let _created_bond = match create_res {
                Ok(bond) => {
                    create_ok = create_ok.saturating_add(1);

                    // If create succeeded, token balances should reflect the transfer.
                    let after_identity = token_client.balance(&identity);
                    let after_contract = token_client.balance(&bond_contract_id);
                    assert_eq!(
                        before_identity.checked_sub(amount).unwrap(),
                        after_identity,
                        "iter={iter} create_bond balance mismatch (identity) amount={amount}"
                    );
                    assert_eq!(
                        before_contract.checked_add(amount).unwrap(),
                        after_contract,
                        "iter={iter} create_bond balance mismatch (contract) amount={amount}"
                    );

                    assert_bond_invariants(&bond);
                    bond
                }
                Err(err) => {
                    *panic_counts.entry(panic_msg(&*err)).or_default() += 1;
                    continue;
                }
            };

            // Run a small sequence of operations after successful creation.
            for _ in 0..actions {
                let op = rng.gen_range_u64(3);
                match op {
                    // Slashing
                    0 => {
                        let before = client.get_identity_state();
                        let slash_amount = sample_amount_for_slash(&mut rng, before.bonded_amount);
                        let res = catch_unwind(AssertUnwindSafe(|| {
                            // Mix between the two slashing entrypoints.
                            if rng.next_bool() {
                                client.slash(&admin, &slash_amount)
                            } else {
                                // `slash_bond` returns i128 (new_slashed).
                                let _ = client.slash_bond(&admin, &slash_amount);
                                client.get_identity_state()
                            }
                        }));
                        match res {
                            Ok(after) => {
                                ops_ok = ops_ok.saturating_add(1);
                                assert_bond_invariants(&after);
                                assert!(
                                    after.slashed_amount >= before.slashed_amount,
                                    "iter={iter} slashed_amount decreased (before={}, after={})",
                                    before.slashed_amount,
                                    after.slashed_amount
                                );
                            }
                            Err(err) => {
                                *panic_counts.entry(panic_msg(&*err)).or_default() += 1;
                            }
                        }
                    }
                    // Withdrawals
                    1 => {
                        let state = client.get_identity_state();
                        assert_bond_invariants(&state);
                        let available = state
                            .bonded_amount
                            .checked_sub(state.slashed_amount)
                            .unwrap_or(0);
                        let withdraw_amount = sample_amount_for_withdraw(&mut rng, available);

                        let before_contract = token_client.balance(&bond_contract_id);
                        let before_identity = token_client.balance(&identity);
                        let before_treasury = token_client.balance(&treasury);

                        // Choose withdraw path; ensure ledger time satisfies the chosen path.
                        let use_early = rng.next_bool();
                        if use_early {
                            // Ensure we're before lock-up end.
                            let now = state.bond_start.saturating_add(
                                rng.gen_range_u64(state.bond_duration.saturating_add(1)),
                            );
                            e.ledger().with_mut(|li| li.timestamp = now);
                        } else if state.is_rolling {
                            // Request withdrawal then advance beyond notice period.
                            e.ledger()
                                .with_mut(|li| li.timestamp = state.bond_start.saturating_add(1));
                            let _ = catch_unwind(AssertUnwindSafe(|| client.request_withdrawal()));
                            let now = e
                                .ledger()
                                .timestamp()
                                .saturating_add(state.notice_period_duration)
                                .saturating_add(1);
                            e.ledger().with_mut(|li| li.timestamp = now);
                        } else {
                            // Ensure we're after lock-up end.
                            let now = state
                                .bond_start
                                .saturating_add(state.bond_duration)
                                .saturating_add(1);
                            e.ledger().with_mut(|li| li.timestamp = now);
                        }

                        let res = catch_unwind(AssertUnwindSafe(|| {
                            if use_early {
                                client.withdraw_early(&withdraw_amount)
                            } else {
                                client.withdraw_bond(&withdraw_amount)
                            }
                        }));

                        match res {
                            Ok(after) => {
                                ops_ok = ops_ok.saturating_add(1);
                                assert_bond_invariants(&after);

                                // Token invariants: contract balance should decrease by the requested
                                // amount on successful withdrawals; identity/treasury increases depend
                                // on penalty in early exit mode.
                                let after_contract = token_client.balance(&bond_contract_id);
                                assert_eq!(
                                before_contract.checked_sub(withdraw_amount).unwrap(),
                                after_contract,
                                "iter={iter} withdrawal contract balance mismatch amount={withdraw_amount}"
                            );

                                if use_early {
                                    // Early exit sends `penalty` to treasury; identity receives the rest.
                                    let after_identity = token_client.balance(&identity);
                                    let after_treasury = token_client.balance(&treasury);
                                    let identity_delta =
                                        after_identity.checked_sub(before_identity).unwrap();
                                    let treasury_delta =
                                        after_treasury.checked_sub(before_treasury).unwrap();
                                    assert_eq!(
                                    identity_delta.checked_add(treasury_delta).unwrap(),
                                    withdraw_amount,
                                    "iter={iter} early withdrawal split mismatch amount={withdraw_amount}"
                                );
                                } else {
                                    let after_identity = token_client.balance(&identity);
                                    assert_eq!(
                                    before_identity.checked_add(withdraw_amount).unwrap(),
                                    after_identity,
                                    "iter={iter} withdrawal identity balance mismatch amount={withdraw_amount}"
                                );
                                }
                            }
                            Err(err) => {
                                *panic_counts.entry(panic_msg(&*err)).or_default() += 1;
                            }
                        }
                    }
                    // No-op / state check
                    _ => {
                        let state = client.get_identity_state();
                        assert_bond_invariants(&state);
                    }
                }
            }
        }

        assert!(
            create_ok > 0,
            "fuzz produced no successful create_bond cases; seed=0x{seed:016x}"
        );
        assert!(
            ops_ok > 0,
            "fuzz produced no successful post-create operations; seed=0x{seed:016x}"
        );

        // Print a compact panic histogram for debugging (only visible with `--nocapture`).
        if !panic_counts.is_empty() {
            std::println!("[bond-fuzz] panic histogram (top-level):");
            for (msg, count) in panic_counts.iter().take(15) {
                std::println!("  {count:>6}  {msg}");
            }
        }
    }));

    if let Some(hook) = prev_hook {
        std::panic::set_hook(hook);
    }
    if let Err(err) = run {
        std::panic::resume_unwind(err);
    }
}
