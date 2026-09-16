# syntax=docker/dockerfile:1

# Two ways in, one image out.
#
# `BINARY_SOURCE=source` (the default) compiles the server here, with cargo-chef
# keeping the dependency layer cached between builds. It is what
# `docker compose build` and the benchmark compose do on a laptop, and it
# needs nothing but this directory.
#
# `BINARY_SOURCE=prebuilt` takes a binary the build context already carries at
# `prebuilt/<TARGETPLATFORM>/rustrak` (`prebuilt/linux/amd64/rustrak`, ...).
# It is what docker-publish.yml uses: the binaries are compiled by cargo on
# native runners, where the compiler cache survives from one release to the
# next, and this file only assembles the image. Every stage below the switch
# is a COPY, or runs on the build platform, so a multi-platform image needs
# no QEMU.
#
# Both paths end in the same runtime stage, and that is the point of keeping
# them in one file: the image an operator builds locally is laid out exactly
# like the one on Docker Hub.

ARG FEATURES=sqlite
ARG BINARY_SOURCE=source

# ── source: compile here ──────────────────────────────────────────────────────
FROM rust:1.94-slim-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-chef --locked
WORKDIR /app

# Compute the dependency recipe from the manifest alone, so the cook layer
# below is invalidated by Cargo.toml and Cargo.lock and by nothing else.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS source
ARG FEATURES
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --no-default-features --features ${FEATURES} --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --no-default-features --features ${FEATURES} \
    && mkdir -p /out && cp target/release/rustrak /out/rustrak

# ── prebuilt: take the binary from the context ────────────────────────────────
# Artifacts do not keep file modes, so the executable bit is set here rather
# than trusted.
FROM scratch AS prebuilt
ARG TARGETPLATFORM
COPY --chmod=755 prebuilt/${TARGETPLATFORM}/rustrak /out/rustrak

# ── the switch ────────────────────────────────────────────────────────────────
# Whichever stage the build arg names is the one the runtime stage reads from.
FROM ${BINARY_SOURCE} AS binary

# ── layout: directories and the dashboard, none of it architecture-specific ──
# The runtime image has no shell, so the directories it needs and their owner
# are prepared here and copied in. Pinned to the build platform: mkdir and
# chown do not care which architecture the image is for, and running them
# natively is what keeps QEMU out of a multi-platform build.
#
# `static/` is the compiled dashboard, written into this directory by
# `scripts/bundle-dashboard.sh` when the JavaScript side was built first, and
# absent when it was not. An empty directory is exactly what the server reads
# as "no dashboard, serve the API only", so its absence is not an error.
FROM --platform=$BUILDPLATFORM busybox:stable AS layout
COPY . /src
RUN mkdir -p /data/sourcemaps /rootfs/app/static \
    && if [ -d /src/static ]; then cp -R /src/static/. /rootfs/app/static/; fi \
    && chown -R 65532:65532 /data /rootfs/app

# ── runtime: minimal distroless ───────────────────────────────────────────────
# `--chown` on the directories even though `layout` already owns them: COPY
# copies a directory's *contents*, and creates the destination directory itself
# as root. Without it `/data` would be root's, and the SQLite default of
# `/data/rustrak.db` could not be created by `nonroot`.
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=binary /out/rustrak /app/rustrak
COPY --chown=65532:65532 --from=layout /rootfs/app/static /app/static
COPY --chown=65532:65532 --from=layout /data /data
VOLUME /data
VOLUME /data/sourcemaps
EXPOSE 8080
USER nonroot
CMD ["/app/rustrak"]
