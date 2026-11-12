-- A new tenant starts a 14-day trial.
--
-- The end date is stored rather than derived from created_at, so support can
-- extend a trial without rewriting when the tenant was created. Expiry itself
-- (read-only mode) lands with billing; this migration only gives every tenant
-- a date to compare against.

alter table tenants
    alter column trial_ends_at set default now() + interval '14 days';

update tenants
set trial_ends_at = created_at + interval '14 days'
where trial_ends_at is null;
