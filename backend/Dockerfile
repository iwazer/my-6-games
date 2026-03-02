FROM rust:1.93-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 依存クレートのキャッシュ最適化
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# ソースをコピーしてビルド
COPY src ./src
COPY templates ./templates
RUN touch src/main.rs && cargo build --release

# ランタイムイメージ
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    fonts-noto-cjk \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/my-6-games /usr/local/bin/my-6-games
COPY --from=builder /app/templates ./templates

EXPOSE 8000

CMD ["my-6-games"]
