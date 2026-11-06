//! Process configuration, read once at startup.

pub struct Config {
    pub port: u16,
    pub database_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3001);

        let database_url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|value| !value.is_empty());

        Self { port, database_url }
    }
}
