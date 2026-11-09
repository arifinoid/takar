//! One-shot daily alert job.
//!
//! Run it from cron or a scheduler: `cargo run -p takar-api --bin alert_job`.
//! It is safe to re-run: alerts are deduplicated by open-alert unique indexes,
//! tenants are claimed with `skip locked`, and delivery state is recorded, so a
//! completed run sends nothing the second time. Do not overlap two runs while
//! sending: delivery is not claimed, and both would compose the same digest.

use anyhow::Context;
use chrono::Utc;
use takar_api::{alerts::send::send_pending, config::Config, notify};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env();

    let database_url = config
        .database_url
        .as_deref()
        .context("DATABASE_URL is required")?;
    let pool = takar_db::connect(database_url)
        .await
        .context("connect to Postgres")?;

    let report = takar_db::alerts::run(&pool)
        .await
        .context("run the alert job")?;

    println!(
        "alert job: tenants={} created={} resolved={}",
        report.tenants, report.created, report.resolved
    );

    let router = notify::router(&config);
    let delivery = send_pending(&pool, &router, &config.app_url, Utc::now().date_naive())
        .await
        .context("send pending alerts")?;

    println!(
        "alert job: sent={} failed={} skipped={}",
        delivery.sent, delivery.failed, delivery.skipped
    );

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
