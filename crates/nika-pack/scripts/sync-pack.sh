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

# ATOMIC by construction. Three copy-list blind spots are documented
# below, and a fourth (`examples/showcase/`, nuked by spec 72fa40b) left
# the vendored pack GUTTED mid-run on 2026-08-13: `rm -rf` had already
# fired when the dead path failed the copy. Staging then swapping means a
# failed sync leaves the pack exactly as it was — the blind spots stay
# blind, but they stop being destructive.
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/nika-pack.XXXXXX")"
trap 'rm -rf "${STAGE}"' EXIT
DEST_FINAL="${DEST}"
DEST="${STAGE}/pack"
mkdir -p "${DEST}/schemas" "${DEST}/examples" "${DEST}/templates" "${DEST}/spec" "${DEST}/stdlib"

cp "${SPEC}/VERSION" "${SPEC}/canon.yaml" "${SPEC}/QUICKSTART.md" "${DEST}/"
# ALL published schemas, not a named one: the single-file form of this line
# silently DELETED law.schema.json + registries.schema.json on re-sync
# (the same rm-rf + copy-list blind spot the coverage-matrix comment below
# documents — third instance of the class, so the line becomes a glob)
cp "${SPEC}"/schemas/*.json "${DEST}/schemas/"
cp "${SPEC}/examples/manifest.yaml" "${SPEC}"/examples/*.nika.yaml "${DEST}/examples/"
# …and the prose that ships WITH them (CONVENTIONS · README-jobs ·
# README). The sixth instance of the blind spot, found while fixing
# the fifth: the line above takes `*.nika.yaml` only, so a re-sync
# quietly dropped two files the pack had carried since its first
# vendoring. Nothing embeds them and no test names them — which is
# exactly why they went unnoticed.
cp "${SPEC}"/examples/*.md "${DEST}/examples/"
# The manifest lists `examples/snippets/**` and the jobs read
# `examples/fixtures/**`; neither was ever in the copy list, so a
# re-sync dropped 8 manifest entries and every fixture — the FIFTH
# instance of the copy-list blind spot documented above, and the one
# `pack_integrity` catches (58 manifest entries vs 50 embedded).
# Whole subtrees, so a new file joins without a new line here.
cp -R "${SPEC}/examples/snippets" "${SPEC}/examples/fixtures" "${DEST}/examples/"
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

rm -rf "${DEST_FINAL}"
mv "${DEST}" "${DEST_FINAL}"
echo "pack synced from ${SPEC} → ${DEST_FINAL} (version $(cat "${DEST_FINAL}/VERSION"))"
