use std::net::SocketAddr;

use takar_api::{config::Config, router, AppState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env();

    let db = match config.database_url.as_deref() {
        Some(url) => match takar_db::connect(url).await {
            Ok(pool) => {
                tracing::info!("database connection established");
                Some(pool)
            }
            Err(error) => {
                tracing::warn!(%error, "database unavailable, starting without it");
                None
            }
        },
        None => {
            tracing::info!("DATABASE_URL is not set, starting without a database");
            None
        }
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(%addr, "takar-api listening");
    axum::serve(listener, router(AppState { db }))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
}
