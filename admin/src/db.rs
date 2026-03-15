use anyhow::Result;
use sqlx::mysql::MySqlPool;

pub async fn init_pool(database_url: &str) -> Result<MySqlPool> {
    let pool = MySqlPool::connect(database_url)
        .await
        .map_err(|e| anyhow::anyhow!("DB接続に失敗しました: {}", e))?;
    Ok(pool)
}
