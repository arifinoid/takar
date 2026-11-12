//! Subscription plans and their usage caps.
//!
//! Limits are entitlements, not permissions: a role decides what a member may
//! do, a plan decides how much of it the tenant may have. They are checked in
//! the service layer before a create, never in RLS, and only on create — a
//! downgrade must not delete or hide rows that already exist.
//!
//! The numbers are an initial hypothesis from the product plan, not final
//! pricing. `None` means unlimited.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Trial,
    Starter,
    Pro,
    Business,
}

/// What a plan puts a ceiling on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Users,
    Materials,
    Recipes,
    Warehouses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanLimits {
    pub users: Option<i64>,
    pub materials: Option<i64>,
    pub recipes: Option<i64>,
    pub warehouses: Option<i64>,
}

const TRIAL: PlanLimits = PlanLimits {
    users: Some(1),
    materials: Some(20),
    recipes: Some(5),
    warehouses: Some(1),
};

const STARTER: PlanLimits = PlanLimits {
    users: Some(2),
    materials: Some(50),
    recipes: Some(15),
    warehouses: Some(1),
};

const PRO: PlanLimits = PlanLimits {
    users: Some(5),
    materials: Some(200),
    recipes: None,
    warehouses: Some(2),
};

const BUSINESS: PlanLimits = PlanLimits {
    users: Some(20),
    materials: None,
    recipes: None,
    warehouses: Some(10),
};

impl Plan {
    pub const fn as_str(self) -> &'static str {
        match self {
            Plan::Trial => "trial",
            Plan::Starter => "starter",
            Plan::Pro => "pro",
            Plan::Business => "business",
        }
    }

    pub fn parse(value: &str) -> Option<Plan> {
        match value {
            "trial" => Some(Plan::Trial),
            "starter" => Some(Plan::Starter),
            "pro" => Some(Plan::Pro),
            "business" => Some(Plan::Business),
            _ => None,
        }
    }

    pub const fn limits(self) -> PlanLimits {
        match self {
            Plan::Trial => TRIAL,
            Plan::Starter => STARTER,
            Plan::Pro => PRO,
            Plan::Business => BUSINESS,
        }
    }
}

impl Resource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Resource::Users => "users",
            Resource::Materials => "materials",
            Resource::Recipes => "recipes",
            Resource::Warehouses => "warehouses",
        }
    }
}

impl PlanLimits {
    /// `None` when the plan does not cap this resource.
    pub const fn limit(self, resource: Resource) -> Option<i64> {
        match resource {
            Resource::Users => self.users,
            Resource::Materials => self.materials,
            Resource::Recipes => self.recipes,
            Resource::Warehouses => self.warehouses,
        }
    }

    /// The ceiling that would be hit, if the tenant is already at it.
    pub fn exceeded(self, resource: Resource, current: i64) -> Option<i64> {
        let limit = self.limit(resource)?;

        (current >= limit).then_some(limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_tenant_starts_on_the_trial_caps() {
        let limits = Plan::Trial.limits();

        assert_eq!(limits.limit(Resource::Users), Some(1));
        assert_eq!(limits.limit(Resource::Materials), Some(20));
        assert_eq!(limits.limit(Resource::Recipes), Some(5));
    }

    #[test]
    fn pro_leaves_recipes_uncapped() {
        assert_eq!(Plan::Pro.limits().limit(Resource::Recipes), None);
    }

    #[test]
    fn a_ceiling_is_hit_at_the_limit_not_after_it() {
        let limits = Plan::Trial.limits();

        assert_eq!(limits.exceeded(Resource::Materials, 19), None);
        assert_eq!(limits.exceeded(Resource::Materials, 20), Some(20));
        assert_eq!(limits.exceeded(Resource::Materials, 21), Some(20));
    }

    #[test]
    fn an_uncapped_resource_is_never_exceeded() {
        assert_eq!(
            Plan::Business
                .limits()
                .exceeded(Resource::Materials, 100_000),
            None
        );
    }

    #[test]
    fn plan_names_round_trip() {
        for plan in [Plan::Trial, Plan::Starter, Plan::Pro, Plan::Business] {
            assert_eq!(Plan::parse(plan.as_str()), Some(plan));
        }

        assert_eq!(Plan::parse("enterprise"), None);
    }
}
