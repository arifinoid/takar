use std::net::SocketAddr;
use std::sync::Arc;

use takar_api::{
    auth::{otp::LogOtpSender, JwtConfig},
    config::{Config, DEV_JWT_SECRET},
    router, AppState,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env();

    if config.jwt_secret == DEV_JWT_SECRET {
        tracing::warn!("JWT_SECRET is not set, using the development secret");
    }

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

    let state = AppState {
        db,
        jwt: JwtConfig {
            secret: config.jwt_secret,
            ttl_seconds: config.jwt_ttl_seconds,
        },
        otp: Arc::new(LogOtpSender),
        otp_ttl_seconds: config.otp_ttl_seconds,
        jwt_ttl_seconds: config.jwt_ttl_seconds,
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(%addr, "takar-api listening");
    axum::serve(listener, router(state))
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
