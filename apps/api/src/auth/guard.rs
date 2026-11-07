//! Route guards that turn the pure RBAC matrix into extractors.
//!
//! A handler declares the permission it needs in its signature, so a route
//! cannot mutate state without naming one. The matrix itself lives in
//! takar-domain; this module only applies it.

use std::marker::PhantomData;

use axum::{extract::FromRequestParts, http::request::Parts};
use takar_domain::Permission;

use super::extract::AuthContext;
use crate::{error::ApiError, AppState};

/// Binds a marker type to the permission it represents.
pub trait PermissionSet: Send + Sync {
    const PERMISSION: Permission;
}

/// Extracts an AuthContext and rejects the request unless the caller's role
/// holds the permission declared by P.
pub struct Require<P: PermissionSet> {
    pub auth: AuthContext,
    marker: PhantomData<P>,
}

impl<P: PermissionSet> FromRequestParts<AppState> for Require<P> {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthContext::from_request_parts(parts, state).await?;

        if !auth.role.can(P::PERMISSION) {
            return Err(ApiError::Forbidden("permission_denied"));
        }

        Ok(Self {
            auth,
            marker: PhantomData,
        })
    }
}

macro_rules! permissions {
    ($($marker:ident => $permission:ident),* $(,)?) => {
        $(
            /// Route guard marker.
            #[derive(Debug, Clone, Copy)]
            pub struct $marker;

            impl PermissionSet for $marker {
                const PERMISSION: Permission = Permission::$permission;
            }
        )*
    };
}

permissions! {
    CanReadMaterials => MaterialRead,
    CanWriteMaterials => MaterialWrite,
    CanWriteBatches => BatchWrite,
    CanAdjustStock => StockAdjust,
    CanReadRecipes => RecipeRead,
    CanWriteRecipes => RecipeWrite,
    CanReadProduction => ProductionRead,
    CanWriteProduction => ProductionWrite,
    CanReadSales => SalesRead,
    CanWriteSales => SalesWrite,
    CanReadReports => ReportRead,
    CanReadMembers => MemberRead,
    CanInviteMembers => MemberInvite,
    CanManageMembers => MemberManage,
    CanChangeRoles => MemberRoleChange,
    CanManageBilling => BillingManage,
    CanManageTenant => TenantManage,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission_of<P: PermissionSet>(_: &P) -> Permission {
        P::PERMISSION
    }

    #[test]
    fn markers_cover_the_whole_matrix() {
        let declared = [
            permission_of(&CanReadMaterials),
            permission_of(&CanWriteMaterials),
            permission_of(&CanWriteBatches),
            permission_of(&CanAdjustStock),
            permission_of(&CanReadRecipes),
            permission_of(&CanWriteRecipes),
            permission_of(&CanReadProduction),
            permission_of(&CanWriteProduction),
            permission_of(&CanReadSales),
            permission_of(&CanWriteSales),
            permission_of(&CanReadReports),
            permission_of(&CanReadMembers),
            permission_of(&CanInviteMembers),
            permission_of(&CanManageMembers),
            permission_of(&CanChangeRoles),
            permission_of(&CanManageBilling),
            permission_of(&CanManageTenant),
        ];

        assert_eq!(declared.len(), Permission::ALL.len());
        for permission in Permission::ALL {
            assert!(
                declared.contains(&permission),
                "no guard declares {}",
                permission.as_str()
            );
        }
    }
}
