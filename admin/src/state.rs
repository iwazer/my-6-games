use openidconnect::core::CoreClient;
use redis::aio::ConnectionManager;
use sqlx::mysql::MySqlPool;
use std::sync::Arc;
use tera::Tera;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: MySqlPool,
    pub redis: ConnectionManager,
    // Phase C でテンプレートレンダリング時に使用（認証フローでも使用）
    pub tera: Arc<Tera>,
    pub oidc_client: Arc<CoreClient>,
}
