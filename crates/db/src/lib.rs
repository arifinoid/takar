//! Postgres access layer for Takar.

pub mod alerts;

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Opens a connection pool. Callers own the pool lifetime.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

/// Applies every migration embedded at compile time.
///
/// Production deploys should run `sqlx migrate run`; this helper exists for
/// tests and local tooling.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

/// Starts a transaction with the RLS session settings applied.
///
/// Every query that touches a tenant-scoped table must run inside one of these.
pub async fn begin_scoped(
    pool: &PgPool,
    tenant_id: Option<Uuid>,
    user_id: Option<Uuid>,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    if let Some(tenant_id) = tenant_id {
        sqlx::query("select set_config('app.tenant_id', $1, true)")
            .bind(tenant_id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    if let Some(user_id) = user_id {
        sqlx::query("select set_config('app.user_id', $1, true)")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await?;
    }

    Ok(tx)
}
