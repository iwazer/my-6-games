use axum::{extract::State, response::Html};
use chrono::Local;
use sqlx::Row;
use tera::Context;

use crate::state::AppState;

pub async fn index(State(state): State<AppState>) -> Html<String> {
    match render(&state).await {
        Ok(html) => Html(html),
        Err(e) => {
            tracing::error!("ダッシュボードのエラー: {}", e);
            Html("<p>Internal Server Error</p>".to_string())
        }
    }
}

async fn render(state: &AppState) -> anyhow::Result<String> {
    // 統計（総数・アクティブ数）
    let stats_row = sqlx::query(
        "SELECT COUNT(*) AS total, \
         COALESCE(SUM(CASE WHEN expires_at > NOW() THEN 1 ELSE 0 END), 0) AS active \
         FROM shares",
    )
    .fetch_one(&state.db)
    .await?;

    let total: i64 = stats_row.try_get("total")?;
    let active: i64 = stats_row.try_get("active")?;

    // 日別作成数（直近30日、グラフ用）
    let day_rows = sqlx::query(
        "SELECT DATE(created_at) AS date, COUNT(*) AS count \
         FROM shares \
         WHERE created_at >= NOW() - INTERVAL 30 DAY \
         GROUP BY DATE(created_at) \
         ORDER BY date",
    )
    .fetch_all(&state.db)
    .await?;

    let mut chart_labels: Vec<String> = Vec::new();
    let mut chart_data: Vec<i64> = Vec::new();
    for row in &day_rows {
        let date: chrono::NaiveDate = row.try_get("date")?;
        let count: i64 = row.try_get("count")?;
        chart_labels.push(date.format("%m/%d").to_string());
        chart_data.push(count);
    }

    // 最近の共有（直近20件）
    let share_rows = sqlx::query(
        "SELECT id, creator, created_at, expires_at \
         FROM shares \
         ORDER BY created_at DESC \
         LIMIT 20",
    )
    .fetch_all(&state.db)
    .await?;

    let now = Local::now().naive_local();
    let recent_shares: Vec<serde_json::Value> = share_rows
        .iter()
        .map(|row| {
            let id: String = row.try_get("id").unwrap_or_default();
            let creator: Option<String> = row.try_get("creator").unwrap_or(None);
            let created_at: chrono::NaiveDateTime = row.try_get("created_at").unwrap_or_default();
            let expires_at: chrono::NaiveDateTime = row.try_get("expires_at").unwrap_or_default();
            serde_json::json!({
                "id": id,
                "creator": creator.unwrap_or_else(|| "（なし）".to_string()),
                "created_at": created_at.format("%Y-%m-%d %H:%M").to_string(),
                "expires_at": expires_at.format("%Y-%m-%d").to_string(),
                "is_active": expires_at > now,
            })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("total", &total);
    ctx.insert("active", &active);
    ctx.insert("expired", &(total - active));
    ctx.insert("chart_labels", &chart_labels);
    ctx.insert("chart_data", &chart_data);
    ctx.insert("recent_shares", &recent_shares);

    Ok(state.tera.render("dashboard.html.tera", &ctx)?)
}
