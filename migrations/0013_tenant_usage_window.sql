-- How far back purchase recommendations look for usage.
--
-- 30 days smooths a week of service; a stable menu can shorten it to react
-- faster, a seasonal one can lengthen it. The window belongs to the tenant
-- because the right answer depends on how often the menu changes.

alter table tenants
    add column usage_window_days int not null default 30
    check (usage_window_days between 1 and 365);
