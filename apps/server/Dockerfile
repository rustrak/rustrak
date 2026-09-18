ARG FEATURES=sqlite

# ── Stage 1: the toolchain, with cargo-chef already installed ────────────────
# The prebuilt cargo-chef image is the rust:<version>-slim-bookworm image plus
# the chef binary. Installing chef with `cargo install` instead compiled it
# from source on every release build, two minutes before a line of Rustrak
# was compiled; the same two minutes on each of the four release images. Bump
# the Rust version here together with `RUST_TOOLCHAIN` in ci.yml.
FROM lukemathwalker/cargo-chef:0.1.77-rust-1.94-slim-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# ── Stage 2: compute dependency recipe from manifest ─────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: compile dependencies (Docker-cached layer) ──────────────────────
# The layer cache is what makes a second local `docker build` fast: a change
# under src/ invalidates the build below, never the cook above. A release
# build in CI starts from nothing, and there the cook is simply the first half
# of the compile.
FROM chef AS builder
ARG FEATURES
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --no-default-features --features ${FEATURES} --recipe-path recipe.json

# Build the application binary.
#
# The anonymous telemetry key is a BuildKit secret rather than a build arg so
# it never lands in the image metadata. A build without it (a local build, a
# fork) compiles a binary whose telemetry is off and says so at startup.
COPY . .
RUN --mount=type=secret,id=rustrak_telemetry_key \
    RUSTRAK_TELEMETRY_KEY="$(cat /run/secrets/rustrak_telemetry_key 2>/dev/null || true)" \
    cargo build --release --locked --no-default-features --features ${FEATURES}

# Create /data directories for SQLite storage and source map storage
RUN mkdir -p /data /data/sourcemaps && chown -R 65532:65532 /data

# The compiled dashboard, if the build context carries one.
#
# `apps/dashboard` builds to `apps/server/static` via
# `scripts/bundle-dashboard.sh`, which is what `pnpm build --filter=@rustrak/server`
# runs — so `static/` is already inside this context when the JavaScript side
# was built first, and absent when it was not. `mkdir -p` makes the next stage's
# COPY unconditional either way, and an empty directory is exactly what the
# server reads as "no dashboard, serve the API only".
RUN mkdir -p /app/static && chown -R 65532:65532 /app/static

# ── Stage 4: minimal distroless runtime ──────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder /app/target/release/rustrak /app/rustrak
COPY --chown=65532:65532 --from=builder /app/static /app/static
COPY --chown=65532:65532 --from=builder /data /data
VOLUME /data
VOLUME /data/sourcemaps
EXPOSE 8080
USER nonroot
CMD ["/app/rustrak"]
