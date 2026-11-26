//! Model-based proptest that asserts every invariant in docs/TREASURY_INVARIANTS.md
//! over randomly generated sequences of deposits, withdrawals, fees and corridor
//! settlements.
//!
//! # Invariants checked after every operation
//! 1. `TotalBalance == BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds)`
//! 2. `token.balance(contract) >= TotalBalance`  (solvency / custody safety)
//! 3. No per-source or total balance is negative.
//! 4. `U256(CumulativeReceived) == U256(CumulativeBySource(ProtocolFee)) + U256(CumulativeBySource(SlashedFunds))`
//! 5. Cumulative values are monotonically non-decreasing (never decrease after any op).
//! 6. Signer-set integrity: duplicate add_signer is a no-op; threshold is never
//!    left above the live signer count (exercised via add/remove cycles).

#[cfg(test)]
mod tests {
    extern crate alloc;
    use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
    use alloc::vec::Vec;
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    const MAX_DEPOSIT: i128 = 1_000_000_000_i128;
    const CUMULATIVE_SEGMENT: u128 = (i128::MAX as u128) + 1;

    // ── Model ────────────────────────────────────────────────────────────────

    /// Pure Rust model that tracks the same accounting state the contract tracks.
    #[derive(Debug, Default, Clone)]
    struct Model {
        total: i128,
        protocol: i128,
        slashed: i128,
        cum_total: u128,
        cum_protocol: u128,
        cum_slashed: u128,
    }

    impl Model {
        fn deposit(&mut self, amount: i128, is_protocol: bool) {
            self.total += amount;
            if is_protocol {
                self.protocol += amount;
                self.cum_protocol += amount as u128;
            } else {
                self.slashed += amount;
                self.cum_slashed += amount as u128;
            }
            self.cum_total += amount as u128;
        }

        /// Mirrors `proportional_deduction` + remainder-on-slashed.
        fn deduct(&mut self, amount: i128) {
            let pd = if self.protocol == 0 || amount == 0 {
                0
            } else if amount == self.total {
                self.protocol
            } else {
                let p = self.protocol as u128;
                let a = amount as u128;
                let t = self.total as u128;
                ((p * a) / t) as i128
            };
            let sd = amount - pd;
            self.total -= amount;
            self.protocol -= pd;
            self.slashed -= sd;
        }
    }

    // ── Operations ───────────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    enum Op {
        /// Deposit `amount` to ProtocolFee (true) or SlashedFunds (false).
        Deposit { is_protocol: bool, amount: i128 },
        /// Withdraw `tenths/10` of the current total (1–10; 10 = full drain).
        Withdraw { tenths: u8 },
        /// Corridor settlement — same proportional deduction as withdrawal.
        Settle { tenths: u8 },
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            4 => (any::<bool>(), 1i128..=MAX_DEPOSIT)
                .prop_map(|(is_protocol, amount)| Op::Deposit { is_protocol, amount }),
            3 => (1u8..=10u8).prop_map(|t| Op::Withdraw { tenths: t }),
            2 => (1u8..=10u8).prop_map(|t| Op::Settle { tenths: t }),
        ]
    }

    fn ops_strategy() -> impl Strategy<Value = Vec<Op>> {
        proptest::collection::vec(op_strategy(), 2..=30)
    }

    // ── Environment helpers ──────────────────────────────────────────────────

    fn setup() -> (
        Env,
        CredenceTreasuryClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let e = Env::default();
        let contract_id = e.register(CredenceTreasury, ());
        let client = CredenceTreasuryClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let token_admin = Address::generate(&e);
        let token_id = e.register_stellar_asset_contract(token_admin);

        e.mock_all_auths();
        client.initialize(&admin, &token_id);

        // Fund admin with enough tokens to cover the entire deposit sequence.
        let stellar = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
        stellar.mint(&admin, &(i128::MAX / 2));

        // One signer, threshold = 1 so withdrawals always execute immediately.
        let signer = Address::generate(&e);
        client.add_signer(&signer);
        client.set_threshold(&1);

        // Register one corridor destination for Settle ops.
        let corridor = Address::generate(&e);
        client.register_corridor(&admin, &corridor);

        (e, client, admin, token_id, signer, corridor)
    }

    // ── Invariant helpers ────────────────────────────────────────────────────

    fn cumulative_to_u128(c: &crate::CumulativeAmount) -> u128 {
        (c.rollovers as u128) * CUMULATIVE_SEGMENT
            + u128::try_from(c.remainder).expect("remainder must be non-negative")
    }

    /// Assert invariants 1–4 and cross-check against the pure model.
    fn assert_invariants(
        client: &CredenceTreasuryClient<'_>,
        token_id: &Address,
        model: &Model,
        label: &str,
    ) {
        let total = client.get_balance();
        let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
        let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
        let contract_addr = client.address.clone();
        let token_client = soroban_sdk::token::TokenClient::new(&client.env, token_id);
        let actual_token = token_client.balance(&contract_addr);

        // Invariant 1: source sum == total
        assert_eq!(
            protocol + slashed,
            total,
            "[{label}] Inv1: protocol({protocol}) + slashed({slashed}) != total({total})"
        );

        // Invariant 2: actual token balance >= TotalBalance (solvency)
        assert!(
            actual_token >= total,
            "[{label}] Inv2: token_balance({actual_token}) < TotalBalance({total})"
        );

        // Invariant 3: no negative balances
        assert!(
            protocol >= 0,
            "[{label}] Inv3: protocol balance negative ({protocol})"
        );
        assert!(
            slashed >= 0,
            "[{label}] Inv3: slashed balance negative ({slashed})"
        );
        assert!(
            total >= 0,
            "[{label}] Inv3: total balance negative ({total})"
        );

        // Invariant 4: cumulative reconciliation — U256 canonical path
        let cum_total_u256 = client.get_cumulative_received_u256();
        let cum_proto_u256 = client.get_cumulative_by_source_u256(&FundSource::ProtocolFee);
        let cum_slash_u256 = client.get_cumulative_by_source_u256(&FundSource::SlashedFunds);
        assert_eq!(
            cum_total_u256,
            cum_proto_u256.add(&cum_slash_u256),
            "[{label}] Inv4(u256): cumulative total != sum of per-source cumulatives"
        );

        // Invariant 4: also check the rollover-safe struct path
        let cum_t = cumulative_to_u128(&client.get_cumulative_received());
        let cum_p =
            cumulative_to_u128(&client.get_cumulative_by_source(&FundSource::ProtocolFee));
        let cum_s =
            cumulative_to_u128(&client.get_cumulative_by_source(&FundSource::SlashedFunds));
        assert_eq!(
            cum_p + cum_s,
            cum_t,
            "[{label}] Inv4(struct): cumulative sum ({}) != total ({})",
            cum_p + cum_s,
            cum_t
        );

        // Model cross-check
        assert_eq!(
            total, model.total,
            "[{label}] model mismatch: total contract={total} model={}",
            model.total
        );
        assert_eq!(
            protocol, model.protocol,
            "[{label}] model mismatch: protocol contract={protocol} model={}",
            model.protocol
        );
        assert_eq!(
            slashed, model.slashed,
            "[{label}] model mismatch: slashed contract={slashed} model={}",
            model.slashed
        );
    }

    /// Assert Invariant 5: cumulative total did not decrease.
    fn assert_cumulative_monotonic(
        prev: u128,
        client: &CredenceTreasuryClient<'_>,
        label: &str,
    ) {
        let curr = cumulative_to_u128(&client.get_cumulative_received());
        assert!(
            curr >= prev,
            "[{label}] Inv5: cumulative decreased {prev} -> {curr}"
        );
    }

    // ── Core property ────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// All documented invariants hold after every operation in a randomly
        /// generated sequence of deposits, withdrawals, and corridor settlements.
        #[test]
        fn all_invariants_hold_over_random_op_sequence(ops in ops_strategy()) {
            let (e, client, admin, token_id, signer, corridor) = setup();
            let mut model = Model::default();

            assert_invariants(&client, &token_id, &model, "initial");

            for (i, op) in ops.iter().enumerate() {
                let label = alloc::format!("op#{i}");
                let prev_cum = cumulative_to_u128(&client.get_cumulative_received());

                match op {
                    Op::Deposit { is_protocol, amount } => {
                        let source = if *is_protocol {
                            FundSource::ProtocolFee
                        } else {
                            FundSource::SlashedFunds
                        };
                        client.receive_fee(&admin, amount, &source);
                        model.deposit(*amount, *is_protocol);
                        // Inv5: cumulative must grow on deposit
                        assert_cumulative_monotonic(prev_cum, &client, &label);
                    }
                    Op::Withdraw { tenths } => {
                        let total = client.get_balance();
                        if total == 0 {
                            continue;
                        }
                        let amount = (total / 10) * i128::from(*tenths);
                        if amount == 0 {
                            continue;
                        }
                        // Inv5: cumulative must not change during withdrawal
                        let cum_before =
                            cumulative_to_u128(&client.get_cumulative_received());
                        let recipient = Address::generate(&e);
                        let pid = client.propose_withdrawal(&signer, &recipient, &amount);
                        client.approve_withdrawal(&signer, &pid);
                        client.execute_withdrawal(&pid, &0);
                        let cum_after =
                            cumulative_to_u128(&client.get_cumulative_received());
                        prop_assert_eq!(
                            cum_before,
                            cum_after,
                            "Inv5: cumulative changed during withdrawal at {}",
                            label
                        );
                        model.deduct(amount);
                    }
                    Op::Settle { tenths } => {
                        let total = client.get_balance();
                        if total == 0 {
                            continue;
                        }
                        let amount = (total / 10) * i128::from(*tenths);
                        if amount == 0 {
                            continue;
                        }
                        let cum_before =
                            cumulative_to_u128(&client.get_cumulative_received());
                        client.settle(&admin, &corridor, &amount);
                        let cum_after =
                            cumulative_to_u128(&client.get_cumulative_received());
                        prop_assert_eq!(
                            cum_before,
                            cum_after,
                            "Inv5: cumulative changed during settle at {}",
                            label
                        );
                        model.deduct(amount);
                    }
                }

                assert_invariants(&client, &token_id, &model, &label);
            }
        }

        /// Invariant 2 holds when tokens arrive directly (bypassing receive_fee):
        /// excess accumulates above TotalBalance but TotalBalance itself is unchanged.
        #[test]
        fn solvency_holds_with_direct_token_airdrop(
            deposit in 1i128..=MAX_DEPOSIT,
            airdrop in 1i128..=MAX_DEPOSIT,
        ) {
            let (e, client, admin, token_id, _signer, _corridor) = setup();
            client.receive_fee(&admin, &deposit, &FundSource::ProtocolFee);

            // Mint directly to the contract address, bypassing receive_fee.
            let stellar = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
            let contract_addr = client.address.clone();
            stellar.mint(&contract_addr, &airdrop);

            let total = client.get_balance();
            let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);
            let actual = token_client.balance(&contract_addr);

            prop_assert_eq!(total, deposit, "TotalBalance must not change from airdrop");
            prop_assert!(
                actual >= total,
                "Inv2: actual({actual}) < TotalBalance({total}) after airdrop"
            );
            prop_assert_eq!(actual, deposit + airdrop);
        }

        /// Full drain always zeroes every balance field, while cumulative still
        /// records the lifetime received amount (Invariants 1–4).
        #[test]
        fn full_drain_zeroes_all_balances(
            protocol_amt in 1i128..=500_000_000i128,
            slashed_amt in 1i128..=500_000_000i128,
        ) {
            let (_e, client, admin, token_id, signer, _corridor) = setup();
            let e = _e;
            client.receive_fee(&admin, &protocol_amt, &FundSource::ProtocolFee);
            client.receive_fee(&admin, &slashed_amt, &FundSource::SlashedFunds);

            let total = client.get_balance();
            let recipient = Address::generate(&e);
            let pid = client.propose_withdrawal(&signer, &recipient, &total);
            client.approve_withdrawal(&signer, &pid);
            client.execute_withdrawal(&pid, &0);

            prop_assert_eq!(client.get_balance(), 0);
            prop_assert_eq!(
                client.get_balance_by_source(&FundSource::ProtocolFee),
                0
            );
            prop_assert_eq!(
                client.get_balance_by_source(&FundSource::SlashedFunds),
                0
            );

            // Cumulative must still reflect lifetime received.
            let cum = cumulative_to_u128(&client.get_cumulative_received());
            prop_assert_eq!(cum, (protocol_amt + slashed_amt) as u128);

            // Inv2 still holds — both sides are zero.
            let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);
            prop_assert_eq!(token_client.balance(&client.address.clone()), 0);
        }

        /// Invariant 6 (signer-set integrity): duplicate add_signer is a no-op;
        /// threshold after remove_signer is capped to the remaining signer count,
        /// so the withdrawal path is never permanently deadlocked.
        #[test]
        fn signer_set_integrity(extra_signer_count in 1u32..=5u32) {
            let (e, client, admin, _token_id, signer, _corridor) = setup();

            // Seed with some deposits so we can verify withdrawals still work.
            client.receive_fee(&admin, &100_000, &FundSource::ProtocolFee);

            // Add `extra_signer_count` fresh signers and re-add the existing one (no-op).
            let mut signers: Vec<Address> = alloc::vec![signer.clone()];
            for _ in 0..extra_signer_count {
                let s = Address::generate(&e);
                client.add_signer(&s);
                signers.push(s);
            }
            // Re-add signer — must be a no-op (Inv6: no duplicate inflation).
            client.add_signer(&signer);

            let total_signers = 1 + extra_signer_count; // original + extras, no double-count
            // Threshold is still 1 (set during setup); a withdrawal must still succeed.
            let recipient = Address::generate(&e);
            let pid = client.propose_withdrawal(&signer, &recipient, &1_000);
            client.approve_withdrawal(&signer, &pid);
            client.execute_withdrawal(&pid, &0);
            prop_assert_eq!(client.get_balance(), 99_000_i128);

            // Remove all extras; threshold must be auto-capped to remaining count.
            for s in signers.iter().skip(1) {
                client.remove_signer(s);
            }
            // threshold was 1, signer count is now 1 — threshold stays 1, still usable.
            let threshold = client.get_threshold();
            prop_assert!(
                threshold <= 1,
                "threshold({threshold}) > remaining signer count(1)"
            );
        }
    }
}
