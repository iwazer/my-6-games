use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use sqlx::Row;
use tera::Context;

use crate::state::AppState;

const PAGE_SIZE: i64 = 20;

// ---------- 一覧 ----------

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub q: Option<String>,
}

pub async fn list(State(state): State<AppState>, Query(query): Query<ListQuery>) -> Html<String> {
    match render_list(&state, &query).await {
        Ok(html) => Html(html),
        Err(e) => {
            tracing::error!("一覧のエラー: {}", e);
            Html("<p>Internal Server Error</p>".to_string())
        }
    }
}

async fn render_list(state: &AppState, query: &ListQuery) -> anyhow::Result<String> {
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let q = query.q.as_deref().unwrap_or("").trim().to_string();

    let (total, rows) = if q.is_empty() {
        let total: i64 = sqlx::query("SELECT COUNT(*) AS total FROM shares")
            .fetch_one(&state.db)
            .await?
            .try_get("total")?;

        let rows = sqlx::query(
            "SELECT id, creator, created_at, expires_at \
             FROM shares ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        (total, rows)
    } else {
        let like = format!("%{}%", q);

        let total: i64 = sqlx::query("SELECT COUNT(*) AS total FROM shares WHERE creator LIKE ?")
            .bind(&like)
            .fetch_one(&state.db)
            .await?
            .try_get("total")?;

        let rows = sqlx::query(
            "SELECT id, creator, created_at, expires_at \
             FROM shares WHERE creator LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(&like)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        (total, rows)
    };

    let now = chrono::Local::now().naive_local();
    let shares: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let id: String = row.try_get("id").unwrap_or_default();
            let creator: Option<String> = row.try_get("creator").unwrap_or(None);
            let created_at: NaiveDateTime = row.try_get("created_at").unwrap_or_default();
            let expires_at: NaiveDateTime = row.try_get("expires_at").unwrap_or_default();
            serde_json::json!({
                "id": id,
                "creator": creator.unwrap_or_else(|| "（なし）".to_string()),
                "created_at": created_at.format("%Y-%m-%d %H:%M").to_string(),
                "expires_at": expires_at.format("%Y-%m-%d").to_string(),
                "is_active": expires_at > now,
            })
        })
        .collect();

    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;

    let mut ctx = Context::new();
    ctx.insert("shares", &shares);
    ctx.insert("q", &q);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("total", &total);

    Ok(state.tera.render("shares/list.html.tera", &ctx)?)
}

// ---------- 詳細 ----------

pub async fn detail(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match render_detail(&state, &id).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("詳細のエラー: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
        }
    }
}

async fn render_detail(state: &AppState, id: &str) -> anyhow::Result<axum::response::Response> {
    let row = sqlx::query(
        "SELECT id, creator, games_json, created_at, accessed_at, expires_at \
         FROM shares WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let Some(row) = row else {
        return Ok((StatusCode::NOT_FOUND, "共有が見つかりません").into_response());
    };

    let share_id: String = row.try_get("id")?;
    let creator: Option<String> = row.try_get("creator")?;
    let games_json: serde_json::Value = row.try_get("games_json")?;
    let created_at: NaiveDateTime = row.try_get("created_at")?;
    let accessed_at: NaiveDateTime = row.try_get("accessed_at")?;
    let expires_at: NaiveDateTime = row.try_get("expires_at")?;
    let is_active = expires_at > chrono::Local::now().naive_local();

    let mut ctx = Context::new();
    ctx.insert("id", &share_id);
    ctx.insert("creator", &creator.unwrap_or_default());
    ctx.insert("games", &games_json);
    ctx.insert(
        "created_at",
        &created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    );
    ctx.insert(
        "accessed_at",
        &accessed_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    );
    ctx.insert(
        "expires_at_display",
        &expires_at.format("%Y-%m-%d %H:%M").to_string(),
    );
    ctx.insert(
        "expires_at_input",
        &expires_at.format("%Y-%m-%dT%H:%M").to_string(),
    );
    ctx.insert("is_active", &is_active);

    Ok(Html(state.tera.render("shares/detail.html.tera", &ctx)?).into_response())
}

// ---------- 編集 ----------

#[derive(Deserialize)]
pub struct EditForm {
    pub creator: String,
    pub expires_at: String, // "YYYY-MM-DDTHH:MM"
}

pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<EditForm>,
) -> impl IntoResponse {
    let expires_at = match NaiveDateTime::parse_from_str(&form.expires_at, "%Y-%m-%dT%H:%M") {
        Ok(dt) => dt,
        Err(_) => return (StatusCode::BAD_REQUEST, "日付形式が不正です").into_response(),
    };

    let creator: Option<&str> = if form.creator.trim().is_empty() {
        None
    } else {
        Some(form.creator.trim())
    };

    let result = sqlx::query("UPDATE shares SET creator = ?, expires_at = ? WHERE id = ?")
        .bind(creator)
        .bind(expires_at)
        .bind(&id)
        .execute(&state.db)
        .await;

    if let Err(e) = result {
        tracing::error!("編集エラー: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
    }

    // Redis キャッシュを無効化
    invalidate_cache(&mut state.redis.clone(), &id).await;

    Redirect::to(&format!("/shares/{}", id)).into_response()
}

// ---------- 削除 ----------

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = sqlx::query("DELETE FROM shares WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
    {
        tracing::error!("削除エラー: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response();
    }

    invalidate_cache(&mut state.redis.clone(), &id).await;
    tracing::info!("共有を削除しました: {}", id);

    Redirect::to("/shares").into_response()
}

// ---------- ヘルパー ----------

/// DB 削除・編集後に関連 Redis キャッシュをすべて削除する
async fn invalidate_cache(redis: &mut redis::aio::ConnectionManager, id: &str) {
    let keys = [
        format!("share:{}", id),
        format!("share:image:{}", id),
        format!("share:image:ogp:{}", id),
    ];
    if let Err(e) = redis::cmd("DEL").arg(&keys).query_async::<()>(redis).await {
        tracing::warn!("Redis キャッシュ削除に失敗: {}", e);
    }
}
