-- Billing: a manual transfer the operator verifies.
--
-- D4 decided a payment is a row carrying the amount, the method, the proof and
-- the period it covers; one `activate()` writes the subscription, called by the
-- operator CLI today and by a gateway webhook later. No card auto-debit.
--
-- Access is derived from dates — `trial_ends_at` and `paid_until` — so there is
-- no status column that can drift out of sync. A verified payment is never
-- rewritten, the same way a sale never rewrites yesterday's price.

alter table tenants add column paid_until timestamptz;

comment on column tenants.paid_until is
  'End of the last verified paid period. Access runs to max(trial_ends_at, paid_until) plus the grace window.';

create table payments (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    amount numeric(18,2) not null check (amount > 0),
    -- transfer, qris, cash. Validated by the API so adding one is a code
    -- change, not a migration.
    method text not null,
    -- Bank reference, gateway id, or a link to the screenshot. The operator
    -- checks it by hand before activating.
    reference text,
    -- What the payment buys: the amount alone cannot say that.
    plan text not null check (plan in ('starter', 'pro', 'business')),
    period_start date not null,
    period_end date not null,
    status text not null default 'pending'
        check (status in ('pending', 'verified', 'rejected')),
    note text,
    created_by uuid references users (id) on delete set null,
    -- Free text, not a user reference: the operator who verifies is not a
    -- member of the tenant and has no row in `users`.
    verified_by text,
    verified_at timestamptz,
    created_at timestamptz not null default now(),
    check (period_end > period_start),
    -- A verified payment always says when it was verified.
    check (status <> 'verified' or verified_at is not null)
);

create index payments_tenant_idx on payments (tenant_id, created_at desc);
-- The operator works through the pending list.
create index payments_pending_idx on payments (created_at) where status = 'pending';

alter table payments enable row level security;
alter table payments force row level security;
create policy payments_tenant on payments
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());
