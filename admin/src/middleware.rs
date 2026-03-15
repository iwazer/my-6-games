use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use crate::{session, state::AppState};

pub const SESSION_COOKIE: &str = "admin_session";

/// Cookie ヘッダーからセッション ID を取り出す
pub fn extract_session_id(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_str = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_str.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(&format!("{}=", SESSION_COOKIE))
            .map(str::to_string)
    })
}

/// 認証チェックミドルウェア（未認証なら /auth/login へリダイレクト）
pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let session_id = match extract_session_id(request.headers()) {
        Some(id) => id,
        None => return Redirect::to("/auth/login").into_response(),
    };

    let mut redis = state.redis.clone();
    match session::get_session_email(&mut redis, &session_id).await {
        Ok(Some(_)) => next.run(request).await,
        Ok(None) => Redirect::to("/auth/login").into_response(),
        Err(e) => {
            tracing::error!("セッション検証エラー: {}", e);
            Redirect::to("/auth/login").into_response()
        }
    }
}
