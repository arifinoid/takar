-- Batches and the movement trail.
--
-- Stock is a batch ledger: material_batches.qty_remaining is on-hand per
-- batch, and stock_movements explains every change. A batch is never created
-- without its opening purchase movement.

create table material_batches (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    material_id uuid not null references materials (id),
    supplier_id uuid references suppliers (id) on delete set null,
    received_at date not null,
    expires_at date,
    qty_received numeric(18,3) not null check (qty_received > 0),
    qty_remaining numeric(18,3) not null
        check (qty_remaining >= 0 and qty_remaining <= qty_received),
    unit_cost numeric(18,4) not null check (unit_cost >= 0),
    note text,
    created_at timestamptz not null default now()
);

-- FEFO reads: earliest expiry first, then the earliest receipt. Null expiry
-- sorts last in ascending order, which is exactly the rule.
create index material_batches_fefo_idx
    on material_batches (tenant_id, material_id, expires_at, received_at)
    where qty_remaining > 0;

create table stock_movements (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    batch_id uuid not null references material_batches (id) on delete cascade,
    type text not null check (type in
        ('purchase', 'production_use', 'waste', 'expired', 'adjustment')),
    -- Inbound is positive, outbound is negative.
    qty numeric(18,3) not null check (qty <> 0),
    unit_cost numeric(18,4) check (unit_cost >= 0),
    note text,
    created_by uuid references users (id) on delete set null,
    created_at timestamptz not null default now(),
    constraint stock_movements_sign check (
        (type = 'purchase' and qty > 0)
        or (type in ('production_use', 'waste', 'expired') and qty < 0)
        or (type = 'adjustment')
    )
);

create index stock_movements_batch_idx on stock_movements (tenant_id, batch_id, created_at);
create index stock_movements_recent_idx on stock_movements (tenant_id, created_at desc);

alter table material_batches enable row level security;
alter table material_batches force row level security;
create policy material_batches_tenant on material_batches
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

alter table stock_movements enable row level security;
alter table stock_movements force row level security;
create policy stock_movements_tenant on stock_movements
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());
