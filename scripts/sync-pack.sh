#!/usr/bin/env bash
# sync-pack.sh — vendor the spec pack into crates/nika-pack/pack/, the
# ONE deterministic lane (lib.rs has promised this script since the
# crate's birth; until 2026-07-13 it lived in operator scratch — a
# phantom the SSOT sweep materialized).
#
#   bash scripts/sync-pack.sh <path-to-nika-spec-checkout>
#
# The mapping IS the contract — additions to the pack are deliberate
# edits HERE, never ad-hoc copies:
#   VERSION · QUICKSTART.md · canon.yaml          ← spec root
#   coverage-matrix.tsv                            ← conformance/
#   design-tokens.yaml · design-motion.yaml        ← design/{tokens,motion}.yaml
#   spec/ · schemas/ · examples/ · templates/      ← whole dirs (adds+deletes flow)
#   stdlib/*.md ONLY                               ← the two YAML SSOTs
#     (verb-starters · authoring-shapes) deliberately stay OUT: they
#     project through their own generator lanes, never the pack.
#
# Byte-copy, then the integrity tests judge (a drifted pack fails
# `cargo test -p nika-pack`, not a user).
set -euo pipefail
SPEC="${1:?usage: sync-pack.sh <nika-spec-checkout>}"
cd "$(dirname "$0")/.."
PACK=crates/nika-pack/pack
[ -f "$SPEC/VERSION" ] || {
  echo "sync-pack: $SPEC is not a nika-spec checkout" >&2
  exit 2
}

cp "$SPEC/VERSION" "$PACK/VERSION"
cp "$SPEC/QUICKSTART.md" "$PACK/QUICKSTART.md"
cp "$SPEC/canon.yaml" "$PACK/canon.yaml"
cp "$SPEC/conformance/coverage-matrix.tsv" "$PACK/coverage-matrix.tsv"
cp "$SPEC/design/tokens.yaml" "$PACK/design-tokens.yaml"
cp "$SPEC/design/motion.yaml" "$PACK/design-motion.yaml"

# The pack carries the exact source identity beside its bytes. The runtime
# build refuses a SPEC_PIN/pack marker mismatch, so a binary cannot silently
# combine two language-spec commits.
spec_sha="$(git -C "$SPEC" rev-parse HEAD)"
if [[ ! "$spec_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "sync-pack: source checkout did not resolve to a full commit SHA" >&2
  exit 2
fi
printf '%s\n' "$spec_sha" >"$PACK/SPEC_SHA"

# READMEs are repo navigation, not pack content — the vendored state
# has never carried them (verified against the #533 tree).
for d in spec schemas examples templates; do
  rsync -a --delete --exclude='README.md' "$SPEC/$d/" "$PACK/$d/"
done

rsync -a --delete --include='*.md' --exclude='*' "$SPEC/stdlib/" "$PACK/stdlib/"

echo "sync-pack: vendored from $(git -C "$SPEC" rev-parse --short HEAD 2>/dev/null || echo '?') — review the diff, the integrity tests judge"
