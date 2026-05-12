# syntax=docker/dockerfile:1.7

# ---- builder ---------------------------------------------------------------
FROM rust:1.85-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p spades-server \
    && cp target/release/spades-server /spades-server

# ---- runtime ---------------------------------------------------------------
FROM debian:12-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 1000 --user-group --no-create-home spades

COPY --from=builder /spades-server /usr/local/bin/spades-server

USER spades
WORKDIR /data
EXPOSE 3000

ENV DATABASE_URL=/data/games.sqlite

ENTRYPOINT ["/usr/local/bin/spades-server"]
CMD ["--port", "3000", "--db", "/data/games.sqlite"]
