FROM rust:1-bookworm AS builder
WORKDIR /app
ARG GIT_COMMIT=dev
ENV GIT_COMMIT=$GIT_COMMIT
# Fly.io / Render: single-job release build avoids OOM on small VMs
ENV CARGO_BUILD_JOBS=1
ENV RUSTFLAGS="-C codegen-units=1"
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY frontend ./frontend
RUN cargo build --locked --release -p api-gateway --bin vantage-earn \
    && rm -rf target/release/deps target/release/build target/release/incremental

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/vantage-earn /app/vantage-earn
COPY crates/api-gateway/migrations ./migrations
COPY templates/email ./templates/email
ENV EMAIL_TEMPLATES_DIR=/app/templates/email
# Fly.io and Render inject PORT at runtime; 3000 fallback for local docker-compose
ENV PORT=3000
CMD ["./vantage-earn"]
