# One image: the Rust server hosts the exported web app, /api and /mcp on a
# single port (8080).

# ---- Web app (Next.js static export) -----------------------------------------
FROM oven/bun:1 AS web
WORKDIR /web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

# ---- Server -------------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /server
# Build dependencies on their own so code changes don't rebuild them.
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY server/migrations ./migrations
COPY server/src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---- Runtime ------------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app methods \
    && mkdir -p /data/files && chown -R methods /data
WORKDIR /app
COPY --from=server /server/target/release/methods-server /usr/local/bin/methods-server
COPY --from=web /web/out ./web
ENV BIND_ADDR=0.0.0.0:8080 \
    STATIC_DIR=/app/web \
    LOCAL_STORAGE_DIR=/data/files \
    RUST_LOG=info,sqlx=warn
# Uploads land here unless S3_BUCKET is set.
VOLUME /data
USER methods
EXPOSE 8080
CMD ["methods-server"]
