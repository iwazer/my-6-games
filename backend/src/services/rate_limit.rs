use redis::aio::ConnectionManager;

pub enum RateLimitResult {
    Allowed,
    Exceeded,
}

/// INCR + EXPIRE パターンによるレート制限チェック
pub async fn check(
    redis: &ConnectionManager,
    key: &str,
    max_requests: i64,
    window_secs: i64,
) -> redis::RedisResult<RateLimitResult> {
    let mut conn = redis.clone();

    let count: i64 = redis::cmd("INCR").arg(key).query_async(&mut conn).await?;

    if count == 1 {
        // 最初のリクエストでウィンドウの TTL をセット
        redis::cmd("EXPIRE")
            .arg(key)
            .arg(window_secs)
            .query_async::<()>(&mut conn)
            .await?;
    }

    if count > max_requests {
        Ok(RateLimitResult::Exceeded)
    } else {
        Ok(RateLimitResult::Allowed)
    }
}
