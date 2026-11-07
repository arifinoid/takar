//! Role-based access control matrix.
//!
//! This module is pure: it maps a role to the permissions it holds. The API
//! enforces the result; Postgres RLS only isolates tenants. Keep role checks
//! out of `takar-core`, which owns calculation only.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Production,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    MaterialRead,
    MaterialWrite,
    BatchWrite,
    StockAdjust,
    RecipeRead,
    RecipeWrite,
    ProductionRead,
    ProductionWrite,
    SalesRead,
    SalesWrite,
    ReportRead,
    MemberRead,
    MemberInvite,
    MemberManage,
    MemberRoleChange,
    BillingManage,
    TenantManage,
}

impl Permission {
    pub const ALL: [Permission; 17] = [
        Permission::MaterialRead,
        Permission::MaterialWrite,
        Permission::BatchWrite,
        Permission::StockAdjust,
        Permission::RecipeRead,
        Permission::RecipeWrite,
        Permission::ProductionRead,
        Permission::ProductionWrite,
        Permission::SalesRead,
        Permission::SalesWrite,
        Permission::ReportRead,
        Permission::MemberRead,
        Permission::MemberInvite,
        Permission::MemberManage,
        Permission::MemberRoleChange,
        Permission::BillingManage,
        Permission::TenantManage,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Permission::MaterialRead => "material_read",
            Permission::MaterialWrite => "material_write",
            Permission::BatchWrite => "batch_write",
            Permission::StockAdjust => "stock_adjust",
            Permission::RecipeRead => "recipe_read",
            Permission::RecipeWrite => "recipe_write",
            Permission::ProductionRead => "production_read",
            Permission::ProductionWrite => "production_write",
            Permission::SalesRead => "sales_read",
            Permission::SalesWrite => "sales_write",
            Permission::ReportRead => "report_read",
            Permission::MemberRead => "member_read",
            Permission::MemberInvite => "member_invite",
            Permission::MemberManage => "member_manage",
            Permission::MemberRoleChange => "member_role_change",
            Permission::BillingManage => "billing_manage",
            Permission::TenantManage => "tenant_manage",
        }
    }
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Owner, Role::Admin, Role::Production, Role::Viewer];

    pub const fn permissions(self) -> &'static [Permission] {
        match self {
            Role::Owner => &[
                Permission::MaterialRead,
                Permission::MaterialWrite,
                Permission::BatchWrite,
                Permission::StockAdjust,
                Permission::RecipeRead,
                Permission::RecipeWrite,
                Permission::ProductionRead,
                Permission::ProductionWrite,
                Permission::SalesRead,
                Permission::SalesWrite,
                Permission::ReportRead,
                Permission::MemberRead,
                Permission::MemberInvite,
                Permission::MemberManage,
                Permission::MemberRoleChange,
                Permission::BillingManage,
                Permission::TenantManage,
            ],
            Role::Admin => &[
                Permission::MaterialRead,
                Permission::MaterialWrite,
                Permission::BatchWrite,
                Permission::StockAdjust,
                Permission::RecipeRead,
                Permission::RecipeWrite,
                Permission::ProductionRead,
                Permission::ProductionWrite,
                Permission::SalesRead,
                Permission::SalesWrite,
                Permission::ReportRead,
                Permission::MemberRead,
                Permission::MemberInvite,
                Permission::MemberManage,
            ],
            Role::Production => &[
                Permission::MaterialRead,
                Permission::BatchWrite,
                Permission::RecipeRead,
                Permission::ProductionRead,
                Permission::ProductionWrite,
                Permission::SalesRead,
                Permission::ReportRead,
                Permission::MemberRead,
            ],
            Role::Viewer => &[
                Permission::MaterialRead,
                Permission::RecipeRead,
                Permission::ProductionRead,
                Permission::SalesRead,
                Permission::ReportRead,
                Permission::MemberRead,
            ],
        }
    }

    pub fn can(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }

    /// Ranks roles for management checks. Higher outranks lower.
    pub const fn rank(self) -> u8 {
        match self {
            Role::Owner => 3,
            Role::Admin => 2,
            Role::Production => 1,
            Role::Viewer => 0,
        }
    }

    /// Whether this role may act on a member holding `target`, including
    /// assigning `target` to someone else.
    ///
    /// Everyone is limited to roles strictly below their own, so an admin
    /// cannot manage another admin. The one exception is owner over owner:
    /// a tenant must be able to hand over or remove a co-owner.
    pub const fn can_manage(self, target: Role) -> bool {
        self.rank() > target.rank() || matches!((self, target), (Role::Owner, Role::Owner))
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Production => "production",
            Role::Viewer => "viewer",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseRoleError;

impl fmt::Display for ParseRoleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown role")
    }
}

impl std::error::Error for ParseRoleError {}

impl FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "owner" => Ok(Role::Owner),
            "admin" => Ok(Role::Admin),
            "production" => Ok(Role::Production),
            "viewer" => Ok(Role::Viewer),
            _ => Err(ParseRoleError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(permissions: &[Permission]) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = permissions
            .iter()
            .map(|permission| permission.as_str())
            .collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn owner_holds_every_permission() {
        for permission in Permission::ALL {
            assert!(
                Role::Owner.can(permission),
                "owner is missing {}",
                permission.as_str()
            );
        }
    }

    #[test]
    fn roles_are_monotonic() {
        for permission in Role::Viewer.permissions() {
            assert!(Role::Production.can(*permission));
        }
        for permission in Role::Production.permissions() {
            assert!(Role::Admin.can(*permission));
        }
        for permission in Role::Admin.permissions() {
            assert!(Role::Owner.can(*permission));
        }
    }

    #[test]
    fn viewer_is_read_only() {
        assert!(!Role::Viewer.can(Permission::MaterialWrite));
        assert!(!Role::Viewer.can(Permission::BatchWrite));
        assert!(!Role::Viewer.can(Permission::ProductionWrite));
        assert!(!Role::Viewer.can(Permission::MemberInvite));
        assert!(!Role::Viewer.can(Permission::TenantManage));
    }

    #[test]
    fn admin_cannot_change_roles_or_billing() {
        assert!(!Role::Admin.can(Permission::MemberRoleChange));
        assert!(!Role::Admin.can(Permission::BillingManage));
        assert!(!Role::Admin.can(Permission::TenantManage));
    }

    #[test]
    fn no_role_lists_a_permission_twice() {
        for role in Role::ALL {
            let mut seen = std::collections::HashSet::new();
            for permission in role.permissions() {
                assert!(
                    seen.insert(*permission),
                    "{} lists {} twice",
                    role.as_str(),
                    permission.as_str()
                );
            }
        }
    }

    #[test]
    fn matrix_matches_the_documented_contract() {
        assert_eq!(
            names(Role::Viewer.permissions()),
            vec![
                "material_read",
                "member_read",
                "production_read",
                "recipe_read",
                "report_read",
                "sales_read",
            ]
        );
        assert_eq!(
            names(Role::Production.permissions()),
            vec![
                "batch_write",
                "material_read",
                "member_read",
                "production_read",
                "production_write",
                "recipe_read",
                "report_read",
                "sales_read",
            ]
        );
        assert_eq!(
            names(Role::Admin.permissions()),
            vec![
                "batch_write",
                "material_read",
                "material_write",
                "member_invite",
                "member_manage",
                "member_read",
                "production_read",
                "production_write",
                "recipe_read",
                "recipe_write",
                "report_read",
                "sales_read",
                "sales_write",
                "stock_adjust",
            ]
        );
        assert_eq!(
            names(Role::Owner.permissions()).len(),
            Permission::ALL.len()
        );
    }

    #[test]
    fn roles_only_manage_roles_below_themselves() {
        assert!(Role::Owner.can_manage(Role::Owner));
        assert!(Role::Owner.can_manage(Role::Admin));
        assert!(Role::Owner.can_manage(Role::Production));
        assert!(Role::Owner.can_manage(Role::Viewer));

        assert!(!Role::Admin.can_manage(Role::Owner));
        assert!(!Role::Admin.can_manage(Role::Admin));
        assert!(Role::Admin.can_manage(Role::Production));
        assert!(Role::Admin.can_manage(Role::Viewer));

        assert!(!Role::Production.can_manage(Role::Production));
        assert!(Role::Production.can_manage(Role::Viewer));

        assert!(!Role::Viewer.can_manage(Role::Viewer));
    }

    #[test]
    fn roles_round_trip_through_text() {
        for role in Role::ALL {
            assert_eq!(Role::from_str(role.as_str()), Ok(role));
            assert_eq!(role.to_string(), role.as_str());
        }
        assert!(Role::from_str("superadmin").is_err());
    }
}
