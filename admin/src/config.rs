use anyhow::Result;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    // Phase B で認証実装時に使用
    #[allow(dead_code)]
    pub auth0: Auth0Config,
    #[allow(dead_code)]
    pub access: AccessConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    // Phase B でセッション実装時に使用
    #[allow(dead_code)]
    pub session: SessionConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub port: u16,
    // Phase B で Auth0 コールバック URL 生成時に使用
    #[allow(dead_code)]
    pub base_url: String,
}

// Phase B で OIDC フロー実装時に使用
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Auth0Config {
    pub domain: String,
    pub client_id: String,
    pub client_secret: String,
}

// Phase B で email ホワイトリスト検証時に使用
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct AccessConfig {
    pub allowed_emails: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
}

// Phase B でセッション実装時に使用
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SessionConfig {
    pub secret_key: String,
    pub ttl_seconds: u64,
}

pub fn load(path: &str) -> Result<Config> {
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("設定ファイル '{}' が読み込めません: {}", path, e))?;
    let mut config: Config = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("設定ファイルのパースに失敗しました: {}", e))?;

    // 機密情報は環境変数で上書き可能（docker-compose 経由で .env から注入）
    if let Ok(url) = std::env::var("DATABASE_URL") {
        config.database.url = url;
    }
    if let Ok(url) = std::env::var("REDIS_URL") {
        config.redis.url = url;
    }

    Ok(config)
}
