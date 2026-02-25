use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod basic_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_role_hierarchy() {
        let env = Env::default();

        // Test role comparisons
        assert!(AdminRole::SuperAdmin > AdminRole::Admin);
        assert!(AdminRole::Admin > AdminRole::Operator);
        assert!(AdminRole::SuperAdmin > AdminRole::Operator);

        // Test role equality
        assert_eq!(AdminRole::SuperAdmin, AdminRole::SuperAdmin);
        assert_eq!(AdminRole::Admin, AdminRole::Admin);
        assert_eq!(AdminRole::Operator, AdminRole::Operator);

        // Test role inequality
        assert!(AdminRole::SuperAdmin != AdminRole::Admin);
        assert!(AdminRole::Admin != AdminRole::Operator);
        assert!(AdminRole::SuperAdmin != AdminRole::Operator);
    }

    #[test]
    fn test_admin_info_creation() {
        let env = Env::default();
        let address = Address::generate(&env);
        let assigned_by = Address::generate(&env);

        let admin_info = AdminInfo {
            address: address.clone(),
            role: AdminRole::Admin,
            assigned_at: 12345,
            assigned_by: assigned_by.clone(),
            active: true,
        };

        assert_eq!(admin_info.address, address);
        assert_eq!(admin_info.role, AdminRole::Admin);
        assert_eq!(admin_info.assigned_at, 12345);
        assert_eq!(admin_info.assigned_by, assigned_by);
        assert!(admin_info.active);
    }

    #[test]
    fn test_required_role_to_assign() {
        // Test that SuperAdmin can assign any role
        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::SuperAdmin),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::Admin),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::Operator),
            AdminRole::Admin
        );
    }

    #[test]
    fn test_role_assignment_logic() {
        let _env = Env::default();

        // Test role assignment requirements
        // SuperAdmin can assign: SuperAdmin, Admin, Operator
        // Admin can assign: Operator
        // Operator cannot assign anything

        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::SuperAdmin),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::Admin),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            AdminContract::get_required_role_to_assign(AdminRole::Operator),
            AdminRole::Admin
        );
    }
}
