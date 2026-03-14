mod cache;
mod config;
mod db;
mod error;
mod routes;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get, Router};
use tera::Tera;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "admin=debug,tower_http=debug".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = config::load(&config_path)?;

    tracing::info!("設定ファイルを読み込みました: {}", config_path);

    let db = db::init_pool(&config.database.url).await?;
    tracing::info!("DB接続完了");

    let redis = cache::init_manager(&config.redis.url).await?;
    tracing::info!("Redis接続完了");

    let tera = Tera::new("templates/**/*.tera")
        .map_err(|e| anyhow::anyhow!("テンプレートの初期化に失敗しました: {}", e))?;

    let state = AppState {
        config: config.clone(),
        db,
        redis,
        tera: Arc::new(tera),
    };

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    tracing::info!("管理画面を起動します: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
