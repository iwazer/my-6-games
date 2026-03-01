use serde::{Deserialize, Serialize};

/// ゲーム検索結果・共有データで共通して使うゲーム情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub igdb_id: i64,
    pub name: String,
    pub cover_url: Option<String>,
    pub release_year: Option<i32>,
    pub platforms: Vec<String>,
}
