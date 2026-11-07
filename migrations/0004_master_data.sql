-- Master data: units, global conversions, suppliers, and materials.
--
-- Units and conversions are system-wide reference data: tenant_id stays null
-- for now and tenant-specific units are deferred. Suppliers and materials are
-- tenant scoped and protected by RLS.
--
-- Decision: unit_conversions are global (see docs/ROADMAP.md, schema issue 2).

create table units (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid references tenants (id) on delete cascade,
    code text not null check (code = lower(btrim(code)) and length(code) between 1 and 12),
    name text not null check (length(btrim(name)) between 1 and 40),
    created_at timestamptz not null default now()
);

-- One code per scope; null tenant_id means a system unit.
create unique index units_code_unique
    on units (coalesce(tenant_id, '00000000-0000-0000-0000-000000000000'::uuid), code);

insert into units (code, name) values
    ('g', 'Gram'),
    ('kg', 'Kilogram'),
    ('ml', 'Millilitre'),
    ('l', 'Litre'),
    ('pcs', 'Piece'),
    ('lusin', 'Dozen');

create table unit_conversions (
    id uuid primary key default gen_random_uuid(),
    from_unit_id uuid not null references units (id) on delete cascade,
    to_unit_id uuid not null references units (id) on delete cascade,
    factor numeric(18,6) not null check (factor > 0),
    constraint unit_conversions_distinct check (from_unit_id <> to_unit_id),
    constraint unit_conversions_pair unique (from_unit_id, to_unit_id)
);

-- Canonical direction only. Callers invert when converting the other way.
insert into unit_conversions (from_unit_id, to_unit_id, factor)
select source.id, target.id, seed.factor
from (
    values ('kg', 'g', 1000), ('l', 'ml', 1000), ('lusin', 'pcs', 12)
) as seed (from_code, to_code, factor)
join units source on source.code = seed.from_code and source.tenant_id is null
join units target on target.code = seed.to_code and target.tenant_id is null;

create table suppliers (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    name text not null check (length(btrim(name)) between 1 and 120),
    phone text,
    lead_time_days int not null default 2 check (lead_time_days >= 0),
    notes text,
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create unique index suppliers_name_unique on suppliers (tenant_id, lower(name));
create index suppliers_active_idx on suppliers (tenant_id) where is_active;

alter table suppliers enable row level security;
alter table suppliers force row level security;
create policy suppliers_tenant on suppliers
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

create table materials (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    name text not null check (length(btrim(name)) between 1 and 120),
    base_unit_id uuid not null references units (id),
    min_stock numeric(18,3) not null default 0 check (min_stock >= 0),
    shelf_life_days int check (shelf_life_days is null or shelf_life_days >= 0),
    default_supplier_id uuid references suppliers (id) on delete set null,
    -- Read-only cache of the latest batch cost; batches stay the source of truth.
    last_cost numeric(18,4) not null default 0 check (last_cost >= 0),
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create unique index materials_name_unique on materials (tenant_id, lower(name));
create index materials_active_idx on materials (tenant_id) where is_active;

alter table materials enable row level security;
alter table materials force row level security;
create policy materials_tenant on materials
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

-- Reference data is readable by every tenant; only the system seeds it.
-- Deployment must grant select to the application role:
--   grant select on units, unit_conversions to takar_app;
