use chrono::{TimeZone, Utc};
use redis::aio::ConnectionManager;
use rocket::http::Status;
use rocket::State;
use rocket_dyn_templates::{context, Template};
use sqlx::{MySqlPool, Row};

use crate::config::AppConfig;
use crate::models::share::{Share, ShareGame};

const INITIAL_DAYS: i64 = 30;
const MAX_DAYS: i64 = 90;

/// GET / — トップページ（ゲーム選択 UI）
#[get("/")]
pub fn index() -> Template {
    Template::render("index", context! {})
}

/// GET /s/<id> — 共有ページ（SSR + OGP）
#[get("/s/<id>")]
pub async fn share_page(
    id: &str,
    db: &State<MySqlPool>,
    redis: &State<ConnectionManager>,
    cfg: &State<AppConfig>,
) -> Result<Template, Status> {
    let share = fetch_share(id, db.inner(), redis.inner()).await?;

    let creator = share
        .creator
        .clone()
        .unwrap_or_else(|| "名無し".to_string());
    let game_names: Vec<&str> = share.games.iter().map(|g| g.name.as_str()).collect();
    let og_description = game_names.join("、");
    let share_url = format!("{}/s/{}", cfg.base_url, share.id);
    let tweet_text = format!("{}を構成する6つのゲーム\n{}", creator, og_description);
    let bsky_text = format!(
        "{}を構成する6つのゲーム\n{}\n{}",
        creator, og_description, share_url
    );

    Ok(Template::render(
        "share",
        context! {
            share_id: &share.id,
            creator: &creator,
            games: &share.games,
            base_url: &cfg.base_url,
            og_description: &og_description,
            share_url: &share_url,
            tweet_text: &tweet_text,
            bsky_text: &bsky_text,
        },
    ))
}

/// Redis または DB から共有データを取得する
async fn fetch_share(id: &str, db: &MySqlPool, redis: &ConnectionManager) -> Result<Share, Status> {
    // Redis キャッシュ確認
    {
        let mut conn = redis.clone();
        if let Ok(cached) = redis::cmd("GET")
            .arg(format!("share:{id}"))
            .query_async::<String>(&mut conn)
            .await
        {
            if let Ok(share) = serde_json::from_str::<Share>(&cached) {
                return Ok(share);
            }
        }
    }

    // DB から取得（期限切れは除外）
    let row = sqlx::query(
        "SELECT id, creator, games_json, created_at, expires_at \
         FROM shares WHERE id = ? AND expires_at > NOW()",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(|e| {
        eprintln!("DB select error: {e}");
        Status::InternalServerError
    })?;

    let row = row.ok_or(Status::NotFound)?;

    let share_id: String = row.try_get("id").unwrap_or_default();
    let creator: Option<String> = row.try_get("creator").ok().flatten();
    let games_json: String = row.try_get("games_json").unwrap_or_default();
    let created_at_naive: chrono::NaiveDateTime = row
        .try_get("created_at")
        .map_err(|_| Status::InternalServerError)?;
    let expires_at_naive: chrono::NaiveDateTime = row
        .try_get("expires_at")
        .map_err(|_| Status::InternalServerError)?;

    let games: Vec<ShareGame> = serde_json::from_str(&games_json).map_err(|e| {
        eprintln!("JSON deserialize error: {e}");
        Status::InternalServerError
    })?;

    // accessed_at・expires_at を更新（最大 90 日）
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
    .execute(db)
    .await;

    let new_expires_at_naive = sqlx::query("SELECT expires_at FROM shares WHERE id = ?")
        .bind(&share_id)
        .fetch_one(db)
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

    cache_share_redis(&share, redis).await;

    Ok(share)
}

/// Redis に share データをキャッシュする（失敗しても無視）
async fn cache_share_redis(share: &Share, redis: &ConnectionManager) {
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
