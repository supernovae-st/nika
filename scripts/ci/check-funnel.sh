#!/usr/bin/env bash
# The release gate, played BEFORE the tag. `scripts/ci/funnel-e2e.sh` is the
# stranger's first path against a built binary and the gate release.yml
# runs on every staged tarball — nothing uploads if it fails. Until
# 2026-08-19 it ran ONLY at tag time, so a tree could turn red for weeks
# and nobody saw it: v0.109.0 was tagged with three fixtures still spelled
# in the previous envelope (`nika: v1` + `workflow:`), the funnel refused
# them at parse (NIKA-PARSE-005), and the release never published (v0.106.0
# died the same way on 2026-07-27). Hygiene vector 50 now greps both gate
# scripts for the dead envelope forms — the SPELLING; this leg is the
# BEHAVIOR: it builds the same binary the release builds (`--features
# local-infer` · the sovereign lane the funnel §8 asserts) in the dev
# profile and plays the whole funnel on every push — wording needles, exit
# codes, the sovereign lane, the mcp wire — so the gate the release trusts
# is a gate that has already run, whatever moved. The funnel's consent leg
# RUNS a `permits:` + `exec:` workflow, which the 0.109 sandbox policy
# refuses unconfined (NIKA-1710 · what killed v0.109.1's Linux builders):
# the CI job installs bubblewrap before this script, exactly as the
# release builders now do. Feature set = release.yml (local-infer,access-harness).
set -euo pipefail

if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

# Same feature set as release.yml:99. A funnel that builds a thinner
# binary than the tarball is the 2026-08-08 ACCESS lie (E2E receipt on
# a local-only shape). metal stays off (tombstone in wiring.yaml).
cargo build -p nika-cli --bin nika-cli --features local-infer,access-harness
bin="${CARGO_TARGET_DIR:-target}/debug/nika-cli"
# Both tag-time gates, in the order release.yml plays them: the stranger's
# first path, then the operator's trust path (resume · reproduce · chain
# verify · tamper/drop · OTLP export) — v0.106.0 died on the battery the
# way v0.109.0 died on the funnel, at the tag, unseen before it.
bash scripts/ci/funnel-e2e.sh "$bin"
exec bash scripts/test/trust-battery.sh "$bin"
