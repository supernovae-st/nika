# syntax=docker/dockerfile:1.6
# =============================================================================
# Nika CLI — Static Multi-Platform Docker Image (cargo-chef pattern)
# =============================================================================
# Build: docker buildx build --platform linux/amd64,linux/arm64 -t nika .
# Run:   docker run --rm ghcr.io/supernovae-st/nika:latest --version
#
# This Dockerfile uses cargo-chef for 10x faster rebuilds by caching deps.
# Final image is scratch-based (~5MB) with static musl binary.
#
# Note: Docker builds use --no-default-features --features docker which
# disables OS keychain support. Use environment variables for secrets:
#   -e ANTHROPIC_API_KEY="sk-ant-..."
# =============================================================================

# Stage 1: Chef base with cargo-chef installed
# Using Rust 1.86+ for ratatui-crossterm, ratatui-widgets, tui-tree-widget compatibility
# cargo-chef pinned to 0.1.71 because 0.1.77+ requires Rust 1.88+ (cargo-platform@0.3.2)
FROM --platform=$BUILDPLATFORM rust:1.86-alpine AS chef
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig perl make
RUN cargo install cargo-chef@0.1.71 --locked
WORKDIR /app

# Stage 2: Planner - analyze dependencies
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - cache dependencies, then build
FROM chef AS builder

# Build arguments for cross-compilation
ARG TARGETARCH
ARG VERSION=dev

# Determine target triple based on architecture
RUN case "${TARGETARCH}" in \
        amd64) echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
        arm64) echo "aarch64-unknown-linux-musl" > /tmp/target ;; \
        *) echo "unsupported architecture: ${TARGETARCH}" && exit 1 ;; \
    esac

# Install cross-compilation toolchain for ARM64
RUN if [ "${TARGETARCH}" = "arm64" ]; then \
        apk add --no-cache gcc-aarch64-none-elf; \
    fi

# Add musl target
RUN rustup target add $(cat /tmp/target)

# Copy recipe AND Cargo.lock to respect locked versions during cook
COPY --from=planner /app/recipe.json recipe.json
COPY --from=planner /app/Cargo.lock Cargo.lock
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --target $(cat /tmp/target) --recipe-path recipe.json \
        --no-default-features --features docker

# Copy source and build application
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    RUSTFLAGS="-C target-feature=+crt-static" \
    cargo build --release --target $(cat /tmp/target) --bin nika \
        --no-default-features --features docker \
    && mkdir -p /release \
    && cp target/$(cat /tmp/target)/release/nika /release/

# Stage 4: Minimal runtime (scratch = ~5MB)
FROM scratch AS runtime

# OCI Labels (https://github.com/opencontainers/image-spec/blob/main/annotations.md)
LABEL org.opencontainers.image.source="https://github.com/supernovae-st/nika"
LABEL org.opencontainers.image.description="Nika — Semantic YAML Workflow Engine for AI"
LABEL org.opencontainers.image.licenses="AGPL-3.0-or-later"
LABEL org.opencontainers.image.vendor="SuperNovae Studio"
LABEL org.opencontainers.image.title="nika"
LABEL org.opencontainers.image.url="https://supernovae.studio"
LABEL org.opencontainers.image.documentation="https://github.com/supernovae-st/nika#readme"

ARG VERSION=dev
LABEL org.opencontainers.image.version="${VERSION}"

# Copy static binary
COPY --from=builder /release/nika /nika

# Working directory for mounted workflows
WORKDIR /workspace

ENTRYPOINT ["/nika"]
CMD ["--help"]
