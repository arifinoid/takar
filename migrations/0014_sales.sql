-- Sales: what left the kitchen and through which channel.
--
-- This is the only place that decrements `products.stock_qty`. Production adds
-- finished goods; a sale removes them. Both run in one transaction, so stock
-- never moves halfway through a document.
--
-- `sale_items.price` is the price at the moment of sale, not the product's
-- current `sell_price`. A later price change must not rewrite yesterday's
-- revenue, the same way a later purchase never rewrites a past HPP.
--
-- `client_ref` is the idempotency key for imports and retries: a bulk entry
-- replayed after a timeout creates one sale, not two, and therefore decrements
-- stock once.

create table sales (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    sold_at timestamptz not null default now(),
    -- toko, reseller, online, lainnya. Validated by the API so adding a
    -- channel is a code change, not a migration.
    channel text not null default 'toko',
    total numeric(18,2) not null check (total >= 0),
    note text,
    -- Null for a manual entry typed straight into the app.
    client_ref uuid,
    created_by uuid references users (id) on delete set null,
    created_at timestamptz not null default now()
);

-- Nulls stay distinct, so any number of manual entries can omit the reference.
create unique index sales_client_ref_unique
    on sales (tenant_id, client_ref)
    where client_ref is not null;

create index sales_recent_idx on sales (tenant_id, sold_at desc, created_at desc);

alter table sales enable row level security;
alter table sales force row level security;
create policy sales_tenant on sales
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

create table sale_items (
    sale_id uuid not null references sales (id) on delete cascade,
    -- Denormalized on purpose: the composite key below proves the product
    -- belongs to the same tenant, which an EXISTS policy alone cannot do.
    tenant_id uuid not null references tenants (id) on delete cascade,
    product_id uuid not null,
    qty numeric(18,3) not null check (qty > 0),
    -- Snapshot of the price charged, never re-read from products.
    price numeric(18,2) not null check (price >= 0),
    primary key (sale_id, product_id),
    constraint sale_items_product_tenant
        foreign key (product_id, tenant_id)
        references products (id, tenant_id)
        on delete restrict
);

create index sale_items_product_idx on sale_items (tenant_id, product_id);

alter table sale_items enable row level security;
alter table sale_items force row level security;
create policy sale_items_tenant on sale_items
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());
