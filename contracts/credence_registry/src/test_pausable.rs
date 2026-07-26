use crate::*;
use soroban_sdk::{Address, Env};

mod pausable_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, CredenceRegistryClient<'static>, Address) {
        let e = Env::default();
        let contract_id = e.register_contract(None, CredenceRegistry);
        let client = CredenceRegistryClient::new(&e, &contract_id);
        let admin = Address::generate(&e);
        e.mock_all_auths();
        client.initialize(&admin);
        (e, client, admin)
    }

    #[test]
    fn test_get_pause_state_defaults_after_init() {
        let (_e, client, _admin) = setup();

        let state = client.get_pause_state();

        // After initialization the contract should not be paused
        assert!(!state.is_paused);
        // No signers configured yet
        assert_eq!(state.signer_count, 0);
        // Threshold defaults to 0 (admin-direct pause)
        assert_eq!(state.threshold, 0);
    }

    #[test]
    fn test_get_pause_state_reflects_signers_and_threshold() {
        let (_e, client, admin) = setup();

        let s1 = Address::generate(&_e);
        let s2 = Address::generate(&_e);

        // Add two pause signers
        client.set_pause_signer(&admin, &s1, &true);
        let state = client.get_pause_state();
        assert!(!state.is_paused);
        assert_eq!(state.signer_count, 1);
        assert_eq!(state.threshold, 1); // auto-adjusted from 0 to 1

        client.set_pause_signer(&admin, &s2, &true);
        let state = client.get_pause_state();
        assert_eq!(state.signer_count, 2);
        assert_eq!(state.threshold, 1); // unchanged

        // Raise the threshold
        client.set_pause_threshold(&admin, &2u32);
        let state = client.get_pause_state();
        assert_eq!(state.signer_count, 2);
        assert_eq!(state.threshold, 2);
    }

    #[test]
    fn test_get_pause_state_reflects_pause_and_unpause() {
        let (_e, client, admin) = setup();

        // Direct admin pause (threshold = 0)
        client.pause(&admin);
        let state = client.get_pause_state();
        assert!(state.is_paused);

        client.unpause(&admin);
        let state = client.get_pause_state();
        assert!(!state.is_paused);
    }

    #[test]
    fn test_get_pause_state_multisig_flow() {
        let (_e, client, admin) = setup();

        let s1 = Address::generate(&_e);
        let s2 = Address::generate(&_e);

        // Configure multisig pause
        client.set_pause_signer(&admin, &s1, &true);
        client.set_pause_signer(&admin, &s2, &true);
        client.set_pause_threshold(&admin, &2u32);

        let state = client.get_pause_state();
        assert!(!state.is_paused);
        assert_eq!(state.signer_count, 2);
        assert_eq!(state.threshold, 2);

        // Propose pause — not yet executed, so contract is still not paused
        let pid = client.pause(&s1).unwrap();
        let state = client.get_pause_state();
        assert!(!state.is_paused, "contract should not be paused before threshold met");

        // Second signer approves, then execute — now the contract pauses
        client.approve_pause_proposal(&s2, &pid);
        client.execute_pause_proposal(&pid);
        let state = client.get_pause_state();
        assert!(state.is_paused, "contract should be paused after execution");

        // Unpause via multisig
        let pid2 = client.unpause(&s1).unwrap();
        let state_after_proposed = client.get_pause_state();
        assert!(
            state_after_proposed.is_paused,
            "contract stays paused until unpause proposal is executed"
        );

        client.approve_pause_proposal(&s2, &pid2);
        client.execute_pause_proposal(&pid2);
        let state = client.get_pause_state();
        assert!(!state.is_paused, "contract should be unpaused after execution");
    }

    #[test]
    fn test_state_changes_blocked_when_paused() {
        let (_e, client, admin) = setup();

        // Pause the contract
        client.pause(&admin);
        let state = client.get_pause_state();
        assert!(state.is_paused);

        // State-changing operations should fail when paused
        let identity = Address::generate(&_e);
        let bond_contract = Address::generate(&_e);
        assert!(
            client.try_register(&identity, &bond_contract, &false).is_err(),
            "register should fail when paused"
        );
        assert!(
            client.try_deactivate(&identity).is_err(),
            "deactivate should fail when paused"
        );

        // Read-only operations should still succeed
        assert!(!client.is_registered(&identity));
    }
}
