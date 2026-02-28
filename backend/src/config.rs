use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub twitch_client_id: String,
    pub twitch_client_secret: String,
    pub base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
            twitch_client_id: env::var("TWITCH_CLIENT_ID")
                .unwrap_or_default(),
            twitch_client_secret: env::var("TWITCH_CLIENT_SECRET")
                .unwrap_or_default(),
            base_url: env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),
        }
    }
}
