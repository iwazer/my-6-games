use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// games_json カラムに格納されるゲームエントリ（コメント・スポイラー含む）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareGame {
    pub igdb_id: i64,
    pub name: String,
    /// IGDB から取得した元のタイトル。ユーザーが name を変更した場合にのみ Some になる。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    pub cover_url: Option<String>,
    pub release_year: Option<i32>,
    pub platforms: Vec<String>,
    pub comment: Option<String>,
    pub is_spoiler: bool,
}

/// 共有レコード（API レスポンス用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub id: String,
    pub creator: Option<String>,
    pub games: Vec<ShareGame>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
