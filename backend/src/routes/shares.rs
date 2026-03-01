use chrono::{Duration, TimeZone, Utc};
use redis::aio::ConnectionManager;
use rocket::http::Status;
use rocket::serde::json::{json, Json};
use rocket::State;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{MySqlPool, Row};

use crate::models::share::{Share, ShareGame};
use crate::routes::ClientIp;
use crate::services::rate_limit::{self, RateLimitResult};

const INITIAL_DAYS: i64 = 30;
const MAX_DAYS: i64 = 90;
const RATE_MAX: i64 = 10;
const RATE_WINDOW: i64 = 3600; // 1h

/// POST /api/shares リクエストボディの1ゲームエントリ
#[derive(Deserialize)]
pub struct GameInput {
    pub igdb_id: i64,
    pub name: String,
    pub cover_url: Option<String>,
    pub release_year: Option<i32>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub comment: Option<String>,
    #[serde(default)]
    pub is_spoiler: bool,
}

/// POST /api/shares リクエストボディ
#[derive(Deserialize)]
pub struct CreateShareRequest {
    #[serde(default)]
    pub creator: Option<String>,
    pub games: Vec<GameInput>,
}

// --- 純粋関数（テスト可能なロジック） ---

/// 16 桁の小文字 16 進数 ID を生成する
pub(crate) fn generate_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

/// ゲーム数が正確に 6 件かを検証する
pub(crate) fn validate_game_count(count: usize) -> bool {
    count == 6
}

/// 作成者名が 40 文字以内かを検証する（文字数、バイト数ではない）
pub(crate) fn validate_creator_length(creator: &str) -> bool {
    creator.chars().count() <= 40
}

/// コメントが 140 文字以内かを検証する（文字数、バイト数ではない）
pub(crate) fn validate_comment_length(comment: &str) -> bool {
    comment.chars().count() <= 140
}

// --- リクエストハンドラ ---

/// POST /api/shares — 共有ページを作成する
///
/// - ゲームは正確に 6 件必須
/// - creator: max 40 文字（任意）
/// - comment: max 140 文字/件（任意）
/// - レート制限: 10 リクエスト/時/IP
#[post("/shares", data = "<body>")]
pub async fn create_share(
    body: Json<CreateShareRequest>,
    db: &State<MySqlPool>,
    redis: &State<ConnectionManager>,
    client_ip: ClientIp,
) -> (Status, Json<Value>) {
    // レート制限（10 req/h per IP）
    let rate_key = format!("ratelimit:create:{}", client_ip.0);
    if let Ok(RateLimitResult::Exceeded) =
        rate_limit::check(redis.inner(), &rate_key, RATE_MAX, RATE_WINDOW).await
    {
        return (
            Status::TooManyRequests,
            Json(json!({ "error": "rate limit exceeded" })),
        );
    }

    // バリデーション
    if !validate_game_count(body.games.len()) {
        return (
            Status::UnprocessableEntity,
            Json(json!({ "error": "exactly 6 games are required" })),
        );
    }
    if let Some(ref c) = body.creator {
        if !validate_creator_length(c) {
            return (
                Status::UnprocessableEntity,
                Json(json!({ "error": "creator must be 40 characters or fewer" })),
            );
        }
    }
    for g in &body.games {
        if let Some(ref comment) = g.comment {
            if !validate_comment_length(comment) {
                return (
                    Status::UnprocessableEntity,
                    Json(json!({ "error": "comment must be 140 characters or fewer" })),
                );
            }
        }
    }

    // ゲームデータを ShareGame に変換
    let games: Vec<ShareGame> = body
        .games
        .iter()
        .map(|g| ShareGame {
            igdb_id: g.igdb_id,
            name: g.name.clone(),
            cover_url: g.cover_url.clone(),
            release_year: g.release_year,
            platforms: g.platforms.clone(),
            comment: g.comment.clone(),
            is_spoiler: g.is_spoiler,
        })
        .collect();

    let games_json = match serde_json::to_string(&games) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("JSON serialize error: {e}");
            return (
                Status::InternalServerError,
                Json(json!({ "error": "internal error" })),
            );
        }
    };

    let id = generate_id();
    let now = Utc::now();
    let expires_at = now + Duration::days(INITIAL_DAYS);

    // DB に挿入
    if let Err(e) = sqlx::query(
        "INSERT INTO shares (id, creator, games_json, created_at, accessed_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&body.creator)
    .bind(&games_json)
    .bind(now.naive_utc())
    .bind(now.naive_utc())
    .bind(expires_at.naive_utc())
    .execute(db.inner())
    .await
    {
        eprintln!("DB insert error: {e}");
        return (
            Status::InternalServerError,
            Json(json!({ "error": "internal error" })),
        );
    }

    // Redis キャッシュ（expires_at まで）
    let share = Share {
        id: id.clone(),
        creator: body.creator.clone(),
        games,
        created_at: now,
        expires_at,
    };
    cache_share(&share, redis.inner()).await;

    (
        Status::Created,
        Json(json!({ "id": id, "url": format!("/s/{id}") })),
    )
}

/// GET /api/shares/<id> — 共有データを JSON で返す
///
/// - Redis キャッシュヒット時はそのまま返す
/// - DB ヒット時: accessed_at を更新し expires_at を最大 90 日まで延長する
#[get("/shares/<id>")]
pub async fn get_share(
    id: &str,
    db: &State<MySqlPool>,
    redis: &State<ConnectionManager>,
) -> (Status, Json<Value>) {
    // Redis キャッシュ確認
    {
        let mut conn = redis.inner().clone();
        if let Ok(cached) = redis::cmd("GET")
            .arg(format!("share:{id}"))
            .query_async::<String>(&mut conn)
            .await
        {
            if let Ok(share) = serde_json::from_str::<Share>(&cached) {
                return (Status::Ok, Json(json!(share)));
            }
        }
    }

    // DB から取得（期限切れは除外）
    let row = sqlx::query(
        "SELECT id, creator, games_json, created_at, expires_at \
         FROM shares WHERE id = ? AND expires_at > NOW()",
    )
    .bind(id)
    .fetch_optional(db.inner())
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return (Status::NotFound, Json(json!({ "error": "not found" }))),
        Err(e) => {
            eprintln!("DB select error: {e}");
            return (
                Status::InternalServerError,
                Json(json!({ "error": "internal error" })),
            );
        }
    };

    let share_id: String = row.try_get("id").unwrap_or_default();
    let creator: Option<String> = row.try_get("creator").ok().flatten();
    let games_json: String = row.try_get("games_json").unwrap_or_default();
    let created_at_naive: chrono::NaiveDateTime = match row.try_get("created_at") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("DB column error: {e}");
            return (
                Status::InternalServerError,
                Json(json!({ "error": "internal error" })),
            );
        }
    };
    let expires_at_naive: chrono::NaiveDateTime = match row.try_get("expires_at") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("DB column error: {e}");
            return (
                Status::InternalServerError,
                Json(json!({ "error": "internal error" })),
            );
        }
    };

    let games: Vec<ShareGame> = match serde_json::from_str(&games_json) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("JSON deserialize error: {e}");
            return (
                Status::InternalServerError,
                Json(json!({ "error": "internal error" })),
            );
        }
    };

    // アクセス日時・有効期限を更新（30日延長、最大 90 日上限）
    let _ = sqlx::query(
        "UPDATE shares \
         SET accessed_at = NOW(), \
             expires_at = LEAST( \
                 DATE_ADD(NOW(), INTERVAL ? DAY), \
                 DATE_ADD(created_at, INTERVAL ? DAY) \
             ) \
         WHERE id = ?",
    )
    .bind(INITIAL_DAYS)
    .bind(MAX_DAYS)
    .bind(&share_id)
    .execute(db.inner())
    .await;

    // 更新後の expires_at を再取得（失敗時は元の値を使用）
    let new_expires_at_naive = sqlx::query("SELECT expires_at FROM shares WHERE id = ?")
        .bind(&share_id)
        .fetch_one(db.inner())
        .await
        .ok()
        .and_then(|r| r.try_get::<chrono::NaiveDateTime, _>("expires_at").ok())
        .unwrap_or(expires_at_naive);

    let created_at = Utc.from_utc_datetime(&created_at_naive);
    let expires_at = Utc.from_utc_datetime(&new_expires_at_naive);

    let share = Share {
        id: share_id,
        creator,
        games,
        created_at,
        expires_at,
    };

    cache_share(&share, redis.inner()).await;

    (Status::Ok, Json(json!(share)))
}

/// Redis に share データをキャッシュする（失敗しても無視）
async fn cache_share(share: &Share, redis: &ConnectionManager) {
    let Ok(json_str) = serde_json::to_string(share) else {
        return;
    };
    let ttl = (share.expires_at - Utc::now()).num_seconds().max(0) as usize;
    if ttl == 0 {
        return;
    }
    let mut conn = redis.clone();
    let _: Result<(), _> = redis::cmd("SETEX")
        .arg(format!("share:{}", share.id))
        .arg(ttl)
        .arg(json_str)
        .query_async::<()>(&mut conn)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- generate_id ---

    #[test]
    fn id_is_16_chars() {
        assert_eq!(generate_id().len(), 16);
    }

    #[test]
    fn id_is_lowercase_hex() {
        let id = generate_id();
        assert!(id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(generate_id(), generate_id());
    }

    // --- validate_game_count ---

    #[test]
    fn game_count_6_is_valid() {
        assert!(validate_game_count(6));
    }

    #[test]
    fn game_count_5_is_invalid() {
        assert!(!validate_game_count(5));
    }

    #[test]
    fn game_count_7_is_invalid() {
        assert!(!validate_game_count(7));
    }

    // --- validate_creator_length ---

    #[test]
    fn creator_40_ascii_chars_is_valid() {
        assert!(validate_creator_length(&"a".repeat(40)));
    }

    #[test]
    fn creator_41_ascii_chars_is_invalid() {
        assert!(!validate_creator_length(&"a".repeat(41)));
    }

    #[test]
    fn creator_40_multibyte_chars_is_valid() {
        assert!(validate_creator_length(&"あ".repeat(40)));
    }

    #[test]
    fn creator_41_multibyte_chars_is_invalid() {
        assert!(!validate_creator_length(&"あ".repeat(41)));
    }

    // --- validate_comment_length ---

    #[test]
    fn comment_140_ascii_chars_is_valid() {
        assert!(validate_comment_length(&"a".repeat(140)));
    }

    #[test]
    fn comment_141_ascii_chars_is_invalid() {
        assert!(!validate_comment_length(&"a".repeat(141)));
    }

    #[test]
    fn comment_140_multibyte_chars_is_valid() {
        assert!(validate_comment_length(&"あ".repeat(140)));
    }

    #[test]
    fn comment_141_multibyte_chars_is_invalid() {
        assert!(!validate_comment_length(&"あ".repeat(141)));
    }
}
