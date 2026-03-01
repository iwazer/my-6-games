use std::net::IpAddr;

use redis::aio::ConnectionManager;
use rocket::http::Status;
use rocket::request::{self, FromRequest, Request};
use rocket::serde::json::{json, Json};
use rocket::State;
use serde_json::Value;

use crate::services::igdb::{IgdbClient, IgdbError};
use crate::services::rate_limit::{self, RateLimitResult};

/// Caddy (X-Forwarded-For) 経由でクライアント IP を取得するリクエストガード
pub struct ClientIp(pub IpAddr);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientIp {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, ()> {
        let ip = req
            .real_ip()
            .or_else(|| req.client_ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        request::Outcome::Success(ClientIp(ip))
    }
}

/// GET /api/games/search?q=<query>&limit=<n>
///
/// - q: 検索クエリ（必須）
/// - limit: 最大件数（省略時 10、上限 20）
/// - レート制限: 60 リクエスト/分/IP
#[get("/games/search?<q>&<limit>")]
pub async fn search_games(
    q: &str,
    limit: Option<u8>,
    igdb: &State<IgdbClient>,
    redis: &State<ConnectionManager>,
    client_ip: ClientIp,
) -> (Status, Json<Value>) {
    // レート制限チェック（Redis エラー時はスキップ）
    let rate_key = format!("ratelimit:search:{}", client_ip.0);
    if let Ok(RateLimitResult::Exceeded) =
        rate_limit::check(redis.inner(), &rate_key, 60, 60).await
    {
        return (
            Status::TooManyRequests,
            Json(json!({ "error": "rate limit exceeded" })),
        );
    }

    let q = q.trim();
    if q.is_empty() {
        return (
            Status::UnprocessableEntity,
            Json(json!({ "error": "q must not be empty" })),
        );
    }

    let limit = limit.unwrap_or(10).min(20);

    match igdb.search_games(q, limit).await {
        Ok(games) => (Status::Ok, Json(json!({ "games": games }))),
        Err(IgdbError::NotConfigured) => (
            Status::ServiceUnavailable,
            Json(json!({ "error": "IGDB not configured on this server" })),
        ),
        Err(e) => {
            eprintln!("IGDB search error: {e}");
            (
                Status::InternalServerError,
                Json(json!({ "error": "search failed" })),
            )
        }
    }
}
