use sqlx::{PgPool, Row};
use uuid::Uuid;

const APP_ROLE: &str = "takar_app";

async fn install_app_role(pool: &PgPool) {
    sqlx::raw_sql(&format!(
        r#"
        do $$
        begin
            begin
                create role {APP_ROLE} nologin;
            exception when duplicate_object then null;
            end;
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
