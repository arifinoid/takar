//! Postgres access layer for Takar.

use sqlx::postgres::{PgPool, PgPoolOptions};

/// Opens a connection pool. Callers own the pool lifetime.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}
