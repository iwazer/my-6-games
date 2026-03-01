use chrono::{Datelike, TimeZone, Utc};
use redis::aio::ConnectionManager;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

use crate::config::AppConfig;
use crate::models::game::Game;

#[derive(Debug, Error)]
pub enum IgdbError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Cache error: {0}")]
    Cache(#[from] redis::RedisError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IGDB not configured: TWITCH_CLIENT_ID / TWITCH_CLIENT_SECRET が未設定")]
    NotConfigured,
}

#[derive(Clone)]
pub struct IgdbClient {
    http: Client,
    client_id: String,
    client_secret: String,
    redis: ConnectionManager,
}

// --- Twitch トークンレスポンス ---

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

// --- IGDB API レスポンス ---

#[derive(Deserialize)]
struct IgdbGame {
    id: i64,
    name: String,
    cover: Option<IgdbCover>,
    first_release_date: Option<i64>,
    platforms: Option<Vec<IgdbPlatform>>,
}

#[derive(Deserialize)]
struct IgdbCover {
    url: String,
}

#[derive(Deserialize)]
struct IgdbPlatform {
    name: String,
}

// --- 純粋関数（テスト可能なロジック） ---

/// IGDB のカバー URL を HTTPS の高解像度 URL に変換する
///
/// 例: `//images.igdb.com/.../t_thumb/co1abc.jpg`
///   → `https://images.igdb.com/.../t_cover_big/co1abc.jpg`
pub(crate) fn normalize_cover_url(url: &str) -> String {
    let url = url.trim_start_matches("//");
    format!("https://{}", url.replace("t_thumb", "t_cover_big"))
}

/// Unix タイムスタンプから年を取得する
pub(crate) fn timestamp_to_year(ts: i64) -> Option<i32> {
    Utc.timestamp_opt(ts, 0).single().map(|dt| dt.year())
}

/// IGDB APIcalypse クエリ文字列内のエスケープが必要な文字を処理する
pub(crate) fn escape_igdb_query(query: &str) -> String {
    query.replace('\\', "\\\\").replace('"', "\\\"")
}

impl IgdbClient {
    pub fn new(cfg: &AppConfig, redis: ConnectionManager) -> Self {
        Self {
            http: Client::new(),
            client_id: cfg.twitch_client_id.clone(),
            client_secret: cfg.twitch_client_secret.clone(),
            redis,
        }
    }

    fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    /// Twitch アクセストークンを取得する（Redis キャッシュ優先）
    async fn get_access_token(&self) -> Result<String, IgdbError> {
        if !self.is_configured() {
            return Err(IgdbError::NotConfigured);
        }

        let mut conn = self.redis.clone();

        // キャッシュヒット
        if let Ok(token) = redis::cmd("GET")
            .arg("igdb:token")
            .query_async::<String>(&mut conn)
            .await
        {
            return Ok(token);
        }

        // Twitch からトークン取得
        let resp = self
            .http
            .post("https://id.twitch.tv/oauth2/token")
            .query(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await?
            .error_for_status()?;

        let token_resp: TokenResponse = resp.json().await?;

        // 有効期限 60 秒前に失効させてキャッシュ
        let ttl = token_resp.expires_in.saturating_sub(60) as usize;
        redis::cmd("SETEX")
            .arg("igdb:token")
            .arg(ttl)
            .arg(&token_resp.access_token)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(token_resp.access_token)
    }

    /// ゲームを検索する（Redis キャッシュ 24h）
    pub async fn search_games(&self, query: &str, limit: u8) -> Result<Vec<Game>, IgdbError> {
        let mut conn = self.redis.clone();
        let cache_key = format!("igdb:search:{}:{}", query.to_lowercase(), limit);

        // キャッシュヒット
        if let Ok(cached) = redis::cmd("GET")
            .arg(&cache_key)
            .query_async::<String>(&mut conn)
            .await
        {
            if let Ok(games) = serde_json::from_str::<Vec<Game>>(&cached) {
                return Ok(games);
            }
        }

        let token = self.get_access_token().await?;

        // IGDB APIcalypse クエリ
        let escaped = escape_igdb_query(query);
        let body = format!(
            r#"search "{escaped}"; fields id,name,cover.url,first_release_date,platforms.name; limit {limit};"#
        );

        let igdb_games: Vec<IgdbGame> = self
            .http
            .post("https://api.igdb.com/v4/games")
            .header("Client-ID", &self.client_id)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "text/plain")
            .body(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let games: Vec<Game> = igdb_games
            .into_iter()
            .map(|g| {
                let cover_url = g.cover.map(|c| normalize_cover_url(&c.url));
                let release_year = g.first_release_date.and_then(timestamp_to_year);

                Game {
                    igdb_id: g.id,
                    name: g.name,
                    cover_url,
                    release_year,
                    platforms: g
                        .platforms
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| p.name)
                        .collect(),
                }
            })
            .collect();

        // 24 時間キャッシュ（失敗してもリクエスト自体は成功として返す）
        if let Ok(json) = serde_json::to_string(&games) {
            let _: Result<(), _> = redis::cmd("SETEX")
                .arg(&cache_key)
                .arg(86400usize)
                .arg(json)
                .query_async::<()>(&mut conn)
                .await;
        }

        Ok(games)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalize_cover_url ---

    #[test]
    fn cover_url_adds_https_and_upgrades_size() {
        let input = "//images.igdb.com/igdb/image/upload/t_thumb/co1abc.jpg";
        let result = normalize_cover_url(input);
        assert_eq!(
            result,
            "https://images.igdb.com/igdb/image/upload/t_cover_big/co1abc.jpg"
        );
    }

    #[test]
    fn cover_url_already_has_https_prefix() {
        // スラッシュなしで始まる URL はそのまま https:// を付与
        let input = "images.igdb.com/igdb/image/upload/t_thumb/co1abc.jpg";
        let result = normalize_cover_url(input);
        assert_eq!(
            result,
            "https://images.igdb.com/igdb/image/upload/t_cover_big/co1abc.jpg"
        );
    }

    #[test]
    fn cover_url_no_thumb_in_path_is_unchanged_aside_from_scheme() {
        let input = "//images.igdb.com/igdb/image/upload/t_cover_big/co1abc.jpg";
        let result = normalize_cover_url(input);
        assert_eq!(
            result,
            "https://images.igdb.com/igdb/image/upload/t_cover_big/co1abc.jpg"
        );
    }

    // --- timestamp_to_year ---

    #[test]
    fn timestamp_1986_returns_1986() {
        // 1986-02-21 00:00:00 UTC (ゼルダの伝説初代の発売日)
        let ts = 509241600_i64;
        assert_eq!(timestamp_to_year(ts), Some(1986));
    }

    #[test]
    fn timestamp_zero_returns_1970() {
        assert_eq!(timestamp_to_year(0), Some(1970));
    }

    #[test]
    fn timestamp_negative_returns_none_or_year() {
        // 1970年以前は timestamp_opt が single() を返せる範囲なら Some
        let result = timestamp_to_year(-1);
        // 1969 または None（実装依存）。パニックしないことを確認
        let _ = result;
    }

    // --- escape_igdb_query ---

    #[test]
    fn escape_query_plain_text_unchanged() {
        assert_eq!(escape_igdb_query("zelda"), "zelda");
    }

    #[test]
    fn escape_query_double_quotes_are_escaped() {
        assert_eq!(escape_igdb_query(r#"the "legend""#), r#"the \"legend\""#);
    }

    #[test]
    fn escape_query_backslash_is_escaped() {
        assert_eq!(escape_igdb_query(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn escape_query_both_special_chars() {
        assert_eq!(escape_igdb_query(r#"a\"b"#), r#"a\\\"b"#);
    }
}
