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
}
