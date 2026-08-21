#!/usr/bin/env bash
# Vector 41: canon-stale-terms — live docs must state the CURRENT canon
# only. Dead names, superseded plans, and wrong counts must not reappear.
#
# Why: doc drift is a recurring failure class — a stale name in a live
# doc (a dead crate name, a banned subsystem name, a superseded version
# pin, a wrong count) silently re-teaches the wrong canon to every
# reader and every agent session. Discipline does not scale; this gate
# does. History belongs in git, ADRs and CHANGELOG — never restated as
# "before X, now Y" narrative in live docs.
#
# Scope: live reader-facing docs + agent rules. Dated records (ADRs,
# CHANGELOG, dated audits/censuses/proposals, legacy-mapping tracking)
# are FROZEN HISTORY and exempt — they are supposed to contain the old
# terms.
#
# Per-line opt-out: a line containing `stale-ok` is skipped (use for
# rejected-name registries that must NAME the rejected term).
#
# Exit codes:
#   0 -- GREEN (no stale term in any live doc)
#   2 -- RED  (stale term found — fix the doc, never the vector)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Live docs (reader-facing + agent-facing enforcement).
LIVE_DOCS=(
  "README.md"
  "ROADMAP.md"
  "DIAMOND.md"
  "MANIFESTO.md"
  "CLAUDE.md"
  ".claude/CLAUDE.md"
)
# Glob groups appended below (rules, architecture, crate-specs).

# Frozen / dated docs exempt by design (history surfaces).
EXEMPT_PATTERNS=(
  "docs/adr/"
  "CHANGELOG.md"
  "docs/golden-commits.md"
  "docs/constellation-reconciliation"
  "docs/architecture/BLUEPRINT_2036.md"
  "docs/architecture/kernel-split-census"
  "docs/architecture/l0-l05-architecture-decisions.md"
  ".claude/tracking/"
)

# Stale classes: dead names · superseded pins · wrong counts.
# Keep this list in sync with the current canon (nika-invariants +
# crate-layer-registry + the Connectome spec).
STALE_TERMS=(
  'Cortex'
  'Egghead'
  'nika-memory-'
  'nika-provider-rig'
  'nika-provider-native'
  'nika-provider-mock'
  'nika-process\b'
  'nika-http-client'
  'fjall'
  '[Oo]xigraph[@ ]0\.5\.6'
  '\b9 satellites'
  '\b14 satellites'
  '\b5 verbs'
  'five verbs'
  # The fourteen-key envelope (dead 2026-08-12 · ADR-113 · nine keys · the
  # identity rides ON `nika:`). A live doc that still opens a workflow with
  # `nika: v1` or a top-level `workflow:` teaches a parse refusal
  # (NIKA-PARSE-003/005 on 0.109). Named-to-refuse lines carry `stale-ok`.
  'nika: v1'
  '^workflow:'
)

is_exempt() {
  local f="$1"
  for pat in "${EXEMPT_PATTERNS[@]}"; do
    [[ "$f" == *"$pat"* ]] && return 0
  done
  return 1
}

# Build the scan list.
SCAN_FILES=()
for f in "${LIVE_DOCS[@]}"; do
  [[ -f "$f" ]] && SCAN_FILES+=("$f")
done
while IFS= read -r f; do
  is_exempt "$f" || SCAN_FILES+=("$f")
done < <(find .claude/rules docs/architecture docs/crate-specs -name '*.md' -type f 2>/dev/null | sort)

violations=0
for f in "${SCAN_FILES[@]}"; do
  for term in "${STALE_TERMS[@]}"; do
    while IFS= read -r hit; do
      [[ "$hit" == *"stale-ok"* ]] && continue
      echo "  STALE [$term] $f:${hit%%:*}"
      violations=$((violations + 1))
    done < <(grep -nE "$term" "$f" 2>/dev/null || true)
  done
done

if [[ $violations -gt 0 ]]; then
  echo "RED: $violations stale-canon term(s) in live docs — state the current canon only (history lives in git/ADRs/CHANGELOG)"
  exit 2
fi

echo "OK: 0 stale-canon terms across ${#SCAN_FILES[@]} live docs (${#STALE_TERMS[@]} dead classes gated)"
exit 0
