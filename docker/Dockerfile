# syntax=docker/dockerfile:1
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# The official nika image (#442) — the RELEASE binary, checksum-verified,
# multi-arch (linux/amd64 + linux/arm64). Built and pushed to
# `ghcr.io/supernovae-st/nika` by the release workflow; standalone-buildable
# too (the same ARG-download pattern the nika-agents kit proved in CI):
#
#   docker build -f docker/Dockerfile --build-arg NIKA_VERSION=0.99.0 -t nika .
#   docker run --rm nika --version
#   docker run -i --rm nika mcp          # the read-only MCP oracle (stdio)
#
# Base: debian-slim, NOT distroless — the `exec:` verb and shell-form
# commands need a shell + coreutils; the oracle lane never uses them but
# the image entrypoint is the WHOLE binary (`nika`), per the issue.
FROM debian:12-slim AS fetch
ARG NIKA_VERSION
ARG TARGETARCH
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN set -eu; \
    test -n "${NIKA_VERSION}" || { echo "NIKA_VERSION build-arg is required" >&2; exit 1; }; \
    case "${TARGETARCH}" in \
      amd64) A=x64 ;; \
      arm64) A=arm64 ;; \
      *) echo "unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    T="nika-linux-${A}-${NIKA_VERSION}.tar.gz"; \
    cd /tmp; \
    curl -fsSL -O "https://github.com/supernovae-st/nika/releases/download/v${NIKA_VERSION}/${T}"; \
    curl -fsSL -O "https://github.com/supernovae-st/nika/releases/download/v${NIKA_VERSION}/SHA256SUMS"; \
    grep " ${T}\$" SHA256SUMS | sha256sum -c -; \
    tar -xzf "${T}" -C /usr/local/bin nika; \
    chmod +x /usr/local/bin/nika

FROM debian:12-slim
# ca-certificates: rustls needs CA roots for `nika:fetch` + provider calls.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 10001 nika
COPY --from=fetch /usr/local/bin/nika /usr/local/bin/nika
USER nika
WORKDIR /home/nika
LABEL org.opencontainers.image.source="https://github.com/supernovae-st/nika" \
      org.opencontainers.image.description="nika — Intent as Code · the workflow language for AI. One file, 4 verbs, one binary. Entrypoint is the whole CLI; `docker run -i --rm <img> mcp` serves the read-only MCP oracle." \
      org.opencontainers.image.licenses="AGPL-3.0-or-later"
ENTRYPOINT ["nika"]
CMD ["--help"]
