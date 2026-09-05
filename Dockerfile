# syntax=docker/dockerfile:1.6
FROM rust:bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --bin aivory-mail-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN useradd -m aivory
WORKDIR /app
COPY --from=builder /app/target/release/aivory-mail-api /usr/local/bin/aivory-mail
COPY migrations ./migrations
RUN mkdir -p /app/data/mail-storage && chown -R aivory:aivory /app
USER aivory
EXPOSE 8095
HEALTHCHECK --interval=15s --timeout=5s --retries=5 CMD curl -sf http://localhost:8095/health || exit 1
CMD ["aivory-mail"]
