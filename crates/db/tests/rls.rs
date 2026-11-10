use sqlx::{PgPool, Row};
use uuid::Uuid;

const APP_ROLE: &str = "takar_app";

async fn install_app_role(pool: &PgPool) {
    // Roles are cluster-wide, so every test database races to create the same
    // one. Advisory locks are per database and cannot serialize that, so the
    // create tolerates both codes a losing racer can see: duplicate_object in
    // the normal path and a unique violation on pg_authid under concurrency.
    sqlx::raw_sql(&format!(
        r#"
        do $$
        begin
            if not exists (select 1 from pg_roles where rolname = '{APP_ROLE}') then
                begin
                    create role {APP_ROLE} nologin;
                exception
                    when duplicate_object or unique_violation then null;
                end;
            end if;
        end
        $$;
        grant usage on schema public to {APP_ROLE};
        grant select, insert, update, delete on all tables in schema public to {APP_ROLE};
        "#
    ))
    .execute(pool)
    .await
    .expect("install app role");
}

struct TenantFixture {
    tenant_id: Uuid,
    owner_id: Uuid,
}

async fn seed_tenant(pool: &PgPool, slug: &str) -> TenantFixture {
    let tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();

    sqlx::query("insert into tenants (id, name) values ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Tenant {slug}"))
        .execute(pool)
        .await
        .expect("insert tenant");

    sqlx::query("insert into users (id, email, name) values ($1, $2, $3)")
        .bind(owner_id)
        .bind(format!("owner@{slug}.test"))
        .bind(format!("Owner {slug}"))
        .execute(pool)
        .await
        .expect("insert owner");

    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'owner')")
        .bind(tenant_id)
        .bind(owner_id)
        .execute(pool)
        .await
        .expect("insert membership");

    TenantFixture {
        tenant_id,
        owner_id,
    }
}

async fn add_member(pool: &PgPool, tenant_id: Uuid, slug: &str, role: &str) -> Uuid {
    let user_id = Uuid::new_v4();

    sqlx::query("insert into users (id, email, name) values ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("{slug}@example.test"))
        .bind(format!("Member {slug}"))
        .execute(pool)
        .await
        .expect("insert member");

    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("insert member membership");

    user_id
}

async fn begin_as(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut tx = pool.begin().await.expect("begin transaction");

    sqlx::query(&format!("set local role {APP_ROLE}"))
        .execute(&mut *tx)
        .await
        .expect("set role");

    sqlx::query("select set_config('app.tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("set tenant");

    sqlx::query("select set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("set user");

    tx
}

async fn begin_as_user(pool: &PgPool, user_id: Uuid) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut tx = pool.begin().await.expect("begin transaction");

    sqlx::query(&format!("set local role {APP_ROLE}"))
        .execute(&mut *tx)
        .await
        .expect("set role");

    sqlx::query("select set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("set user");

    tx
}

#[sqlx::test(migrations = "../../migrations")]
async fn user_can_list_own_memberships_without_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "theta").await;
    let tenant_b = seed_tenant(&pool, "iota").await;
    let shared = Uuid::new_v4();

    sqlx::query("insert into users (id, email, name) values ($1, 'shared@example.test', 'Shared')")
        .bind(shared)
        .execute(&pool)
        .await
        .expect("insert shared user");

    for tenant_id in [tenant_a.tenant_id, tenant_b.tenant_id] {
        sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'viewer')")
            .bind(tenant_id)
            .bind(shared)
            .execute(&pool)
            .await
            .expect("insert shared membership");
    }

    let mut tx = begin_as_user(&pool, shared).await;

    let tenant_ids: Vec<Uuid> = sqlx::query("select tenant_id from memberships")
        .fetch_all(&mut *tx)
        .await
        .expect("read own memberships")
        .iter()
        .map(|row| row.get("tenant_id"))
        .collect();

    assert_eq!(tenant_ids.len(), 2, "both memberships should be visible");
    assert!(tenant_ids.contains(&tenant_a.tenant_id));
    assert!(tenant_ids.contains(&tenant_b.tenant_id));
}

#[sqlx::test(migrations = "../../migrations")]
async fn memberships_are_scoped_to_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "alpha").await;
    let tenant_b = seed_tenant(&pool, "beta").await;

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let rows = sqlx::query("select tenant_id from memberships")
        .fetch_all(&mut *tx)
        .await
        .expect("read memberships");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<Uuid, _>("tenant_id"),
        tenant_a.tenant_id,
        "only the active tenant should be visible"
    );

    let tenant_ids: Vec<Uuid> = rows.iter().map(|row| row.get("tenant_id")).collect();
    assert!(!tenant_ids.contains(&tenant_b.tenant_id));
}

#[sqlx::test(migrations = "../../migrations")]
async fn writes_to_another_tenant_are_rejected(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "gamma").await;
    let tenant_b = seed_tenant(&pool, "delta").await;
    let outsider = add_member(&pool, tenant_a.tenant_id, "outsider", "viewer").await;

    let mut tx = begin_as(&pool, tenant_b.tenant_id, tenant_b.owner_id).await;

    let result =
        sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'viewer')")
            .bind(tenant_a.tenant_id)
            .bind(outsider)
            .execute(&mut *tx)
            .await;

    assert!(result.is_err(), "RLS must reject a row for another tenant");
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_are_visible_only_inside_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "epsilon").await;
    let tenant_b = seed_tenant(&pool, "zeta").await;
    let teammate = add_member(&pool, tenant_a.tenant_id, "teammate", "production").await;

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let ids: Vec<Uuid> = sqlx::query("select id from users")
        .fetch_all(&mut *tx)
        .await
        .expect("read users")
        .iter()
        .map(|row| row.get("id"))
        .collect();

    assert!(ids.contains(&tenant_a.owner_id));
    assert!(ids.contains(&teammate));
    assert!(!ids.contains(&tenant_b.owner_id));
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_plan_is_rejected(pool: PgPool) {
    let result = sqlx::query("insert into tenants (name, plan) values ('Bad plan', 'enterprise')")
        .execute(&pool)
        .await;

    assert!(result.is_err(), "tenants.plan must stay inside the enum");
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_pending_invitation_is_rejected(pool: PgPool) {
    let tenant = seed_tenant(&pool, "eta").await;

    let insert = |token: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "insert into invitations (tenant_id, email, role, token_hash, invited_by, expires_at)
                 values ($1, 'invitee@example.test', 'viewer', $2, $3, now() + interval '7 days')",
            )
            .bind(tenant.tenant_id)
            .bind(token)
            .bind(tenant.owner_id)
            .execute(&pool)
            .await
        }
    };

    insert("hash-one").await.expect("first invitation");
    let second = insert("hash-two").await;

    assert!(second.is_err(), "only one pending invitation per contact");
}

#[sqlx::test(migrations = "../../migrations")]
async fn materials_are_scoped_to_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "alpha").await;
    let tenant_b = seed_tenant(&pool, "beta").await;

    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(&pool)
        .await
        .expect("kg unit");

    for (tenant_id, name) in [
        (tenant_a.tenant_id, "Tepung"),
        (tenant_b.tenant_id, "Cabai"),
    ] {
        sqlx::query("insert into materials (tenant_id, name, base_unit_id) values ($1, $2, $3)")
            .bind(tenant_id)
            .bind(name)
            .bind(unit_id)
            .execute(&pool)
            .await
            .expect("insert material");
    }

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let rows = sqlx::query("select name from materials")
        .fetch_all(&mut *tx)
        .await
        .expect("read materials");

    assert_eq!(rows.len(), 1, "only the active tenant should be visible");
    assert_eq!(rows[0].get::<String, _>("name"), "Tepung");

    let denied = sqlx::query(
        "insert into materials (tenant_id, name, base_unit_id) values ($1, 'Bawang', $2)",
    )
    .bind(tenant_b.tenant_id)
    .bind(unit_id)
    .execute(&mut *tx)
    .await;

    assert!(denied.is_err(), "writing into another tenant must fail");
}

#[sqlx::test(migrations = "../../migrations")]
async fn batches_are_scoped_to_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "alpha").await;
    let tenant_b = seed_tenant(&pool, "beta").await;

    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(&pool)
        .await
        .expect("kg unit");

    let mut materials = Vec::new();
    let mut batches = Vec::new();

    for (tenant_id, name) in [
        (tenant_a.tenant_id, "Tepung"),
        (tenant_b.tenant_id, "Cabai"),
    ] {
        let (material_id,): (Uuid,) = sqlx::query_as(
            "insert into materials (tenant_id, name, base_unit_id) values ($1, $2, $3) returning id",
        )
        .bind(tenant_id)
        .bind(name)
        .bind(unit_id)
        .fetch_one(&pool)
        .await
        .expect("insert material");

        let (batch_id,): (Uuid,) = sqlx::query_as(
            "insert into material_batches
                 (tenant_id, material_id, received_at, qty_received, qty_remaining, unit_cost)
             values ($1, $2, current_date, 10, 10, 1000)
             returning id",
        )
        .bind(tenant_id)
        .bind(material_id)
        .fetch_one(&pool)
        .await
        .expect("insert batch");

        materials.push(material_id);
        batches.push(batch_id);
    }

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let rows = sqlx::query("select id from material_batches")
        .fetch_all(&mut *tx)
        .await
        .expect("read batches");

    assert_eq!(rows.len(), 1, "only the active tenant should be visible");
    assert_eq!(rows[0].get::<Uuid, _>("id"), batches[0]);

    let denied = sqlx::query(
        "insert into material_batches
             (tenant_id, material_id, received_at, qty_received, qty_remaining, unit_cost)
         values ($1, $2, current_date, 1, 1, 1)",
    )
    .bind(tenant_b.tenant_id)
    .bind(materials[1])
    .execute(&mut *tx)
    .await;

    assert!(denied.is_err(), "writing into another tenant must fail");
}

#[sqlx::test(migrations = "../../migrations")]
async fn products_and_recipe_items_are_scoped_to_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "kappa").await;
    let tenant_b = seed_tenant(&pool, "lambda").await;

    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(&pool)
        .await
        .expect("unit");

    let mut fixtures = Vec::new();

    for (tenant_id, name) in [
        (tenant_a.tenant_id, "Kue lapis"),
        (tenant_b.tenant_id, "Bolu"),
    ] {
        let material_id = Uuid::new_v4();
        sqlx::query(
            "insert into materials (id, tenant_id, name, base_unit_id)
             values ($1, $2, $3, $4)",
        )
        .bind(material_id)
        .bind(tenant_id)
        .bind(format!("{name} flour"))
        .bind(unit_id)
        .execute(&pool)
        .await
        .expect("material");

        let product_id = Uuid::new_v4();
        sqlx::query("insert into products (id, tenant_id, name) values ($1, $2, $3)")
            .bind(product_id)
            .bind(tenant_id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("product");

        let recipe_id = Uuid::new_v4();
        sqlx::query(
            "insert into recipes (id, tenant_id, product_id, version, yield_qty)
             values ($1, $2, $3, 1, 10)",
        )
        .bind(recipe_id)
        .bind(tenant_id)
        .bind(product_id)
        .execute(&pool)
        .await
        .expect("recipe");

        sqlx::query("insert into recipe_items (recipe_id, material_id, qty) values ($1, $2, 1)")
            .bind(recipe_id)
            .bind(material_id)
            .execute(&pool)
            .await
            .expect("recipe item");

        fixtures.push((product_id, recipe_id));
    }

    let (product_a, recipe_a) = fixtures[0];
    let (_, recipe_b) = fixtures[1];

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let products: Vec<Uuid> = sqlx::query("select id from products")
        .fetch_all(&mut *tx)
        .await
        .expect("read products")
        .iter()
        .map(|row| row.get("id"))
        .collect();
    assert_eq!(
        products,
        vec![product_a],
        "only the active tenant's products"
    );

    let recipes: Vec<Uuid> = sqlx::query("select recipe_id from recipe_items")
        .fetch_all(&mut *tx)
        .await
        .expect("read recipe items")
        .iter()
        .map(|row| row.get("recipe_id"))
        .collect();
    assert_eq!(recipes, vec![recipe_a], "recipe items follow their recipe");

    let cross_tenant =
        sqlx::query("insert into recipe_items (recipe_id, material_id, qty) values ($1, $2, 1)")
            .bind(recipe_b)
            .bind(Uuid::new_v4())
            .execute(&mut *tx)
            .await;
    assert!(
        cross_tenant.is_err(),
        "an item cannot be attached to another tenant's recipe"
    );

    let cross_product = sqlx::query("insert into products (tenant_id, name) values ($1, 'Cross')")
        .bind(tenant_b.tenant_id)
        .execute(&mut *tx)
        .await;
    assert!(
        cross_product.is_err(),
        "RLS must reject a cross-tenant product"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_is_scoped_to_the_active_tenant(pool: PgPool) {
    install_app_role(&pool).await;

    let tenant_a = seed_tenant(&pool, "mu").await;
    let tenant_b = seed_tenant(&pool, "nu").await;

    let (unit_id,): (Uuid,) = sqlx::query_as("select id from units where code = 'kg'")
        .fetch_one(&pool)
        .await
        .expect("unit");

    let mut fixtures = Vec::new();

    for (tenant_id, name) in [
        (tenant_a.tenant_id, "Kue lapis"),
        (tenant_b.tenant_id, "Bolu"),
    ] {
        let material_id = Uuid::new_v4();
        sqlx::query(
            "insert into materials (id, tenant_id, name, base_unit_id)
             values ($1, $2, $3, $4)",
        )
        .bind(material_id)
        .bind(tenant_id)
        .bind(format!("{name} flour"))
        .bind(unit_id)
        .execute(&pool)
        .await
        .expect("material");

        let batch_id = Uuid::new_v4();
        sqlx::query(
            "insert into material_batches
                 (id, tenant_id, material_id, received_at, qty_received, qty_remaining, unit_cost)
             values ($1, $2, $3, current_date, 10, 10, 1000)",
        )
        .bind(batch_id)
        .bind(tenant_id)
        .bind(material_id)
        .execute(&pool)
        .await
        .expect("batch");

        let product_id = Uuid::new_v4();
        sqlx::query("insert into products (id, tenant_id, name) values ($1, $2, $3)")
            .bind(product_id)
            .bind(tenant_id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("product");

        let recipe_id = Uuid::new_v4();
        sqlx::query(
            "insert into recipes (id, tenant_id, product_id, version, yield_qty)
             values ($1, $2, $3, 1, 10)",
        )
        .bind(recipe_id)
        .bind(tenant_id)
        .bind(product_id)
        .execute(&pool)
        .await
        .expect("recipe");

        let order_id = Uuid::new_v4();
        sqlx::query(
            "insert into production_orders
                 (id, tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on)
             values ($1, $2, $3, $4, 1, 1, current_date)",
        )
        .bind(order_id)
        .bind(tenant_id)
        .bind(product_id)
        .bind(recipe_id)
        .execute(&pool)
        .await
        .expect("production order");

        sqlx::query(
            "insert into production_consumptions
                 (production_order_id, batch_id, qty, unit_cost)
             values ($1, $2, 1, 1000)",
        )
        .bind(order_id)
        .bind(batch_id)
        .execute(&pool)
        .await
        .expect("consumption");

        fixtures.push((order_id, batch_id));
    }

    let (order_a, batch_a) = fixtures[0];
    let (order_b, _) = fixtures[1];

    let mut tx = begin_as(&pool, tenant_a.tenant_id, tenant_a.owner_id).await;

    let orders: Vec<Uuid> = sqlx::query("select id from production_orders")
        .fetch_all(&mut *tx)
        .await
        .expect("read orders")
        .iter()
        .map(|row| row.get("id"))
        .collect();
    assert_eq!(orders, vec![order_a], "only the active tenant's orders");

    let consumptions: Vec<Uuid> =
        sqlx::query("select production_order_id from production_consumptions")
            .fetch_all(&mut *tx)
            .await
            .expect("read consumptions")
            .iter()
            .map(|row| row.get("production_order_id"))
            .collect();
    assert_eq!(
        consumptions,
        vec![order_a],
        "consumptions follow their order"
    );

    let cross_order = sqlx::query(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on)
         values ($1, (select id from products where tenant_id = $1),
                 (select id from recipes where tenant_id = $1), 1, 1, current_date)",
    )
    .bind(tenant_b.tenant_id)
    .execute(&mut *tx)
    .await;
    assert!(
        cross_order.is_err(),
        "RLS must reject a cross-tenant production order"
    );

    let cross_consumption = sqlx::query(
        "insert into production_consumptions
             (production_order_id, batch_id, qty, unit_cost)
         values ($1, $2, 1, 1000)",
    )
    .bind(order_b)
    .bind(batch_a)
    .execute(&mut *tx)
    .await;
    assert!(
        cross_consumption.is_err(),
        "a consumption cannot be attached to another tenant's order"
    );
}
