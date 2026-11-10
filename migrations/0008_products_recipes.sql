-- Finished products and their versioned recipes.
--
-- A recipe is immutable once written. Editing means inserting a new version
-- and deactivating the previous one, so a production order can point at the
-- exact version it consumed and historical HPP never changes.
--
-- `products.current_recipe_id` is deliberately not here: P2-02 adds it along
-- with the product-facing rules. This migration only needs a parent row.

create table products (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    name text not null check (length(btrim(name)) between 1 and 120),
    sku text,
    sell_price numeric(18,2) not null default 0 check (sell_price >= 0),
    -- Finished-goods stock. Batch tracking for products is deferred (D8).
    stock_qty numeric(18,3) not null default 0 check (stock_qty >= 0),
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    -- Lets recipes prove the product belongs to the same tenant.
    constraint products_id_tenant_unique unique (id, tenant_id)
);

create unique index products_name_unique on products (tenant_id, lower(name));
create unique index products_sku_unique on products (tenant_id, lower(sku))
    where sku is not null;
create index products_active_idx on products (tenant_id) where is_active;

alter table products enable row level security;
alter table products force row level security;
create policy products_tenant on products
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

create table recipes (
    id uuid primary key default gen_random_uuid(),
    tenant_id uuid not null references tenants (id) on delete cascade,
    product_id uuid not null,
    version int not null check (version > 0),
    -- Output of one batch of this recipe.
    yield_qty numeric(18,3) not null check (yield_qty > 0),
    labor_minutes int not null default 0 check (labor_minutes >= 0),
    is_active boolean not null default true,
    created_by uuid references users (id) on delete set null,
    created_at timestamptz not null default now(),
    constraint recipes_version_unique unique (product_id, version),
    constraint recipes_product_tenant
        foreign key (product_id, tenant_id)
        references products (id, tenant_id)
        on delete cascade
);

-- At most one active version per product, enforced by the database rather than
-- by whichever request happens to run last.
create unique index recipes_one_active on recipes (product_id) where is_active;
create index recipes_product_idx on recipes (tenant_id, product_id, version desc);

alter table recipes enable row level security;
alter table recipes force row level security;
create policy recipes_tenant on recipes
    for all using (tenant_id = app_tenant_id())
    with check (tenant_id = app_tenant_id());

create table recipe_items (
    recipe_id uuid not null references recipes (id) on delete cascade,
    material_id uuid not null references materials (id) on delete restrict,
    -- Base unit of the material, same as batches.
    qty numeric(18,4) not null check (qty > 0),
    waste_pct numeric(5,2) not null default 0 check (waste_pct >= 0 and waste_pct <= 100),
    primary key (recipe_id, material_id)
);

create index recipe_items_material_idx on recipe_items (material_id);

-- Child table without tenant_id: isolate through the parent recipe. The
-- subquery is itself filtered by the recipes policy, so a row is visible only
-- when its recipe belongs to the active tenant.
alter table recipe_items enable row level security;
alter table recipe_items force row level security;
create policy recipe_items_tenant on recipe_items
    for all
    using (exists (
        select 1 from recipes r
        where r.id = recipe_id and r.tenant_id = app_tenant_id()
    ))
    with check (exists (
        select 1 from recipes r
        where r.id = recipe_id and r.tenant_id = app_tenant_id()
    ));
