-- Alert rules and the alert queue.
--
-- A rule says when to warn: expiry warnings N days before a batch expires, or
-- when a material drops below its minimum stock. The daily job turns rules
-- into alerts. Alerts are structured on purpose; the WhatsApp text is composed
-- by the sender (P1-07), not stored here.

create table alert_rules (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    type text not null check (type in ('expiry', 'low_stock')),
    days_before int check (days_before is null or days_before >= 0),
    channel text not null default 'whatsapp' check (channel in ('whatsapp', 'email')),
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    constraint alert_rules_shape check (
        (type = 'expiry' and days_before is not null)
        or (type = 'low_stock' and days_before is null)
    )
);

create unique index alert_rules_expiry_unique on alert_rules (tenant_id, days_before)
    where type = 'expiry';
create unique index alert_rules_low_stock_unique on alert_rules (tenant_id)
    where type = 'low_stock';

create table alerts (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    rule_id uuid not null references alert_rules (id) on delete cascade,
    type text not null check (type in ('expiry', 'low_stock')),
    material_id uuid not null references materials (id) on delete cascade,
    batch_id uuid references material_batches (id) on delete cascade,
    created_at timestamptz not null default now(),
    sent_at timestamptz,
    resolved_at timestamptz,
    constraint alerts_shape check (
        (type = 'expiry' and batch_id is not null)
        or (type = 'low_stock' and batch_id is null)
    )
);

-- One open alert per rule and subject. Resolved alerts stay for history but do
-- not block a new one, so a material that recovers and drops again warns again.
create unique index alerts_open_expiry on alerts (tenant_id, rule_id, batch_id)
    where resolved_at is null and batch_id is not null;
create unique index alerts_open_low_stock on alerts (tenant_id, rule_id, material_id)
    where resolved_at is null and batch_id is null;

create index alerts_open_idx on alerts (tenant_id, created_at desc)
    where resolved_at is null;

-- Default rules for tenants that already exist. New tenants get the same rows
-- when they are created; this backfill only matters for databases that already
-- hold data.
insert into alert_rules (tenant_id, type, days_before)
select t.id, 'expiry', days.days
from tenants t
cross join (values (7), (3)) as days (days);

insert into alert_rules (tenant_id, type)
select id, 'low_stock' from tenants;

alter table alert_rules enable row level security;
alter table alert_rules force row level security;
create policy alert_rules_tenant on alert_rules
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

alter table alerts enable row level security;
alter table alerts force row level security;
create policy alerts_tenant on alerts
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());
