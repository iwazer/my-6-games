use redis::aio::ConnectionManager;
use rocket::serde::json::{json, Json};
use rocket::State;
use serde_json::Value;
use sqlx::MySqlPool;

#[get("/health")]
pub async fn health(
    db: &State<MySqlPool>,
    redis: &State<ConnectionManager>,
) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1")
        .execute(db.inner())
        .await
        .is_ok();

    let redis_ok = {
        let mut conn = redis.inner().clone();
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .is_ok()
    };

    Json(json!({
        "status": if db_ok && redis_ok { "ok" } else { "degraded" },
        "db": if db_ok { "ok" } else { "error" },
        "cache": if redis_ok { "ok" } else { "error" },
    }))
}
