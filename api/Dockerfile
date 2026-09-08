# 多階段（規格 §16）：rust 1.98 建置 → debian-slim 執行
FROM rust:1.98-bookworm AS build
WORKDIR /src
# migrations 在編譯期被 sqlx::migrate! 內嵌，要在 build 前 COPY 進來
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
RUN cargo build --release --locked

FROM debian:bookworm-slim
# ca-certificates：對綠界／SMTP 的 TLS（計畫 3 交接 9）；curl：compose healthcheck
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home app \
 && mkdir -p /data/uploads && chown -R app:app /data
COPY --from=build /src/target/release/api /usr/local/bin/api
USER app
ENV UPLOAD_DIR=/data/uploads \
    LISTEN_ADDR=0.0.0.0:8080 \
    RUST_LOG=info,tower_http=info
EXPOSE 8080
VOLUME ["/data/uploads"]
CMD ["api"]
