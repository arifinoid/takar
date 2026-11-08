//! One-shot daily alert job.
//!
//! Run it from cron or a scheduler: `cargo run -p takar-api --bin alert_job`.
//! It is safe to run twice or in parallel; alerts are deduplicated by open-alert
//! unique indexes and tenants are claimed with `skip locked`.

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let pool = takar_db::connect(&database_url)
        .await
        .context("connect to Postgres")?;

    let report = takar_db::alerts::run(&pool)
        .await
        .context("run the alert job")?;

    println!(
        "alert job: tenants={} created={} resolved={}",
        report.tenants, report.created, report.resolved
    );

    Ok(())
}
