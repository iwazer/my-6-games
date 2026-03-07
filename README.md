# my-6-games

「私を構成する6つのゲーム」を選んで共有ページを作れるサービスです。

## 機能

- IGDB からゲームを検索して6本選択
- 各ゲームに一言コメント・ネタバレフラグを設定
- 共有ページを生成（URL: `/s/<id>`）
- OGP 対応（X / Bluesky へのシェアボタン付き）
- 共有ページから PNG 画像をダウンロード

## 技術スタック

| 役割 | 技術 |
|------|------|
| バックエンド | Rust + [Rocket](https://rocket.rs/) 0.5 |
| テンプレート | [Tera](https://keats.github.io/tera/) (SSR) |
| フロントエンド | [Alpine.js](https://alpinejs.dev/) + [Tailwind CSS](https://tailwindcss.com/) (CDN) |
| 画像生成 | [resvg](https://github.com/linebender/resvg) (SVG→PNG) |
| DB | MariaDB 11 |
| キャッシュ | Redis 7 |
| リバースプロキシ | Caddy 2 (自動 HTTPS) |
| コンテナ | Docker Compose |

## セットアップ

### 前提

- Docker / Docker Compose
- [just](https://github.com/casey/just)（タスクランナー）
- IGDB API キー（[Twitch Developer Console](https://dev.twitch.tv/console) で取得）

### 起動

```bash
# 1. 環境変数を設定
cp .env.example .env
# .env を編集して TWITCH_CLIENT_ID / TWITCH_CLIENT_SECRET / ROCKET_SECRET_KEY を設定
# ROCKET_SECRET_KEY は openssl rand -base64 32 で生成

# 2. コンテナを起動
just up

# 3. 動作確認
curl http://localhost/health
# → {"status":"ok","db":"ok","cache":"ok"}
```

ブラウザで `http://localhost` を開くとトップページが表示されます。

## 開発コマンド

```bash
just fmt      # コードフォーマット
just lint     # Clippy（警告をエラーとして扱う）
just test     # 単体テスト
just logs     # アプリログをストリーミング表示
just health   # ヘルスチェック
just search zelda   # ゲーム検索 API を叩く
```

## アーキテクチャ

```
[ブラウザ]
    │
    ▼
[Caddy]  ← 自動 HTTPS / gzip 圧縮
    │
    ▼
[Rocket (Rust)]
    ├── [Redis]   ← IGDB キャッシュ / 共有キャッシュ / 画像キャッシュ / レート制限
    ├── [MariaDB] ← 共有データの永続化
    └── [IGDB API]
```

### パフォーマンス設計

- **共有閲覧**: Redis キャッシュ優先（DB へのアクセスを最小化）
- **IGDB 検索**: Redis 24h キャッシュ + フロントのデバウンス（400ms）
- **画像生成**: 初回生成後は Redis にキャッシュ（有効期限まで）
- **レート制限**: 検索 60 req/min・作成 10 req/h（Redis INCR+EXPIRE）

## ライセンス

MIT