-- Production orders consume material batches and add finished goods.
--
-- This is the only place that writes `production_use` movements. Each
-- consumption snapshots the batch cost at the moment of use, so historical
-- HPP never changes when a new purchase arrives at a different price. The
-- cost columns on the order stay null until P2-05 computes HPP.

create table production_orders (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    product_id uuid not null,
    recipe_id uuid not null,
    planned_qty numeric(18,3) not null check (planned_qty > 0),
    actual_qty numeric(18,3) not null check (actual_qty > 0),
    produced_on date not null,
    -- Filled by P2-05 from the consumptions plus labor and overhead.
    material_cost numeric(18,2) check (material_cost >= 0),
    labor_cost numeric(18,2) check (labor_cost >= 0),
    overhead_cost numeric(18,2) check (overhead_cost >= 0),
    unit_cost numeric(18,4) check (unit_cost >= 0),
    status text not null default 'done' check (status in ('done', 'void')),
    created_by uuid references users (id) on delete set null,
    created_at timestamptz not null default now(),
    constraint production_orders_product_tenant
        foreign key (product_id, tenant_id)
        references products (id, tenant_id)
        on delete cascade,
    -- The order must consume a version of the product it produces. 0009 made
    -- (id, product_id) unique on recipes for exactly this check.
    constraint production_orders_recipe_product
        foreign key (recipe_id, product_id)
        references recipes (id, product_id)
);

create index production_orders_recent_idx
    on production_orders (tenant_id, produced_on desc, created_at desc);

alter table production_orders enable row level security;
alter table production_orders force row level security;
create policy production_orders_tenant on production_orders
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

create table production_consumptions (
    production_order_id uuid not null
        references production_orders (id) on delete cascade,
    batch_id uuid not null references material_batches (id) on delete restrict,
    qty numeric(18,3) not null check (qty > 0),
    -- Batch cost when the material was used, not its cost today.
    unit_cost numeric(18,4) not null check (unit_cost >= 0),
    primary key (production_order_id, batch_id)
);

create index production_consumptions_batch_idx on production_consumptions (batch_id);

-- Child table without tenant_id: isolate through the parent order. The
-- subquery is itself filtered by the production_orders policy, so a row is
-- visible only when its order belongs to the active tenant.
alter table production_consumptions enable row level security;
alter table production_consumptions force row level security;
create policy production_consumptions_tenant on production_consumptions
    for all
    using (exists (
        select 1 from production_orders o
        where o.id = production_order_id and o.tenant_id = app_tenant_id()
    ))
    with check (exists (
        select 1 from production_orders o
        where o.id = production_order_id and o.tenant_id = app_tenant_id()
    ));
