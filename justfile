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

# 単体テストを実行（Docker 内で cargo test）
test:
    docker run --rm \
        -v "$(pwd)/backend:/app" \
        -w /app \
        rust:1.93-slim \
        sh -c "apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev 2>/dev/null \
               && cargo test 2>&1"

# ===== 動作確認 =====

# ヘルスチェック
health:
    curl -s http://localhost/health

# ゲーム検索（例: just search zelda）
search q:
    curl -s "http://localhost/api/games/search?q={{q}}"

# ===== Redis =====

# Redis の全キーを表示
redis-keys:
    docker compose exec cache redis-cli KEYS "*"

# Redis キャッシュを全削除
redis-flush:
    docker compose exec cache redis-cli FLUSHALL
