use std::sync::Arc;

use base64::{engine::general_purpose, Engine as _};

use crate::models::share::Share;

// --- エラー型 ---

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("render error: {0}")]
    Render(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("svg parse error: {0}")]
    Svg(#[from] resvg::usvg::Error),
}

// --- ImageService ---

pub struct ImageService {
    http: reqwest::Client,
    fontdb: Arc<resvg::usvg::fontdb::Database>,
}

impl Default for ImageService {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageService {
    pub fn new() -> Self {
        let mut fontdb = resvg::usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        Self {
            http: reqwest::Client::new(),
            fontdb: Arc::new(fontdb),
        }
    }

    /// Share を受け取り縦長 PNG（900×980、ダウンロード用）を生成して返す
    pub async fn generate_png(&self, share: &Share) -> Result<Vec<u8>, ImageError> {
        let covers = self.fetch_covers(share).await;
        Self::render_svg(build_svg(share, &covers), self.fontdb.clone()).await
    }

    /// Share を受け取り横長 PNG（1200×628、OGP 用）を生成して返す
    pub async fn generate_png_ogp(&self, share: &Share) -> Result<Vec<u8>, ImageError> {
        let covers = self.fetch_covers(share).await;
        Self::render_svg(build_svg_ogp(share, &covers), self.fontdb.clone()).await
    }

    /// 各ゲームのカバー画像を並列フェッチして base64 エンコードする
    async fn fetch_covers(&self, share: &Share) -> Vec<Option<String>> {
        let tasks: Vec<_> = share
            .games
            .iter()
            .map(|game| {
                let url = game.cover_url.clone();
                let http = self.http.clone();
                tokio::spawn(async move {
                    if let Some(url) = url {
                        fetch_cover_base64(&url, &http).await.ok()
                    } else {
                        None
                    }
                })
            })
            .collect();
        let mut covers = Vec::with_capacity(tasks.len());
        for task in tasks {
            covers.push(task.await.unwrap_or(None));
        }
        covers
    }

    /// SVG 文字列を spawn_blocking で PNG バイト列に変換する
    async fn render_svg(
        svg_string: String,
        fontdb: Arc<resvg::usvg::fontdb::Database>,
    ) -> Result<Vec<u8>, ImageError> {
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, ImageError> {
            let options = resvg::usvg::Options::default();
            let tree = resvg::usvg::Tree::from_str(&svg_string, &options, &fontdb)?;
            let size = tree.size().to_int_size();
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
                .ok_or_else(|| ImageError::Render("pixmap allocation failed".into()))?;
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
            );
            pixmap
                .encode_png()
                .map_err(|e| ImageError::Render(e.to_string()))
        })
        .await?
    }
}

// --- ヘルパー関数 ---

async fn fetch_cover_base64(url: &str, client: &reqwest::Client) -> Result<String, reqwest::Error> {
    let url = if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_owned()
    };
    let bytes = client.get(&url).send().await?.bytes().await?;
    Ok(general_purpose::STANDARD.encode(&bytes))
}

/// テキストを max_chars 文字で切り詰める（超過時は末尾を "…" に置換）
pub(crate) fn truncate_text(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        let mut t: String = chars[..max_chars - 1].iter().collect();
        t.push('…');
        t
    }
}

/// SVG/XML の特殊文字をエスケープする
pub(crate) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Share データから SVG 文字列（900×980）を組み立てる
/// カバー画像は IGDB cover_big（264×374）の縦横比を維持して表示する
fn build_svg(share: &Share, covers: &[Option<String>]) -> String {
    const W: u32 = 900;
    const H: u32 = 980;
    const CARD_W: u32 = 280;
    const CARD_H: u32 = 436;
    const COL_X: [u32; 3] = [16, 310, 604];
    const ROW_Y: [u32; 2] = [68, 518];
    // cover_big は 264×374（縦横比 ≈ 0.706）; カード両側 8px パディング
    const COVER_W: u32 = 264;
    const COVER_H: u32 = 374;
    const COVER_X_OFF: u32 = (CARD_W - COVER_W) / 2; // 8
    const COVER_Y_OFF: u32 = 10;
    const TITLE_Y_OFF: u32 = COVER_Y_OFF + COVER_H + 18; // 402
    const YEAR_Y_OFF: u32 = TITLE_Y_OFF + 18; // 420

    let creator = share.creator.as_deref().unwrap_or("?");
    let creator_esc = escape_xml(&truncate_text(creator, 20));
    let header_cx = W / 2;
    let footer_x = W - 16;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
  <defs>
    <style>text {{ font-family: 'Noto Sans CJK JP', 'Noto Sans', sans-serif; }}</style>
  </defs>
  <rect width="{W}" height="{H}" fill="#0f0f14"/>
  <text x="{header_cx}" y="44" text-anchor="middle" font-size="22" font-weight="bold" fill="#c084fc">{creator_esc}を構成する6つのゲーム</text>
  <text x="{footer_x}" y="968" text-anchor="end" font-size="13" fill="#6b7280">my-6-games</text>"##
    );

    for (i, game) in share.games.iter().enumerate() {
        let col = i % 3;
        let row = i / 3;
        let card_x = COL_X[col];
        let card_y = ROW_Y[row];
        let cover_x = card_x + COVER_X_OFF;
        let cover_y = card_y + COVER_Y_OFF;
        let text_cx = card_x + CARD_W / 2;
        let title_y = card_y + TITLE_Y_OFF;
        let year_y = card_y + YEAR_Y_OFF;
        let name = escape_xml(&truncate_text(&game.name, 22));

        // カード背景
        svg.push_str(&format!(
            "\n  <rect x=\"{card_x}\" y=\"{card_y}\" width=\"{CARD_W}\" height=\"{CARD_H}\" rx=\"8\" fill=\"#1f2028\" stroke=\"#374151\" stroke-width=\"1\"/>"
        ));

        // カバー画像（縦横比維持）or プレースホルダー
        if let Some(Some(b64)) = covers.get(i) {
            svg.push_str(&format!(
                "\n  <image x=\"{cover_x}\" y=\"{cover_y}\" width=\"{COVER_W}\" height=\"{COVER_H}\" href=\"data:image/jpeg;base64,{b64}\" preserveAspectRatio=\"xMidYMid meet\"/>"
            ));
        } else {
            svg.push_str(&format!(
                "\n  <rect x=\"{cover_x}\" y=\"{cover_y}\" width=\"{COVER_W}\" height=\"{COVER_H}\" rx=\"4\" fill=\"#374151\"/>"
            ));
        }

        // タイトル
        svg.push_str(&format!(
            "\n  <text x=\"{text_cx}\" y=\"{title_y}\" text-anchor=\"middle\" font-size=\"13\" fill=\"#ffffff\">{name}</text>"
        ));

        // 発売年
        if let Some(year) = game.release_year {
            let year_esc = format!("{year}年");
            svg.push_str(&format!(
                "\n  <text x=\"{text_cx}\" y=\"{year_y}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#9ca3af\">{year_esc}</text>"
            ));
        }
    }

    svg.push_str("\n</svg>");
    svg
}

/// Share データから OGP 用 SVG 文字列（1200×628、横長）を組み立てる
/// カバー画像はカード全幅に xMidYMid slice で表示する
fn build_svg_ogp(share: &Share, covers: &[Option<String>]) -> String {
    const W: u32 = 1200;
    const H: u32 = 628;
    const CARD_W: u32 = 380;
    const CARD_H: u32 = 255;
    const COL_X: [u32; 3] = [16, 410, 804];
    const ROW_Y: [u32; 2] = [64, 333];
    // カバーはカード全幅・上部を xMidYMid slice でクロップ表示
    const COVER_H: u32 = 215;
    const NAME_Y_OFF: u32 = COVER_H + 24; // 239

    let creator = share.creator.as_deref().unwrap_or("?");
    let creator_esc = escape_xml(&truncate_text(creator, 20));
    let header_cx = W / 2;
    let footer_x = W - 16;

    // clipPath 生成（上角丸、下辺ストレート）
    let mut clip_defs = String::new();
    for i in 0u32..6 {
        let col = (i % 3) as usize;
        let row = (i / 3) as usize;
        let cx = COL_X[col];
        let cy = ROW_Y[row];
        let (x0, y0) = (cx, cy);
        let (x1, y1) = (cx + CARD_W, cy + COVER_H);
        let r = 8u32;
        clip_defs.push_str(&format!(
            "<clipPath id=\"ogp{i}\"><path d=\"M {},{} Q {},{} {},{} L {},{} Q {},{} {},{} L {},{} L {},{} Z\"/></clipPath>",
            x0, y0 + r,
            x0, y0, x0 + r, y0,
            x1 - r, y0,
            x1, y0, x1, y0 + r,
            x1, y1,
            x0, y1,
        ));
    }

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
  <defs>
    <style>text {{ font-family: 'Noto Sans CJK JP', 'Noto Sans', sans-serif; }}</style>
    {clip_defs}
  </defs>
  <rect width="{W}" height="{H}" fill="#0f0f14"/>
  <text x="{header_cx}" y="42" text-anchor="middle" font-size="22" font-weight="bold" fill="#c084fc">{creator_esc}を構成する6つのゲーム</text>
  <text x="{footer_x}" y="618" text-anchor="end" font-size="13" fill="#6b7280">my-6-games</text>"##
    );

    for (i, game) in share.games.iter().enumerate() {
        let col = i % 3;
        let row = i / 3;
        let card_x = COL_X[col];
        let card_y = ROW_Y[row];
        let text_cx = card_x + CARD_W / 2;
        let name_y = card_y + NAME_Y_OFF;
        let name = escape_xml(&truncate_text(&game.name, 22));

        // カード背景
        svg.push_str(&format!(
            "\n  <rect x=\"{card_x}\" y=\"{card_y}\" width=\"{CARD_W}\" height=\"{CARD_H}\" rx=\"8\" fill=\"#1f2028\" stroke=\"#374151\" stroke-width=\"1\"/>"
        ));

        // カバー画像（全幅スライス）or プレースホルダー
        if let Some(Some(b64)) = covers.get(i) {
            svg.push_str(&format!(
                "\n  <image x=\"{card_x}\" y=\"{card_y}\" width=\"{CARD_W}\" height=\"{COVER_H}\" clip-path=\"url(#ogp{i})\" href=\"data:image/jpeg;base64,{b64}\" preserveAspectRatio=\"xMidYMid slice\"/>"
            ));
        } else {
            svg.push_str(&format!(
                "\n  <rect x=\"{card_x}\" y=\"{card_y}\" width=\"{CARD_W}\" height=\"{COVER_H}\" clip-path=\"url(#ogp{i})\" fill=\"#374151\"/>"
            ));
        }

        // タイトル
        svg.push_str(&format!(
            "\n  <text x=\"{text_cx}\" y=\"{name_y}\" text-anchor=\"middle\" font-size=\"13\" fill=\"#ffffff\">{name}</text>"
        ));
    }

    svg.push_str("\n</svg>");
    svg
}

// --- 単体テスト ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::share::ShareGame;
    use chrono::Utc;

    fn make_share(creator: Option<&str>) -> Share {
        Share {
            id: "test123".to_string(),
            creator: creator.map(str::to_string),
            games: (0..6)
                .map(|i| ShareGame {
                    igdb_id: i,
                    name: format!("Test Game {i}"),
                    cover_url: None,
                    release_year: Some(2020 + i as i32),
                    platforms: vec![],
                    comment: None,
                    is_spoiler: false,
                })
                .collect(),
            created_at: Utc::now(),
            expires_at: Utc::now(),
        }
    }

    // --- truncate_text ---

    #[test]
    fn truncate_within_limit_unchanged() {
        assert_eq!(truncate_text("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_limit_unchanged() {
        assert_eq!(truncate_text("hello", 5), "hello");
    }

    #[test]
    fn truncate_over_limit_ascii() {
        let result = truncate_text("hello world", 6);
        assert_eq!(result, "hello…");
        assert_eq!(result.chars().count(), 6);
    }

    #[test]
    fn truncate_over_limit_multibyte() {
        let s = "あいうえおかきくけこ"; // 10 文字
        let result = truncate_text(s, 6);
        assert_eq!(result.chars().count(), 6);
        assert!(result.ends_with('…'));
    }

    // --- escape_xml ---

    #[test]
    fn escape_xml_ampersand() {
        assert_eq!(escape_xml("a & b"), "a &amp; b");
    }

    #[test]
    fn escape_xml_lt_gt() {
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn escape_xml_quote() {
        assert_eq!(escape_xml(r#"say "hi""#), "say &quot;hi&quot;");
    }

    #[test]
    fn escape_xml_no_special_chars_unchanged() {
        assert_eq!(escape_xml("hello"), "hello");
    }

    // --- build_svg ---

    #[test]
    fn build_svg_starts_and_ends_with_svg_tags() {
        let share = make_share(Some("テストユーザー"));
        let svg = build_svg(&share, &vec![None; 6]);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn build_svg_contains_creator_name() {
        let share = make_share(Some("Alice"));
        let svg = build_svg(&share, &vec![None; 6]);
        assert!(svg.contains("Alice"));
    }

    #[test]
    fn build_svg_contains_all_game_names() {
        let share = make_share(Some("Bob"));
        let svg = build_svg(&share, &vec![None; 6]);
        for i in 0..6 {
            assert!(svg.contains(&format!("Test Game {i}")));
        }
    }

    #[test]
    fn build_svg_uses_placeholder_color_when_no_cover() {
        let share = make_share(None);
        let svg = build_svg(&share, &vec![None; 6]);
        assert!(!svg.contains("data:image/jpeg"));
        assert!(svg.contains("#374151")); // プレースホルダー色
    }
}
