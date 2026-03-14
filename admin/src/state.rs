use redis::aio::ConnectionManager;
use sqlx::mysql::MySqlPool;
use std::sync::Arc;
use tera::Tera;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    // Phase B 以降で使用
    #[allow(dead_code)]
    pub config: Config,
    pub db: MySqlPool,
    pub redis: ConnectionManager,
    // Phase C でテンプレートレンダリング時に使用
    #[allow(dead_code)]
    pub tera: Arc<Tera>,
}
