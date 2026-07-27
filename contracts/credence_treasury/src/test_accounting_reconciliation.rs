//! Deterministic accounting reconciliation test harness for the treasury.
//!
//! # Purpose
//! Provides reproducible, step-by-step reconciliation tests that detect ledger
//! drift across deposits, withdrawals, corridor settlements, and fee-on-transfer
//! tokens. Every test captures a full accounting snapshot before and after each
//! operation, then asserts every invariant simultaneously so any desynchronization
//! is immediately pinpointed.
//!
//! # Invariants enforced
//! 1. `TotalBalance == BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds)`
//! 2. No per-source balance is negative.
//! 3. `CumulativeReceived == CumulativeBySource(ProtocolFee) + CumulativeBySource(SlashedFunds)` (as U256).
//! 4. Cumulative values are monotonically non-decreasing.
//! 5. Actual on-chain token balance of the contract matches `TotalBalance`.
//! 6. After a withdrawal, the withdrawn amount is correctly deducted from both sources.

#[cfg(test)]
mod tests {
    use crate::{CredenceTreasury, CredenceTreasuryClient, CumulativeAmount, FundSource};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env};

    const CUMULATIVE_SEGMENT: u128 = (i128::MAX as u128) + 1;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// A snapshot of every accounting field at a point in time.
    #[derive(Debug, Clone)]
    struct AccountingSnapshot {
        total_balance: i128,
        protocol_balance: i128,
        slashed_balance: i128,
        cumulative_total: CumulativeAmount,
        cumulative_protocol: CumulativeAmount,
        cumulative_slashed: CumulativeAmount,
        actual_token_balance: i128,
    }

    fn cumulative_to_u128(c: &CumulativeAmount) -> u128 {
        (u128::from(c.rollovers) * CUMULATIVE_SEGMENT)
            + u128::try_from(c.remainder).expect("remainder non-negative")
    }

    /// Capture a full accounting snapshot.
    fn snapshot(client: &CredenceTreasuryClient<'_>, token_id: &Address) -> AccountingSnapshot {
        let e = &client.env;
        let contract_addr = client.address.clone();
        let token_client = soroban_sdk::token::TokenClient::new(e, token_id);

        AccountingSnapshot {
            total_balance: client.get_balance(),
            protocol_balance: client.get_balance_by_source(&FundSource::ProtocolFee),
            slashed_balance: client.get_balance_by_source(&FundSource::SlashedFunds),
            cumulative_total: client.get_cumulative_received(),
            cumulative_protocol: client.get_cumulative_by_source(&FundSource::ProtocolFee),
            cumulative_slashed: client.get_cumulative_by_source(&FundSource::SlashedFunds),
            actual_token_balance: token_client.balance(&contract_addr),
        }
    }

    /// Assert all invariants on a snapshot.
    fn assert_all_invariants(snap: &AccountingSnapshot, label: &str) {
        // Invariant 1: source sum == total
        assert_eq!(
            snap.protocol_balance + snap.slashed_balance,
            snap.total_balance,
            "[{label}] source sum ({}) != TotalBalance ({})",
            snap.protocol_balance + snap.slashed_balance,
            snap.total_balance
        );

        // Invariant 2: no negative balances
        assert!(
            snap.protocol_balance >= 0,
            "[{label}] ProtocolFee balance negative: {}",
            snap.protocol_balance
        );
        assert!(
            snap.slashed_balance >= 0,
            "[{label}] SlashedFunds balance negative: {}",
            snap.slashed_balance
        );
        assert!(
            snap.total_balance >= 0,
            "[{label}] TotalBalance negative: {}",
            snap.total_balance
        );

        // Invariant 3: cumulative total == sum of per-source cumulatives (as u128)
        let cum_total = cumulative_to_u128(&snap.cumulative_total);
        let cum_proto = cumulative_to_u128(&snap.cumulative_protocol);
        let cum_slash = cumulative_to_u128(&snap.cumulative_slashed);
        assert_eq!(
            cum_proto + cum_slash,
            cum_total,
            "[{label}] cumulative sum ({}) != cumulative total ({})",
            cum_proto + cum_slash,
            cum_total
        );

        // Invariant 4: actual token balance matches TotalBalance
        assert_eq!(
            snap.actual_token_balance, snap.total_balance,
            "[{label}] actual token balance ({}) != TotalBalance ({})",
            snap.actual_token_balance, snap.total_balance
        );

        // Invariant 5: cumulative remainder in range [0, CUMULATIVE_SEGMENT)
        assert!(
            snap.cumulative_total.remainder >= 0,
            "[{label}] cumulative total remainder negative"
        );
        assert!(
            (snap.cumulative_total.remainder as u128) < CUMULATIVE_SEGMENT,
            "[{label}] cumulative total remainder out of range"
        );
    }

    /// Assert that cumulative values did not decrease compared to a prior snapshot.
    fn assert_cumulative_monotonic(
        prev: &AccountingSnapshot,
        curr: &AccountingSnapshot,
        label: &str,
    ) {
        let prev_total = cumulative_to_u128(&prev.cumulative_total);
        let curr_total = cumulative_to_u128(&curr.cumulative_total);
        assert!(
            curr_total >= prev_total,
            "[{label}] cumulative total decreased: {prev_total} -> {curr_total}"
        );

        let prev_proto = cumulative_to_u128(&prev.cumulative_protocol);
        let curr_proto = cumulative_to_u128(&curr.cumulative_protocol);
        assert!(
            curr_proto >= prev_proto,
            "[{label}] cumulative ProtocolFee decreased: {prev_proto} -> {curr_proto}"
        );

        let prev_slash = cumulative_to_u128(&prev.cumulative_slashed);
        let curr_slash = cumulative_to_u128(&curr.cumulative_slashed);
        assert!(
            curr_slash >= prev_slash,
            "[{label}] cumulative SlashedFunds decreased: {prev_slash} -> {curr_slash}"
        );
    }

    /// Set up a fresh treasury with one signer (threshold=1).
    fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address, Address) {
        let contract_id = e.register(CredenceTreasury, ());
        let client = CredenceTreasuryClient::new(e, &contract_id);
        let admin = Address::generate(e);
        let token_admin = Address::generate(e);
        let token_id = e.register_stellar_asset_contract(token_admin.clone());

        e.mock_all_auths();
        client.initialize(&admin, &token_id);

        let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
        stellar_client.mint(&admin, &(i128::MAX / 2));

        let signer = Address::generate(e);
        client.add_signer(&signer);
        client.set_threshold(&1);

        (client, admin, token_id, signer)
    }

    /// Helper: execute a full withdrawal cycle (propose + approve + execute).
    fn execute_full_withdrawal(
        client: &CredenceTreasuryClient<'_>,
        signer: &Address,
        amount: i128,
    ) -> Address {
        let recipient = Address::generate(&client.env);
        let id = client.propose_withdrawal(signer, &recipient, &amount);
        client.approve_withdrawal(signer, &id);
        client.execute_withdrawal(&id, &0);
        recipient
    }

    // ── Test: empty treasury invariant ──────────────────────────────────────

    #[test]
    fn reconciliation_empty_treasury() {
        let e = Env::default();
        let (client, _admin, token_id, _signer) = setup(&e);

        let snap = snapshot(&client, &token_id);
        assert_all_invariants(&snap, "empty treasury");
        assert_eq!(snap.total_balance, 0);
        assert_eq!(snap.protocol_balance, 0);
        assert_eq!(snap.slashed_balance, 0);
        assert_eq!(cumulative_to_u128(&snap.cumulative_total), 0);
    }

    // ── Test: single deposit per source ─────────────────────────────────────

    #[test]
    fn reconciliation_single_deposit_protocol_fee() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        let before = snapshot(&client, &token_id);
        client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
        let after = snapshot(&client, &token_id);

        assert_all_invariants(&after, "after protocol deposit");
        assert_cumulative_monotonic(&before, &after, "protocol deposit");
        assert_eq!(after.total_balance, 5_000);
        assert_eq!(after.protocol_balance, 5_000);
        assert_eq!(after.slashed_balance, 0);
        assert_eq!(cumulative_to_u128(&after.cumulative_protocol), 5_000);
        assert_eq!(cumulative_to_u128(&after.cumulative_slashed), 0);
    }

    #[test]
    fn reconciliation_single_deposit_slashed_funds() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        client.receive_fee(&admin, &3_000, &FundSource::SlashedFunds);
        let snap = snapshot(&client, &token_id);

        assert_all_invariants(&snap, "after slashed deposit");
        assert_eq!(snap.total_balance, 3_000);
        assert_eq!(snap.protocol_balance, 0);
        assert_eq!(snap.slashed_balance, 3_000);
        assert_eq!(cumulative_to_u128(&snap.cumulative_protocol), 0);
        assert_eq!(cumulative_to_u128(&snap.cumulative_slashed), 3_000);
    }

    // ── Test: alternating deposits accumulate correctly ──────────────────────

    #[test]
    fn reconciliation_alternating_deposits() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        let amounts = [
            (FundSource::ProtocolFee, 1_000_i128),
            (FundSource::SlashedFunds, 2_000),
            (FundSource::ProtocolFee, 500),
            (FundSource::SlashedFunds, 1_500),
            (FundSource::ProtocolFee, 3_000),
        ];

        let mut prev = snapshot(&client, &token_id);
        let mut expected_protocol = 0_i128;
        let mut expected_slashed = 0_i128;

        for (i, (source, amount)) in amounts.iter().enumerate() {
            client.receive_fee(&admin, amount, source);

            let curr = snapshot(&client, &token_id);
            let label = if i == 0 {
                "deposit #0"
            } else if i == 1 {
                "deposit #1"
            } else if i == 2 {
                "deposit #2"
            } else if i == 3 {
                "deposit #3"
            } else {
                "deposit #4"
            };
            assert_all_invariants(&curr, label);
            assert_cumulative_monotonic(&prev, &curr, label);

            match source {
                FundSource::ProtocolFee => expected_protocol += amount,
                FundSource::SlashedFunds => expected_slashed += amount,
            }
            assert_eq!(curr.protocol_balance, expected_protocol);
            assert_eq!(curr.slashed_balance, expected_slashed);
            assert_eq!(curr.total_balance, expected_protocol + expected_slashed);
            assert_eq!(
                cumulative_to_u128(&curr.cumulative_protocol),
                expected_protocol as u128
            );
            assert_eq!(
                cumulative_to_u128(&curr.cumulative_slashed),
                expected_slashed as u128
            );

            prev = curr;
        }
    }

    // ── Test: withdrawal with proportional deduction ─────────────────────────

    #[test]
    fn reconciliation_proportional_withdrawal_two_sources() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        // Deposit: ProtocolFee=700, SlashedFunds=300, Total=1000
        client.receive_fee(&admin, &700, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &300, &FundSource::SlashedFunds);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "before withdrawal");

        // Withdraw 400 (40% of total)
        execute_full_withdrawal(&client, &signer, 400);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after withdrawal");

        // Proportional deduction:
        // protocol_deduction = floor(700 * 400 / 1000) = 280
        // slashed_deduction  = 400 - 280 = 120
        assert_eq!(after.total_balance, 600);
        assert_eq!(after.protocol_balance, 420); // 700 - 280
        assert_eq!(after.slashed_balance, 180); // 300 - 120

        // Cumulative should NOT decrease after withdrawal (tracks received, not available).
        assert_cumulative_monotonic(&before, &after, "after withdrawal");
    }

    // ── Test: full drain zeroes everything ───────────────────────────────────

    #[test]
    fn reconciliation_full_drain() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &4_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &6_000, &FundSource::SlashedFunds);

        execute_full_withdrawal(&client, &signer, 10_000);

        let snap = snapshot(&client, &token_id);
        assert_all_invariants(&snap, "after full drain");
        assert_eq!(snap.total_balance, 0);
        assert_eq!(snap.protocol_balance, 0);
        assert_eq!(snap.slashed_balance, 0);
        assert_eq!(snap.actual_token_balance, 0);

        // Cumulative still reflects lifetime received.
        assert_eq!(cumulative_to_u128(&snap.cumulative_total), 10_000);
        assert_eq!(cumulative_to_u128(&snap.cumulative_protocol), 4_000);
        assert_eq!(cumulative_to_u128(&snap.cumulative_slashed), 6_000);
    }

    // ── Test: single-source deposit then full withdrawal ─────────────────────

    #[test]
    fn reconciliation_single_source_full_withdrawal() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "single source before");
        assert_eq!(before.slashed_balance, 0);

        execute_full_withdrawal(&client, &signer, 5_000);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "single source after full withdrawal");
        assert_eq!(after.total_balance, 0);
        assert_eq!(after.protocol_balance, 0);
        assert_eq!(after.slashed_balance, 0);
    }

    // ── Test: partial withdrawal, then more deposits, then another withdrawal ─

    #[test]
    fn reconciliation_deposit_withdraw_deposit_withdraw_cycle() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        // Round 1: deposit and withdraw
        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &1_000, &FundSource::SlashedFunds);

        let snap1 = snapshot(&client, &token_id);
        assert_all_invariants(&snap1, "round 1 after deposits");
        assert_eq!(snap1.total_balance, 2_000);

        execute_full_withdrawal(&client, &signer, 500);

        let snap2 = snapshot(&client, &token_id);
        assert_all_invariants(&snap2, "round 1 after partial withdrawal");
        assert_eq!(snap2.total_balance, 1_500);
        // protocol: floor(1000 * 500 / 2000) = 250 -> 750
        // slashed: 500 - 250 = 250 -> 750
        assert_eq!(snap2.protocol_balance, 750);
        assert_eq!(snap2.slashed_balance, 750);

        // Round 2: deposit more, then withdraw more
        client.receive_fee(&admin, &3_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &1_000, &FundSource::SlashedFunds);

        let snap3 = snapshot(&client, &token_id);
        assert_all_invariants(&snap3, "round 2 after more deposits");
        assert_eq!(snap3.total_balance, 5_500);
        assert_eq!(snap3.protocol_balance, 3_750); // 750 + 3000
        assert_eq!(snap3.slashed_balance, 1_750); // 750 + 1000

        execute_full_withdrawal(&client, &signer, 2_000);

        let snap4 = snapshot(&client, &token_id);
        assert_all_invariants(&snap4, "round 2 after second withdrawal");
        assert_eq!(snap4.total_balance, 3_500);
        // protocol: floor(3750 * 2000 / 5500) = floor(7_500_000 / 5500) = 1363
        // slashed: 2000 - 1363 = 637
        assert_eq!(snap4.protocol_balance, 2_387); // 3750 - 1363
        assert_eq!(snap4.slashed_balance, 1_113); // 1750 - 637
    }

    // ── Test: rounding bias accumulates correctly ────────────────────────────

    #[test]
    fn reconciliation_repeated_small_withdrawals_rounding() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        // Uneven ratio: ProtocolFee=1, SlashedFunds=2, Total=3
        client.receive_fee(&admin, &1, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &2, &FundSource::SlashedFunds);

        // Withdraw 1 unit repeatedly until drained.
        let snap0 = snapshot(&client, &token_id);
        assert_all_invariants(&snap0, "rounding initial");

        execute_full_withdrawal(&client, &signer, 1);
        let snap1 = snapshot(&client, &token_id);
        assert_all_invariants(&snap1, "rounding iter 1");
        assert_eq!(snap1.total_balance, 2);

        execute_full_withdrawal(&client, &signer, 1);
        let snap2 = snapshot(&client, &token_id);
        assert_all_invariants(&snap2, "rounding iter 2");
        assert_eq!(snap2.total_balance, 1);

        execute_full_withdrawal(&client, &signer, 1);
        let snap3 = snapshot(&client, &token_id);
        assert_all_invariants(&snap3, "rounding iter 3");
        assert_eq!(snap3.total_balance, 0);
        assert_eq!(snap3.protocol_balance, 0);
        assert_eq!(snap3.slashed_balance, 0);
    }

    // ── Test: large-value deposits and withdrawal ────────────────────────────

    #[test]
    fn reconciliation_large_values() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        let large = i128::MAX / 4;
        let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
        stellar_client.mint(&admin, &large);

        client.receive_fee(&admin, &large, &FundSource::ProtocolFee);
        stellar_client.mint(&admin, &large);
        client.receive_fee(&admin, &large, &FundSource::SlashedFunds);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "large values before");
        assert_eq!(before.total_balance, large * 2);

        let withdraw = large; // withdraw half
        execute_full_withdrawal(&client, &signer, withdraw);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "large values after partial withdrawal");
        assert_eq!(after.total_balance, large);
    }

    // ── Test: zero-amount withdrawal is rejected ─────────────────────────────

    #[test]
    fn reconciliation_propose_zero_rejected() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "before zero proposal");

        // propose_withdrawal with 0 should panic (AmountMustBePositive)
        let result = client.try_propose_withdrawal(&signer, &Address::generate(&e), &0);
        assert!(result.is_err());

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after rejected zero proposal");
        assert_eq!(after.total_balance, before.total_balance);
        assert_eq!(after.protocol_balance, before.protocol_balance);
        assert_eq!(after.slashed_balance, before.slashed_balance);
    }

    // ── Test: multiple sequential withdrawals maintain invariants ─────────────

    #[test]
    fn reconciliation_sequential_withdrawals() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &10_000, &FundSource::SlashedFunds);

        let mut prev = snapshot(&client, &token_id);
        let mut running_total = 20_000_i128;
        let mut running_protocol = 10_000_i128;
        let mut running_slashed = 10_000_i128;

        let labels = [
            "seq withdraw 0",
            "seq withdraw 1",
            "seq withdraw 2",
            "seq withdraw 3",
            "seq withdraw 4",
            "seq withdraw 5",
            "seq withdraw 6",
            "seq withdraw 7",
            "seq withdraw 8",
            "seq withdraw 9",
        ];

        for i in 0..10 {
            let withdraw = 1_000;
            execute_full_withdrawal(&client, &signer, withdraw);

            let curr = snapshot(&client, &token_id);
            assert_all_invariants(&curr, labels[i]);
            assert_cumulative_monotonic(&prev, &curr, labels[i]);

            // Compute expected proportional deductions.
            let protocol_ded =
                (running_protocol as u128 * withdraw as u128 / running_total as u128) as i128;
            let slashed_ded = withdraw - protocol_ded;

            running_total -= withdraw;
            running_protocol -= protocol_ded;
            running_slashed -= slashed_ded;

            assert_eq!(curr.total_balance, running_total);
            assert_eq!(curr.protocol_balance, running_protocol);
            assert_eq!(curr.slashed_balance, running_slashed);

            prev = curr;
        }
    }

    // ── Test: corridor settlement reconciles correctly ───────────────────────

    #[test]
    fn reconciliation_corridor_settlement() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        let destination = Address::generate(&e);
        client.register_corridor(&admin, &destination);

        client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &5_000, &FundSource::SlashedFunds);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "before settle");

        client.settle(&admin, &destination, &4_000);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after settle");

        assert_eq!(after.total_balance, 6_000);
        // protocol: floor(5000 * 4000 / 10000) = 2000 -> 3000
        // slashed: 4000 - 2000 = 2000 -> 3000
        assert_eq!(after.protocol_balance, 3_000);
        assert_eq!(after.slashed_balance, 3_000);

        // Verify actual token balance.
        let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);
        let contract_addr = client.address.clone();
        assert_eq!(token_client.balance(&contract_addr), 6_000);
    }

    // ── Test: deposit + withdraw + deposit + settle interleaved ──────────────

    #[test]
    fn reconciliation_mixed_operations() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        let destination = Address::generate(&e);
        client.register_corridor(&admin, &destination);

        // Op 1: deposit protocol
        client.receive_fee(&admin, &2_000, &FundSource::ProtocolFee);
        assert_all_invariants(&snapshot(&client, &token_id), "op1 deposit protocol");

        // Op 2: deposit slashed
        client.receive_fee(&admin, &3_000, &FundSource::SlashedFunds);
        assert_all_invariants(&snapshot(&client, &token_id), "op2 deposit slashed");

        // Op 3: multi-sig withdrawal
        execute_full_withdrawal(&client, &signer, 1_000);
        let snap3 = snapshot(&client, &token_id);
        assert_all_invariants(&snap3, "op3 withdrawal");
        assert_eq!(snap3.total_balance, 4_000);

        // Op 4: corridor settlement
        client.settle(&admin, &destination, &1_500);
        let snap4 = snapshot(&client, &token_id);
        assert_all_invariants(&snap4, "op4 settle");
        assert_eq!(snap4.total_balance, 2_500);

        // Op 5: more deposits
        client.receive_fee(&admin, &500, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &500, &FundSource::SlashedFunds);
        let snap5 = snapshot(&client, &token_id);
        assert_all_invariants(&snap5, "op5 final deposits");
        assert_eq!(snap5.total_balance, 3_500);

        // Final: cumulative total should equal sum of all deposits.
        let cum_total = cumulative_to_u128(&snap5.cumulative_total);
        assert_eq!(cum_total, 6_000); // 2000+3000+500+500
    }

    // ── Test: proposal expiry does not corrupt accounting ────────────────────

    #[test]
    fn reconciliation_expired_proposal_no_corruption() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
        client.set_proposal_ttl(&admin, &3600);

        let before = snapshot(&client, &token_id);

        let recipient = Address::generate(&e);
        let id = client.propose_withdrawal(&signer, &recipient, &2_000);

        // Advance past TTL.
        let info = e.ledger().get();
        e.ledger().set(soroban_sdk::testutils::LedgerInfo {
            timestamp: info.timestamp + 3601,
            ..info
        });

        // Approval should fail (expired).
        let result = client.try_approve_withdrawal(&signer, &id);
        assert!(result.is_err());

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after expired proposal attempt");
        assert_eq!(after.total_balance, before.total_balance);
        assert_eq!(after.protocol_balance, before.protocol_balance);
        assert_eq!(after.slashed_balance, before.slashed_balance);
    }

    // ── Test: double-execute rejected, accounting unchanged ───────────────────

    #[test]
    fn reconciliation_double_execute_no_corruption() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &3_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &2_000, &FundSource::SlashedFunds);

        let recipient = Address::generate(&e);
        let id = client.propose_withdrawal(&signer, &recipient, &1_000);
        client.approve_withdrawal(&signer, &id);
        client.execute_withdrawal(&id, &0);

        let after_first = snapshot(&client, &token_id);
        assert_all_invariants(&after_first, "after first execute");

        // Second execute should fail.
        let result = client.try_execute_withdrawal(&id, &0);
        assert!(result.is_err());

        let after_second = snapshot(&client, &token_id);
        assert_all_invariants(&after_second, "after rejected second execute");
        assert_eq!(after_second.total_balance, after_first.total_balance);
        assert_eq!(after_second.protocol_balance, after_first.protocol_balance);
        assert_eq!(after_second.slashed_balance, after_first.slashed_balance);
    }

    // ── Test: cumulative reconstruction matches U256 on-chain getter ─────────

    #[test]
    fn reconciliation_cumulative_reconstruction() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &2_000, &FundSource::SlashedFunds);

        let cum_total = client.get_cumulative_received();
        let cum_proto = client.get_cumulative_by_source(&FundSource::ProtocolFee);
        let cum_slash = client.get_cumulative_by_source(&FundSource::SlashedFunds);

        // Manual reconstruction.
        let total_u128 = cumulative_to_u128(&cum_total);
        let proto_u128 = cumulative_to_u128(&cum_proto);
        let slash_u128 = cumulative_to_u128(&cum_slash);

        assert_eq!(total_u128, 3_000);
        assert_eq!(proto_u128, 1_000);
        assert_eq!(slash_u128, 2_000);
        assert_eq!(proto_u128 + slash_u128, total_u128);

        // On-chain U256 getters must match.
        let u256_total = client.get_cumulative_received_u256();
        let u256_proto = client.get_cumulative_by_source_u256(&FundSource::ProtocolFee);
        let u256_slash = client.get_cumulative_by_source_u256(&FundSource::SlashedFunds);

        assert_eq!(u256_total, u256_proto.add(&u256_slash));
    }

    // ── Test: asymmetric source withdrawals maintain ratio ────────────────────

    #[test]
    fn reconciliation_asymmetric_sources_preserve_ratio() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        // Asymmetric: ProtocolFee=999, SlashedFunds=1, Total=1000
        client.receive_fee(&admin, &999, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &1, &FundSource::SlashedFunds);

        // Withdraw 500 — almost all should come from ProtocolFee.
        execute_full_withdrawal(&client, &signer, 500);

        let snap = snapshot(&client, &token_id);
        assert_all_invariants(&snap, "asymmetric after withdrawal");
        assert_eq!(snap.total_balance, 500);
        // protocol: floor(999 * 500 / 1000) = floor(499500/1000) = 499
        // slashed: 500 - 499 = 1
        assert_eq!(snap.protocol_balance, 500); // 999 - 499
        assert_eq!(snap.slashed_balance, 0); // 1 - 1
    }

    // ── Test: interleaved deposits during withdrawal lifecycle ────────────────

    #[test]
    fn reconciliation_propose_then_deposit_then_execute() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &1_000, &FundSource::SlashedFunds);

        let recipient = Address::generate(&e);
        let id = client.propose_withdrawal(&signer, &recipient, &500);
        client.approve_withdrawal(&signer, &id);

        // Deposit more before execution — changes the ratio.
        client.receive_fee(&admin, &3_000, &FundSource::ProtocolFee);

        let before = snapshot(&client, &token_id);
        assert_all_invariants(&before, "before execute after extra deposit");
        assert_eq!(before.total_balance, 5_000);
        assert_eq!(before.protocol_balance, 4_000);
        assert_eq!(before.slashed_balance, 1_000);

        client.execute_withdrawal(&id, &0);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after execute with changed ratio");
        assert_eq!(after.total_balance, 4_500);
        // protocol: floor(4000 * 500 / 5000) = 400
        // slashed: 500 - 400 = 100
        assert_eq!(after.protocol_balance, 3_600); // 4000 - 400
        assert_eq!(after.slashed_balance, 900); // 1000 - 100
    }

    // ── Test: rescue_native preserves accounting invariants ───────────────────

    #[test]
    fn reconciliation_rescue_native_preserves_invariants() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);

        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);

        let contract_id = client.address.clone();
        let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
        stellar_client.mint(&contract_id, &500); // excess

        let before = snapshot(&client, &token_id);
        // Actual balance is 1500, accounted is 1000. The snapshot captures the mismatch.
        assert_eq!(before.actual_token_balance, 1_500);
        assert_eq!(before.total_balance, 1_000);

        let recipient = Address::generate(&e);
        client.rescue_native(&admin, &recipient, &500);

        let after = snapshot(&client, &token_id);
        // After rescue, actual balance should match accounted balance.
        assert_all_invariants(&after, "after rescue_native");
        assert_eq!(after.total_balance, 1_000);
        assert_eq!(after.actual_token_balance, 1_000);
    }

    // ── Test: min_liquidity floor enforced, accounting unchanged on reject ────

    #[test]
    fn reconciliation_min_liquidity_rejection_preserves_state() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
        client.set_min_liquidity(&admin, &500);

        let before = snapshot(&client, &token_id);

        let recipient = Address::generate(&e);
        let id = client.propose_withdrawal(&signer, &recipient, &800);
        client.approve_withdrawal(&signer, &id);

        // Execute should fail: 1000 - 800 = 200 < min_liquidity(500)
        let result = client.try_execute_withdrawal(&id, &0);
        assert!(result.is_err());

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after min_liquidity rejection");
        assert_eq!(after.total_balance, before.total_balance);
        assert_eq!(after.protocol_balance, before.protocol_balance);
        assert_eq!(after.slashed_balance, before.slashed_balance);
    }

    // ── Test: many deposits to single source then drain ──────────────────────

    #[test]
    fn reconciliation_many_deposits_single_source_then_drain() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        for _ in 0..100 {
            client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
        }

        let snap = snapshot(&client, &token_id);
        assert_all_invariants(&snap, "100 deposits");
        assert_eq!(snap.total_balance, 10_000);
        assert_eq!(snap.protocol_balance, 10_000);
        assert_eq!(snap.slashed_balance, 0);
        assert_eq!(cumulative_to_u128(&snap.cumulative_protocol), 10_000);
        assert_eq!(cumulative_to_u128(&snap.cumulative_slashed), 0);

        execute_full_withdrawal(&client, &signer, 10_000);

        let after = snapshot(&client, &token_id);
        assert_all_invariants(&after, "after draining 100 deposits");
        assert_eq!(after.total_balance, 0);
        assert_eq!(cumulative_to_u128(&after.cumulative_protocol), 10_000);
        assert_eq!(cumulative_to_u128(&after.cumulative_total), 10_000);
    }

    // ── Test: depositor (non-admin) deposits reconcile correctly ─────────────

    #[test]
    fn reconciliation_depositor_deposit_reconciles() {
        let e = Env::default();
        let (client, admin, token_id, _signer) = setup(&e);
        let token_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);

        let depositor = Address::generate(&e);
        token_client.mint(&depositor, &5_000);
        client.add_depositor(&depositor);

        client.receive_fee(&depositor, &5_000, &FundSource::SlashedFunds);

        let snap = snapshot(&client, &token_id);
        assert_all_invariants(&snap, "depositor deposit");
        assert_eq!(snap.total_balance, 5_000);
        assert_eq!(snap.slashed_balance, 5_000);
    }

    // ── Test: interleaved source deposits with proportional withdrawals ───────

    #[test]
    fn reconciliation_uneven_ratio_multiple_withdrawals() {
        let e = Env::default();
        let (client, admin, token_id, signer) = setup(&e);

        // ProtocolFee=3, SlashedFunds=7, Total=10
        client.receive_fee(&admin, &3, &FundSource::ProtocolFee);
        client.receive_fee(&admin, &7, &FundSource::SlashedFunds);

        // Withdraw 1 at a time, 9 times (leaving 1).
        let labels = [
            "uneven 0",
            "uneven 1",
            "uneven 2",
            "uneven 3",
            "uneven 4",
            "uneven 5",
            "uneven 6",
            "uneven 7",
            "uneven 8",
        ];

        for i in 0..9 {
            let before = snapshot(&client, &token_id);
            execute_full_withdrawal(&client, &signer, 1);
            let after = snapshot(&client, &token_id);
            assert_all_invariants(&after, labels[i]);
            assert_eq!(after.total_balance, before.total_balance - 1);
        }

        let final_snap = snapshot(&client, &token_id);
        assert_all_invariants(&final_snap, "uneven ratio final");
        assert_eq!(final_snap.total_balance, 1);
        // Cumulative should still be 10.
        assert_eq!(cumulative_to_u128(&final_snap.cumulative_total), 10);
    }
}
