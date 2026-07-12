#!/usr/bin/env bash
# sync-pack.sh · vendor the spec pack snapshot into crates/nika-pack/pack/
#
# The pack = the versioned language artifacts the binary embeds (per the
# spec README §The examples pack). Source of truth = the nika-spec repo;
# this script copies the exact surface and nothing else. Run it when
# bumping the embedded pack to a new spec version, then commit the diff.
#
# Usage · scripts/sync-pack.sh [path-to-nika-spec-checkout]
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ -n "${CRATE_DIR}" && -f "${CRATE_DIR}/Cargo.toml" ]] || {
  echo "CRATE_DIR resolution failed" >&2
  exit 2
}
SPEC="${1:-${CRATE_DIR}/../../../spec}"
DEST="${CRATE_DIR}/pack"

[[ -f "${SPEC}/canon.yaml" ]] || {
  echo "spec checkout not found at ${SPEC}" >&2
  exit 2
}

rm -rf "${DEST}"
mkdir -p "${DEST}/schemas" "${DEST}/examples/showcase" "${DEST}/templates" "${DEST}/spec" "${DEST}/stdlib"

cp "${SPEC}/VERSION" "${SPEC}/canon.yaml" "${SPEC}/QUICKSTART.md" "${DEST}/"
cp "${SPEC}/schemas/workflow.schema.json" "${DEST}/schemas/"
cp "${SPEC}/examples/manifest.yaml" "${SPEC}"/examples/*.nika.yaml "${DEST}/examples/"
cp "${SPEC}"/examples/showcase/*.nika.yaml "${DEST}/examples/showcase/"
cp "${SPEC}"/templates/*.nika.yaml "${DEST}/templates/"
cp "${SPEC}"/spec/*.md "${DEST}/spec/"
cp "${SPEC}"/stdlib/*.md "${DEST}/stdlib/"
# The conformance coverage matrix rides at pack root (added 2026-07-10 with
# the egress-to-outputs arc) — without this line every re-sync silently
# DELETED it (the rm -rf + copy-list pattern's blind spot).
cp "${SPEC}/conformance/coverage-matrix.tsv" "${DEST}/"
# The shared visual vocabulary (spec design/tokens.yaml · the design SSOT ·
# #464): verb colors + severity + brand core. graph's mermaid classDefs
# derive from it at build time. Same blind-spot law as the line above.
cp "${SPEC}/design/tokens.yaml" "${DEST}/design-tokens.yaml"
# The motion vocabulary (spec design/motion.yaml · one family per verb ·
# css keyframes + terminal frames + reduced-motion law) — the same
# derivation the site's tiles and the CLI's spinners share.
cp "${SPEC}/design/motion.yaml" "${DEST}/design-motion.yaml"

echo "pack synced from ${SPEC} → ${DEST} (version $(cat "${DEST}/VERSION"))"
