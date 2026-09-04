# Nika — official image (ghcr.io/supernovae-st/nika · linux/amd64 + linux/arm64)
#
# Built on the release train (.github/workflows/release.yml `docker` job) from
# the SAME tarball binaries the GitHub release ships — never a rebuild. The
# base mirrors the proven MCP-lane image (nika-plugins integrations/mcp/
# Dockerfile · in-container oracle probed in CI): ubuntu:24.04 matches the
# glibc of the release build runners (the linux-arm64 lane builds on
# ubuntu-24.04-arm — a smaller-glibc base would strand that binary). TLS
# roots are compiled into the binary (rustls + webpki-roots), so no
# ca-certificates layer is needed for `infer:`/`fetch` HTTPS.
#
# Lanes this image targets: `nika check` · `nika run` · `nika mcp`. `exec:`
# tasks get the base's shell + coreutils and nothing more — extend the image
# (`FROM ghcr.io/supernovae-st/nika` + apt-get) when a workflow shells out
# to heavier tools.
#
# CI stages binaries at dist/linux/<TARGETARCH>/nika. Local build = the same
# staging (docker arch: amd64|arm64 · release tarball arch: x64|arm64):
#   v=0.118.2; a=arm64                    # apple-silicon/arm hosts (amd64: a=x64)
#   curl -fsSLO "https://github.com/supernovae-st/nika/releases/download/v${v}/nika-linux-${a}-${v}.tar.gz"
#   mkdir -p dist/linux/arm64 && tar -xzf "nika-linux-${a}-${v}.tar.gz" -C dist/linux/arm64 nika
#   docker build -t nika .
#
# Run:  docker run --rm -v "$PWD:/work" -w /work ghcr.io/supernovae-st/nika check flow.nika.yaml
# MCP:  docker run -i --rm ghcr.io/supernovae-st/nika mcp

FROM ubuntu:24.04@sha256:4fbb8e6a8395de5a7550b33509421a2bafbc0aab6c06ba2cef9ebffbc7092d90
ARG TARGETARCH
LABEL org.opencontainers.image.source="https://github.com/supernovae-st/nika" \
      org.opencontainers.image.description="Nika — the workflow language for AI. One YAML file: check it, run it, trace it." \
      org.opencontainers.image.licenses="AGPL-3.0-or-later"
COPY dist/linux/${TARGETARCH}/nika /usr/local/bin/nika
ENTRYPOINT ["nika"]
