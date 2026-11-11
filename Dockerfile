# syntax=docker/dockerfile:1.6
FROM rust:bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
COPY migrations ./migrations
# Cache mounts keep the cargo registry and target/ between builds, so a change
# in the workspace crates recompiles only those crates, not every dependency.
# target/ is a cache mount, so the binary is copied out in the same step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,id=avry-mail-target \
    cargo build --release --bin aivory-mail-api \
    && cp target/release/aivory-mail-api /usr/local/bin/aivory-mail-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN useradd -m aivory
WORKDIR /app
COPY --from=builder /usr/local/bin/aivory-mail-api /usr/local/bin/aivory-mail
COPY migrations ./migrations
RUN mkdir -p /app/data/mail-storage && chown -R aivory:aivory /app
USER aivory
EXPOSE 8095
HEALTHCHECK --interval=15s --timeout=5s --retries=5 CMD curl -sf http://localhost:8095/health || exit 1
CMD ["aivory-mail"]
