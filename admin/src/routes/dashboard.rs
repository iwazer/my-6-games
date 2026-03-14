// Phase C で本実装予定
use axum::{extract::State, response::Html};
use tera::Context;

use crate::state::AppState;

pub async fn index(State(state): State<AppState>) -> Html<String> {
    let ctx = Context::new();
    match state.tera.render("dashboard.html.tera", &ctx) {
        Ok(html) => Html(html),
        Err(e) => {
            tracing::error!("ダッシュボードのレンダリングエラー: {}", e);
            Html("<p>Internal error</p>".to_string())
        }
    }
}
