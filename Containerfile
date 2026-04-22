# ── Builder stage: compile all workspace binaries ──
FROM rust:1.87-bookworm AS builder

WORKDIR /build

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.toml
COPY common/Cargo.toml common/Cargo.toml
COPY catalog-service/Cargo.toml catalog-service/Cargo.toml
COPY streaming-service/Cargo.toml streaming-service/Cargo.toml
COPY user-service/Cargo.toml user-service/Cargo.toml
COPY gateway/Cargo.toml gateway/Cargo.toml

# Create stub source files so cargo can resolve the workspace and cache deps
RUN mkdir -p common/src catalog-service/src streaming-service/src user-service/src gateway/src \
    && echo "pub mod config; pub mod error; pub mod models;" > common/src/lib.rs \
    && touch common/src/config.rs common/src/error.rs common/src/models.rs \
    && echo "fn main() {}" > catalog-service/src/main.rs \
    && echo "fn main() {}" > streaming-service/src/main.rs \
    && echo "fn main() {}" > user-service/src/main.rs \
    && echo "fn main() {}" > gateway/src/main.rs

RUN cargo build --release 2>/dev/null || true

# Copy actual source and rebuild
COPY common/ common/
COPY catalog-service/ catalog-service/
COPY streaming-service/ streaming-service/
COPY user-service/ user-service/
COPY gateway/ gateway/

# Touch source files so cargo knows they changed
RUN touch common/src/lib.rs catalog-service/src/main.rs streaming-service/src/main.rs user-service/src/main.rs gateway/src/main.rs

RUN cargo build --release


# ── Catalog Service ──
FROM debian:bookworm-slim AS catalog-service

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/catalog-service /usr/local/bin/catalog-service

ENV CATALOG_PORT=3001
ENV DATABASE_PATH=/data/catalog.db
EXPOSE 3001

VOLUME ["/data"]

CMD ["catalog-service"]


# ── Streaming Service ──
FROM debian:bookworm-slim AS streaming-service

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/streaming-service /usr/local/bin/streaming-service

ENV STREAMING_PORT=3002
ENV MEDIA_STORE_PATH=/data/media-store
EXPOSE 3002

VOLUME ["/data/media-store"]

CMD ["streaming-service"]


# ── User Service ──
FROM debian:bookworm-slim AS user-service

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/user-service /usr/local/bin/user-service

ENV USER_PORT=3003
ENV USER_DATABASE_PATH=/data/users.db
EXPOSE 3003

VOLUME ["/data"]

CMD ["user-service"]


# ── Gateway ──
FROM debian:bookworm-slim AS gateway

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/gateway /usr/local/bin/gateway

ENV GATEWAY_PORT=3000
EXPOSE 3000

CMD ["gateway"]
