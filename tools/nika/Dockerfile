# syntax=docker/dockerfile:1.6
# =============================================================================
# Nika CLI — Multi-Stage Docker Image
# =============================================================================
#
# Stage 1 (builder): Rust build environment for testing
#   docker compose -f docker-compose.test.yml --profile unit up
#
# Stage 2 (runtime): Minimal scratch image with pre-built static binaries (~5MB)
#   docker run --rm ghcr.io/supernovae-st/nika:latest --version
#
# Build context for runtime (created by CI):
#   docker-context/
#   ├── amd64/nika    (x86_64-unknown-linux-musl binary)
#   └── arm64/nika    (aarch64-unknown-linux-musl binary)
#
# Usage:
#   docker run --rm -v $(pwd):/workspace ghcr.io/supernovae-st/nika:latest run workflow.nika.yaml
#
# Note: Runtime uses --no-default-features (no OS keychain). Use env vars:
#   -e ANTHROPIC_API_KEY="sk-ant-..."
# =============================================================================

# ─── Stage 1: Builder (for docker-compose test services) ───────────────────
FROM rust:1.86-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-nextest for parallel test execution
RUN curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/cargo/bin

WORKDIR /app
COPY . .

# Pre-build dependencies (cached layer)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --no-default-features --lib 2>/dev/null || true

# ─── Stage 2: Runtime (minimal scratch image) ─────────────────────────────
FROM scratch AS runtime

# OCI Labels
LABEL org.opencontainers.image.source="https://github.com/supernovae-st/nika"
LABEL org.opencontainers.image.description="Nika — Semantic YAML Workflow Engine for AI"
LABEL org.opencontainers.image.licenses="AGPL-3.0-or-later"
LABEL org.opencontainers.image.vendor="SuperNovae Studio"
LABEL org.opencontainers.image.title="nika"
LABEL org.opencontainers.image.url="https://supernovae.studio"
LABEL org.opencontainers.image.documentation="https://github.com/supernovae-st/nika#readme"

ARG TARGETARCH
ARG VERSION=0.75.0

LABEL org.opencontainers.image.version="${VERSION}"

# Copy pre-built static binary for target architecture
COPY ${TARGETARCH}/nika /nika

WORKDIR /workspace

ENTRYPOINT ["/nika"]
CMD ["--help"]
