use crate::*;
use soroban_sdk::{Address, Env};

mod pausable_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, AdminContractClient<'static>, Address) {
        let e = Env::default();
        let contract_id = e.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&e, &contract_id);
        let super_admin = Address::generate(&e);
        e.mock_all_auths();
        client.initialize(&super_admin, &1u32, &100u32);
        (e, client, super_admin)
    }

    // -------------------------------------------------------------------------
    // Existing baseline tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_pause_blocks_state_changes_but_allows_reads() {
        let (e, client, super_admin) = setup();

        assert!(!client.is_paused());
        client.pause(&super_admin);
        assert!(client.is_paused());

        // Read should still work
        assert_eq!(client.get_admin_count(), 1);

        // State changes should fail
        let new_admin = Address::generate(&e);
        assert!(client
            .try_add_admin(&super_admin, &new_admin, &AdminRole::Admin)
            .is_err());

        client.unpause(&super_admin);
        assert!(!client.is_paused());

        client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
        assert_eq!(client.get_admin_count(), 2);
    }

    #[test]
    fn test_pause_multisig_flow() {
        let (e, client, super_admin) = setup();

        let s1 = Address::generate(&e);
        let s2 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s2, &true);
        client.set_pause_threshold(&super_admin, &2u32);

        let pid = client.pause(&s1).unwrap();
        assert!(!client.is_paused());

        client.approve_pause_proposal(&s2, &pid);
        client.execute_pause_proposal(&pid);
        assert!(client.is_paused());

        let pid2 = client.unpause(&s1).unwrap();
        client.approve_pause_proposal(&s2, &pid2);
        client.execute_pause_proposal(&pid2);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_execute_requires_threshold() {
        let (e, client, super_admin) = setup();

        let s1 = Address::generate(&e);
        let s2 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s2, &true);
        client.set_pause_threshold(&super_admin, &2u32);

        let pid = client.pause(&s1).unwrap();

        assert!(client.try_execute_pause_proposal(&pid).is_err());

        client.approve_pause_proposal(&s2, &pid);
        client.execute_pause_proposal(&pid);
        assert!(client.is_paused());
    }

    // -------------------------------------------------------------------------
    // Adversarial regression cases
    // -------------------------------------------------------------------------

    /// Non-SuperAdmin cannot pause when threshold == 0;
    /// non-pause-signer cannot initiate when threshold > 0.
    #[test]
    fn test_unauthorized_pause_actions() {
        let (e, client, super_admin) = setup();
        let random_user = Address::generate(&e);

        // threshold == 0: requires SuperAdmin
        let res = client.try_pause(&random_user);
        assert!(res.is_err(), "non-admin must not pause at threshold=0");

        // Raise threshold but don't register random_user as signer
        client.set_pause_signer(&super_admin, &Address::generate(&e), &true);
        client.set_pause_threshold(&super_admin, &1u32);

        // threshold > 0: requires PauseSigner
        let res2 = client.try_pause(&random_user);
        assert!(
            res2.is_err(),
            "non-signer must not propose pause at threshold>0"
        );
    }

    /// Zero address and the contract's own address must be rejected as signers.
    #[test]
    fn test_invalid_signers_rejected() {
        let (e, client, super_admin) = setup();

        let zero_addr = Address::from_string(&soroban_sdk::String::from_str(
            &e,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        let self_addr = client.address.clone();

        assert!(
            client
                .try_set_pause_signer(&super_admin, &zero_addr, &true)
                .is_err(),
            "zero address must be rejected"
        );
        assert!(
            client
                .try_set_pause_signer(&super_admin, &self_addr, &true)
                .is_err(),
            "contract self-address must be rejected"
        );
    }

    /// set_pause_signer and set_pause_threshold must be idempotent.
    #[test]
    fn test_signer_and_threshold_idempotency() {
        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);

        // Enabling an already-enabled signer must not panic or double-count
        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s1, &true);

        // Disabling a signer that was never enabled must not panic
        let ghost = Address::generate(&e);
        client.set_pause_signer(&super_admin, &ghost, &false);

        // Threshold above signer count must fail
        assert!(
            client.try_set_pause_threshold(&super_admin, &2u32).is_err(),
            "threshold > signer count must be rejected"
        );

        // Same threshold twice must not panic
        client.set_pause_threshold(&super_admin, &1u32);
        client.set_pause_threshold(&super_admin, &1u32);
    }

    /// Removing a signer automatically clamps the threshold so the contract
    /// cannot become permanently unpauseable.
    #[test]
    fn test_threshold_clamped_on_signer_removal() {
        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);
        let s2 = Address::generate(&e);
        let s3 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s2, &true);
        client.set_pause_signer(&super_admin, &s3, &true);
        // threshold == 3: every signer must approve
        client.set_pause_threshold(&super_admin, &3u32);

        // After removing s3 the threshold must drop to 2, not remain at 3
        client.set_pause_signer(&super_admin, &s3, &false);

        // With 2 signers and auto-clamped threshold of 2, a full round must succeed
        let pid = client.pause(&s1).unwrap();
        client.approve_pause_proposal(&s2, &pid);
        client.execute_pause_proposal(&pid);
        assert!(
            client.is_paused(),
            "threshold clamp must allow execution with remaining signers"
        );
    }

    /// Duplicate approvals from the same signer must not inflate the count.
    /// Executing before threshold must fail; after threshold must succeed.
    /// Idempotent pause/unpause at threshold==0 must not panic.
    #[test]
    fn test_pause_idempotency_and_duplicate_approvals() {
        // --- multisig path ---
        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);
        let s2 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s2, &true);
        client.set_pause_threshold(&super_admin, &2u32);

        let pid = client.pause(&s1).unwrap();

        // s1 already voted during propose; a second approval from s1 is a no-op
        client.approve_pause_proposal(&s1, &pid);

        // Only 1 distinct signer (s1) has approved → execution must fail
        assert!(
            client.try_execute_pause_proposal(&pid).is_err(),
            "must not execute before threshold is met"
        );

        // s2 approves
        client.approve_pause_proposal(&s2, &pid);
        // Duplicate from s2 must be idempotent (no double-count)
        client.approve_pause_proposal(&s2, &pid);

        // Now threshold is met → execution must succeed
        client.execute_pause_proposal(&pid);
        assert!(client.is_paused());

        // --- single-admin (threshold==0) path ---
        let (_, client2, super_admin2) = setup();

        // Unpause when already unpaused → must not panic
        client2.unpause(&super_admin2);
        assert!(!client2.is_paused());

        // Pause then pause again → second call must not panic
        client2.pause(&super_admin2);
        assert!(client2.is_paused());
        client2.pause(&super_admin2);
        assert!(client2.is_paused());
    }

    /// Approve/execute after the epoch boundary must fail with StaleAdminEpoch.
    #[test]
    fn test_approve_after_epoch_boundary_fails() {
        use crate::pausable::PROPOSAL_EPOCH_SIZE;
        use soroban_sdk::testutils::Ledger as _;

        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);
        let s2 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_signer(&super_admin, &s2, &true);
        client.set_pause_threshold(&super_admin, &2u32);

        // Propose at the last ledger of epoch 0
        let boundary = u32::from(PROPOSAL_EPOCH_SIZE);
        e.ledger().with_mut(|l| {
            l.sequence_number = boundary - 1;
        });
        let pid = client.pause(&s1).unwrap();

        // Cross into epoch 1
        e.ledger().with_mut(|l| {
            l.sequence_number = boundary;
        });

        // Approval from epoch 1 must fail
        let res = client.try_approve_pause_proposal(&s2, &pid);
        assert!(
            res.is_err(),
            "approval in a new epoch must fail with StaleAdminEpoch"
        );
    }

    /// A non-signer must not be able to approve any proposal.
    #[test]
    fn test_non_signer_cannot_approve() {
        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);
        let intruder = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_threshold(&super_admin, &1u32);

        let pid = client.pause(&s1).unwrap();

        // intruder is not a registered signer
        let res = client.try_approve_pause_proposal(&intruder, &pid);
        assert!(res.is_err(), "non-signer must not approve a proposal");
    }

    /// Approving or executing a non-existent proposal must fail.
    #[test]
    fn test_nonexistent_proposal_rejected() {
        let (e, client, super_admin) = setup();
        let s1 = Address::generate(&e);

        client.set_pause_signer(&super_admin, &s1, &true);
        client.set_pause_threshold(&super_admin, &1u32);

        let fake_id: u64 = 0xdeadbeef_cafebabe;

        assert!(
            client.try_approve_pause_proposal(&s1, &fake_id).is_err(),
            "approving phantom proposal must fail"
        );
        assert!(
            client.try_execute_pause_proposal(&fake_id).is_err(),
            "executing phantom proposal must fail"
        );
    }
}
