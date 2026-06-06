# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Builder: produce a fully static (musl) binary so the runtime image's glibc
# version is irrelevant — the binary runs on any x86_64 Linux base. All deps
# are pure-Rust (tokio/axum/zip/serde/uuid/rand), so no C toolchain is needed
# beyond musl-tools for the linker.
# ---------------------------------------------------------------------------
FROM rust:1-bookworm AS builder

RUN rustup target add x86_64-unknown-linux-musl \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml ./
# Copy Cargo.lock when present for reproducible builds; build works without it too.
COPY Cargo.lock* ./
COPY src ./src

RUN cargo build --release --target x86_64-unknown-linux-musl --locked \
    || cargo build --release --target x86_64-unknown-linux-musl

# ---------------------------------------------------------------------------
# Runtime: steamcmd/steamcmd already ships `steamcmd` on PATH. Because our
# binary is static, there is no glibc dependency to worry about here.
# ---------------------------------------------------------------------------
FROM steamcmd/steamcmd:latest AS runtime

ENV STEAMCMD_BIN=steamcmd \
    WORKSHOP_HELPER_BIND=0.0.0.0:8090 \
    WORKSHOP_DATA_DIR=/data

COPY --from=builder \
    /build/target/x86_64-unknown-linux-musl/release/calaworkshop-helper \
    /usr/local/bin/calaworkshop-helper

EXPOSE 8090
VOLUME /data

ENTRYPOINT ["/usr/local/bin/calaworkshop-helper"]
