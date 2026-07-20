#!/usr/bin/env bash
# render-notes.sh — the curated release body: What · Install · Verify · Provenance.
#
# The generated PR list (`gh release create --generate-notes`) is a good index
# and a bad front page: a reader landing on a release should see what shipped,
# how to install it, and how to PROVE the artifact they hold is the artifact
# CI built — before the merge log. This script renders that front page; the
# workflow passes it via `--notes-file` and keeps `--generate-notes`, so the
# full PR list still rides at the bottom (gh prepends provided notes to the
# generated ones — the index survives, the story leads).
#
# The What section reads CHANGELOG.md — the same section git-cliff maintains —
# so the release body and the changelog can never tell two stories. An empty
# or missing section degrades to the compare link, never to silence.
#
# Usage: render-notes.sh vX.Y.Z            (from the repo root)
# Stdout is the markdown body. No network, no mutation — pure projection.
set -euo pipefail

tag="${1:?usage: render-notes.sh vX.Y.Z}"
version="${tag#v}"
repo="${GITHUB_REPOSITORY:-supernovae-st/nika}"

# ── What — the CHANGELOG section between this version's header and the next `## [`
what="$(awk -v ver="$version" '
  index($0, "## [" ver "]") == 1 { grab = 1; next }
  grab && /^## \[/ { exit }
  grab { print }
' CHANGELOG.md)"
# strip leading blank lines; trailing ones are harmless in markdown
what="$(printf '%s\n' "$what" | sed -e '/[^[:space:]]/,$!d')"
if [ -z "${what//[[:space:]]/}" ]; then
  what="Curated notes pending for this tag — the full diff: https://github.com/${repo}/compare/$(git describe --tags --abbrev=0 "${tag}^" 2>/dev/null || echo "v0.90.0")..${tag}"
fi

cat <<EOF
## What

${what}

## Install

\`\`\`sh
brew install supernovae-st/tap/nika          # macOS · Linux
curl -LsSf https://nika.sh/install.sh | sh   # script install
docker run --rm ghcr.io/${repo}:${version} --version
\`\`\`

Tarballs below: macOS arm64 / x64 · Linux x64 / arm64, plus \`SHA256SUMS\`.

## Verify — three independent proofs

\`\`\`sh
# 1 · checksum — the bytes you hold are the bytes CI hashed
sha256sum -c SHA256SUMS --ignore-missing     # macOS: shasum -a 256 -c

# 2 · attestation — GitHub-signed build provenance for this exact artifact
gh attestation verify nika-<platform>-${version}.tar.gz --repo ${repo}

# 3 · SLSA provenance — the intoto asset, verifiable offline
slsa-verifier verify-artifact nika-<platform>-${version}.tar.gz \\
  --provenance-path multiple.intoto.jsonl \\
  --source-uri github.com/${repo} --source-tag ${tag}
\`\`\`

## Provenance

Built from tag \`${tag}\` by [\`release.yml\`](https://github.com/${repo}/blob/${tag}/.github/workflows/release.yml)
on GitHub-hosted runners. Provenance is published twice: GitHub's native
build attestation (proof 2) and the SLSA generator's \`multiple.intoto.jsonl\`
release asset (proof 3). The release itself is a claim on the
machine-verified timeline: https://nika.sh/timeline
EOF
