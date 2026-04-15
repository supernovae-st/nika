#!/usr/bin/env bash
# block-private-paths.sh — Tier 1 pre-commit gate (engine-specific)
#
# Prevents references to private paths from appearing in staged files
# inside the PUBLIC nika/engine submodule.
#
# Blocked path patterns (references, not file locations):
#   nika/hq/         — private strategy/brand content
#   .claude/projects — private memory/config
#   supernovae-hq/   — monorepo root references (engine must be self-contained)
#   studio-spn/      — cross-product private content
#   jungo/hq/        — other product private content
#
# Runs from inside nika/engine (cwd set by lefthook root: directive).
# Inspects git-staged content, not working tree, to avoid false positives.
#
# Exit: 0 = clean | 1 = leak detected
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

readonly PRIVATE_PATTERNS=(
  'nika/hq/'
  '\.claude/projects/'
  'studio-spn/'
  'jungo/hq/'
  'novanet/hq/'
  'qrcodeai/hq/'
  'supernovae-hq/'
)

# Get staged files in the engine context (exclude deleted files).
# Self-exclusion — these directories legitimately enumerate the very patterns
# we guard against, which would cause self-referential false-positives:
#   scripts/hooks/     the guarding code + its PRIVATE_PATTERNS constants
#   scripts/hygiene/   vectors that spot-check privacy (patterns.conf, tests)
#   scripts/test/      red-team fixtures that describe blocked scenarios
#   scripts/ci/        CI ratchets that may count/grep private-path refs
# See P1-7 Batch H+ (per BATCH_H_PLUS_DECISIONS.md Q-PRIVATE).
STAGED=()
while IFS= read -r _f; do
  case "$_f" in
    '' | scripts/hooks/* | scripts/hygiene/* | scripts/test/* | scripts/ci/*) ;;
    *) STAGED+=("$_f") ;;
  esac
done < <(git diff --cached --name-only --diff-filter=ACM 2>/dev/null || true)

if ((${#STAGED[@]} == 0)); then
  exit 0
fi

FOUND=()
for pattern in "${PRIVATE_PATTERNS[@]}"; do
  while IFS= read -r match; do
    [[ -n "$match" ]] && FOUND+=("$match")
  done < <(
    git diff --cached -U0 -- "${STAGED[@]}" 2>/dev/null \
      | grep '^+' \
      | grep -v '^+++' \
      | grep -F "$pattern" \
      | head -5 \
      || true
  )
done

if ((${#FOUND[@]} > 0)); then
  printf '\n[block-private-paths] BLOCKED — private path reference in staged engine files:\n' >&2
  printf '  %s\n' "${FOUND[@]}" >&2
  printf '\nEngine (%s) is PUBLIC. Private content belongs in nika/hq/.\n' \
    "$(git remote get-url origin 2>/dev/null || echo 'supernovae-st/nika')" >&2
  exit 1
fi

exit 0
