use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

// Phase C/D で各ルートから使用する
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum AppError {
    #[error("データベースエラー: {0}")]
    Database(#[from] sqlx::Error),
    #[error("キャッシュエラー: {0}")]
    Cache(#[from] redis::RedisError),
    #[error("内部エラー: {0}")]
    Internal(#[from] anyhow::Error),
    #[error("見つかりません")]
    NotFound,
    #[error("認証が必要です")]
    Unauthorized,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            _ => {
                tracing::error!("{}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "内部エラーが発生しました".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;
