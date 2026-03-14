use anyhow::Result;
use redis::{aio::ConnectionManager, Client};

pub async fn init_manager(redis_url: &str) -> Result<ConnectionManager> {
    let client =
        Client::open(redis_url).map_err(|e| anyhow::anyhow!("Redis URLが無効です: {}", e))?;
    let manager = ConnectionManager::new(client)
        .await
        .map_err(|e| anyhow::anyhow!("Redis接続に失敗しました: {}", e))?;
    Ok(manager)
}
