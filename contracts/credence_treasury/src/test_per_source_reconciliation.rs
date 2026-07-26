//! Property-based reconciliation tests asserting that treasury per-source balances
//! always sum to TotalBalance after arbitrary interleavings of receive_fee and
//! execute_withdrawal operations.
//!
//! # Invariant
//! At all times: `BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance`
//!
//! # Why this matters
//! `execute_withdrawal` computes the `ProtocolFee` deduction via proportional_deduction
//! and derives `SlashedFunds` deduction as `actual_amount - protocol_deduction`. Any
//! rounding or ordering bug can silently desynchronize per-source balances from the total.
//! A property test over random op sequences is the only reliable way to catch such drift.
//!
//! # Operation generation strategy
//! - `receive_fee(ProtocolFee, amount)` — deposit to protocol fee source
//! - `receive_fee(SlashedFunds, amount)` — deposit to slashed funds source
//! - `execute_withdrawal(fraction_of_total)` — withdraw a fraction of the current total

#[cfg(test)]
mod tests {
    use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
    use proptest::prelude::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    /// Maximum deposit amount per operation to keep arithmetic tractable.
    const MAX_DEPOSIT: i128 = 1_000_000_000_i128;

    /// Represents a single treasury operation in the generated sequence.
    #[derive(Debug, Clone)]
    enum TreasuryOp {
        /// Deposit `amount` to the given source.
        Deposit { source: u8, amount: i128 },
        /// Withdraw `numerator/10` fraction of the current total (0–10).
        Withdraw { fraction_tenths: u8 },
    }

    fn treasury_op_strategy() -> impl Strategy<Value = TreasuryOp> {
        prop_oneof![
            // Deposit to ProtocolFee (source=0) or SlashedFunds (source=1)
            (0u8..=1u8, 1i128..=MAX_DEPOSIT).prop_map(|(s, a)| TreasuryOp::Deposit {
                source: s,
                amount: a
            }),
            // Withdraw 0–100% of total in 10% steps
            (0u8..=10u8).prop_map(|f| TreasuryOp::Withdraw { fraction_tenths: f }),
        ]
    }

    fn ops_strategy() -> impl Strategy<Value = Vec<TreasuryOp>> {
        proptest::collection::vec(treasury_op_strategy(), 1..=20)
    }

    /// Set up a fresh treasury environment with one signer (threshold=1) and return
    /// (env, client, admin, token_id, signer).
    fn make_env() -> (
        Env,
        CredenceTreasuryClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let e = Env::default();
        let contract_id = e.register(CredenceTreasury, ());
        let client = CredenceTreasuryClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        let token_admin = Address::generate(&e);
        let token_id = e.register_stellar_asset_contract(token_admin.clone());
        let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);

        e.mock_all_auths();
        client.initialize(&admin, &token_id);

        // Mint a large amount to the admin so receive_fee calls succeed.
        stellar_client.mint(&admin, &(i128::MAX / 4));

        // Configure one signer with threshold=1 so we can always execute proposals.
        let signer = Address::generate(&e);
        client.add_signer(&signer);
        client.set_threshold(&1);

        (e, client, admin, token_id, signer)
    }

    /// Assert the core invariant: sum of per-source balances equals TotalBalance,
    /// and no per-source balance is negative.
    fn assert_invariant(client: &CredenceTreasuryClient<'_>, label: &str) {
        let total = client.get_balance();
        let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
        let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);

        assert!(
            protocol >= 0,
            "{label}: ProtocolFee balance is negative ({protocol})"
        );
        assert!(
            slashed >= 0,
            "{label}: SlashedFunds balance is negative ({slashed})"
        );
        assert_eq!(
            protocol + slashed,
            total,
            "{label}: per-source sum ({}) != TotalBalance ({}); protocol={protocol} slashed={slashed}",
            protocol + slashed,
            total
        );
    }

    proptest! {
        /// Core reconciliation property: after any sequence of deposits and proportional
        /// withdrawals, `BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance`.
        #[test]
        fn per_source_sum_equals_total_balance(ops in ops_strategy()) {
            let (e, client, admin, _token_id, signer) = make_env();

            // Invariant holds on a fresh treasury.
            assert_invariant(&client, "initial");

            for (i, op) in ops.iter().enumerate() {
                match op {
                    TreasuryOp::Deposit { source, amount } => {
                        let fund_source = if *source == 0 {
                            FundSource::ProtocolFee
                        } else {
                            FundSource::SlashedFunds
                        };
                        client.receive_fee(&admin, amount, &fund_source);
                        assert_invariant(&client, &format!("after deposit op #{i}"));
                    }
                    TreasuryOp::Withdraw { fraction_tenths } => {
                        let total = client.get_balance();
                        if total == 0 || *fraction_tenths == 0 {
                            // Nothing to withdraw; invariant trivially holds.
                            continue;
                        }
                        // Compute withdrawal amount as a fraction of the current total.
                        let amount = (total / 10) * i128::from(*fraction_tenths);
                        if amount == 0 {
                            continue;
                        }
                        let recipient = Address::generate(&e);
                        let proposal_id = client.propose_withdrawal(&signer, &recipient, &amount);
                        client.approve_withdrawal(&signer, &proposal_id);
                        client.execute_withdrawal(&proposal_id, &0);
                        assert_invariant(&client, &format!("after withdrawal op #{i}"));

                        // Additionally assert no individual source went negative
                        // (redundant with assert_invariant but explicit for clarity).
                        prop_assert!(client.get_balance_by_source(&FundSource::ProtocolFee) >= 0);
                        prop_assert!(client.get_balance_by_source(&FundSource::SlashedFunds) >= 0);

                        // Assert aggregate withdrawn does not exceed what was requested.
                        let new_total = client.get_balance();
                        prop_assert!(
                            new_total >= 0,
                            "TotalBalance went negative after withdrawal: {new_total}"
                        );
                        prop_assert!(
                            total - new_total <= amount,
                            "Withdrew more ({}) than requested ({})",
                            total - new_total,
                            amount
                        );
                    }
                }
            }
        }

        /// Edge case: deposits to only one source then full withdrawal.
        /// Ensures the non-deposited source stays at zero throughout.
        #[test]
        fn single_source_deposit_then_full_withdrawal(
            deposit_amount in 1i128..=MAX_DEPOSIT,
            use_protocol_fee in any::<bool>(),
        ) {
            let (_e, client, admin, _token_id, signer) = make_env();
            let e = _e;

            let fund_source = if use_protocol_fee {
                FundSource::ProtocolFee
            } else {
                FundSource::SlashedFunds
            };
            let other_source = if use_protocol_fee {
                FundSource::SlashedFunds
            } else {
                FundSource::ProtocolFee
            };

            client.receive_fee(&admin, &deposit_amount, &fund_source);
            assert_invariant(&client, "after single-source deposit");

            // Non-deposited source must remain exactly zero.
            prop_assert_eq!(client.get_balance_by_source(&other_source), 0);
            prop_assert_eq!(client.get_balance_by_source(&fund_source), deposit_amount);

            // Full withdrawal.
            let recipient = Address::generate(&e);
            let proposal_id = client.propose_withdrawal(&signer, &recipient, &deposit_amount);
            client.approve_withdrawal(&signer, &proposal_id);
            client.execute_withdrawal(&proposal_id, &0);

            assert_invariant(&client, "after full withdrawal of single-source deposit");
            prop_assert_eq!(client.get_balance(), 0);
            prop_assert_eq!(client.get_balance_by_source(&fund_source), 0);
            prop_assert_eq!(client.get_balance_by_source(&other_source), 0);
        }

        /// Edge case: withdraw exactly equal to total when both sources have non-zero balances.
        #[test]
        fn full_withdrawal_two_sources_balances_to_zero(
            protocol_amt in 1i128..=500_000_000i128,
            slashed_amt in 1i128..=500_000_000i128,
        ) {
            let (_e, client, admin, _token_id, signer) = make_env();
            let e = _e;

            client.receive_fee(&admin, &protocol_amt, &FundSource::ProtocolFee);
            client.receive_fee(&admin, &slashed_amt, &FundSource::SlashedFunds);
            let total = client.get_balance();
            prop_assert_eq!(total, protocol_amt + slashed_amt);

            assert_invariant(&client, "after two-source deposits");

            let recipient = Address::generate(&e);
            let proposal_id = client.propose_withdrawal(&signer, &recipient, &total);
            client.approve_withdrawal(&signer, &proposal_id);
            client.execute_withdrawal(&proposal_id, &0);

            assert_invariant(&client, "after full two-source withdrawal");
            prop_assert_eq!(client.get_balance(), 0);
            prop_assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 0);
            prop_assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);
        }
    }
}
