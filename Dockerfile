FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY frontend ./frontend
RUN cargo build --release -p api-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/vantage-earn /app/vantage-earn
COPY crates/api-gateway/migrations ./migrations
ENV PORT=3000
EXPOSE 3000
CMD ["./vantage-earn"]
