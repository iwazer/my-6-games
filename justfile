# デフォルト: タスク一覧を表示
default:
    @just --list

# ===== Docker =====

# コンテナをビルドして起動
up:
    docker compose up -d --build

# コンテナを起動（ビルドなし）
start:
    docker compose up -d

# コンテナを停止
down:
    docker compose down

# アプリログをストリーミング表示
logs:
    docker compose logs -f app

# コンテナの状態確認
ps:
    docker compose ps

# ===== 開発 =====

# コードをフォーマット（rustfmt）
fmt:
    cd backend && cargo fmt

# フォーマットチェックのみ（CI 用、ファイルを変更しない）
fmt-check:
    cd backend && cargo fmt --check

# Linter を実行（clippy、警告をエラーとして扱う）
lint:
    cd backend && cargo clippy -- -D warnings

# 単体テストを実行
test:
    cd backend && cargo test

# ===== 動作確認 =====

# ヘルスチェック
health:
    curl -s http://localhost/health

# ゲーム検索（例: just search zelda）
search q:
    curl -s "http://localhost/api/games/search?q={{q}}"

# 共有を取得（例: just share-get a1b2c3d4e5f67890）
share-get id:
    curl -s "http://localhost/api/shares/{{id}}"

# ===== Redis =====

# Redis の全キーを表示
redis-keys:
    docker compose exec cache redis-cli KEYS "*"

# Redis キャッシュを全削除
redis-flush:
    docker compose exec cache redis-cli FLUSHALL
