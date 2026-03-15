use anyhow::Result;
use redis::aio::ConnectionManager;
use uuid::Uuid;

const SESSION_PREFIX: &str = "admin:session:";
const OAUTH_STATE_PREFIX: &str = "admin:oauth:";
const OAUTH_STATE_TTL: u64 = 600; // 10分

/// セッションを作成して session_id を返す
pub async fn create_session(
    redis: &mut ConnectionManager,
    email: &str,
    ttl_seconds: u64,
) -> Result<String> {
    let session_id = Uuid::new_v4().to_string();
    let key = format!("{}{}", SESSION_PREFIX, session_id);
    redis::cmd("SET")
        .arg(&key)
        .arg(email)
        .arg("EX")
        .arg(ttl_seconds)
        .query_async::<()>(redis)
        .await?;
    Ok(session_id)
}

/// session_id に対応する email を返す（存在しない or 期限切れなら None）
pub async fn get_session_email(
    redis: &mut ConnectionManager,
    session_id: &str,
) -> Result<Option<String>> {
    let key = format!("{}{}", SESSION_PREFIX, session_id);
    let email: Option<String> = redis::cmd("GET").arg(&key).query_async(redis).await?;
    Ok(email)
}

/// セッションを削除する
pub async fn delete_session(redis: &mut ConnectionManager, session_id: &str) -> Result<()> {
    let key = format!("{}{}", SESSION_PREFIX, session_id);
    redis::cmd("DEL").arg(&key).query_async::<()>(redis).await?;
    Ok(())
}

/// CSRF state → nonce を Redis に保存（10分 TTL）
pub async fn store_oauth_state(
    redis: &mut ConnectionManager,
    state: &str,
    nonce: &str,
) -> Result<()> {
    let key = format!("{}{}", OAUTH_STATE_PREFIX, state);
    redis::cmd("SET")
        .arg(&key)
        .arg(nonce)
        .arg("EX")
        .arg(OAUTH_STATE_TTL)
        .query_async::<()>(redis)
        .await?;
    Ok(())
}

/// CSRF state に対応する nonce を取得して削除する（使い捨て）
pub async fn pop_oauth_nonce(redis: &mut ConnectionManager, state: &str) -> Result<Option<String>> {
    let key = format!("{}{}", OAUTH_STATE_PREFIX, state);
    let nonce: Option<String> = redis::cmd("GETDEL").arg(&key).query_async(redis).await?;
    Ok(nonce)
}
