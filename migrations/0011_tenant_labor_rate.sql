-- Tenant settings that feed cost.
--
-- The rate is what one hour of production labor costs this tenant. Recipes
-- carry labor_minutes for one batch; production multiplies the two so the HPP
-- includes labor. Zero means the tenant has not set a rate yet, and orders keep
-- snapshotting material cost only.

alter table tenants
    add column labor_rate_per_hour numeric(18,2) not null default 0
    check (labor_rate_per_hour >= 0);
