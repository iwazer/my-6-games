use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();

    let redis_ok = {
        let mut conn = state.redis.clone();
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
