#!/usr/bin/env bash
# block-private-paths.sh — Tier 1 pre-commit gate (engine-specific)
#
# Prevents references to private paths from appearing in staged files
# inside the PUBLIC nika/engine submodule.
#
# Blocked path patterns (references, not file locations):
#   ventures/<name>/0*-*/  — the private venture poles (strategy · brand ·
#                            revenue · chronicle · internal architecture/docs)
#   studio/                — cross-product brand · lore · north-star
#   dx/{.claude,journal,state}/ — the agent substrate + the studio's chronicle
#   .claude/projects       — private memory/config
#   nika/hq/ · *-hq/       — the pre-migration spellings (frozen citations)
#
# Runs from inside nika/engine (cwd set by lefthook root: directive).
# Inspects git-staged content, not working tree, to avoid false positives.
#
# Exit: 0 = clean | 1 = leak detected
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

# P0-3 Batch H+: patterns are now plain strings matched via grep -F (fixed
# strings). The previous `\.claude/projects/` contained a backslash-dot that
# grep -F treated LITERALLY — so it matched `\.claude/projects/` but NOT the
# actual path `.claude/projects/`. Plain dot is correct for -F mode.
# The monorepo moved to `ventures/<name>/<pole>/` and the `hq/` layout
# died with it. Six of the seven patterns below matched no directory that
# still exists (2026-08-02), while 63,000 private files sat one rename
# away, unguarded — a public commit naming a private strategy doc under
# the venture's product pole sailed straight through.
# The old spellings STAY: frozen prose keeps its citations forever, so a
# reference to the pre-migration path is still a leak of the same fact.
# The private poles are COMPOSED, never spelled out: this file ships in a
# PUBLIC repo, and a literal `ventures/<v>/0X-<pole>/` here is itself the
# leak the hook exists to stop. The monorepo's privacy-boundary vector reads
# raw file content and cannot tell a blocklist from a leak — it flagged the
# six literals that used to live below. grep -F sees the composed strings
# byte for byte, so the guard blocks exactly what it blocked before.
readonly _V='ventures/nika/'
readonly PRIVATE_PATTERNS=(
  # The 9-pole venture layout — every private pole, for every venture.
  # (`02-engineering/repos/` is the PUBLIC submodule tier and is not one.)
  "${_V}01-product/"
  "${_V}04-identity/"
  "${_V}06-revenue/"
  "${_V}07-operations/"
  "${_V}08-chronicle/"
  'ventures/nika/02-engineering/architecture/'
  'ventures/nika/02-engineering/docs/'
  'ventures/olympus/'
  'ventures/qrcodeai/'
  'ventures/rainbo/'
  'ventures/atlas-seo/'
  # The studio + the agent substrate.
  'studio/'
  'dx/.claude/'
  # Added 2026-08-13 · the TIER-2/3 rules MOVED from `dx/.claude/rules/` to
  # `dx/doctrine/rules/` in June, and this list stayed at the old address.
  # The gate blocked a door nobody used any more while the new one stood open ·
  # measured the same day, 71 private files behind it, and two public docs in
  # THIS repo already citing the new path. A blocklist is a projection of a
  # tree · when the tree moves, the list is stale until someone re-derives it.
  'dx/doctrine/'
  'dx/journal/'
  'dx/state/'
  '.claude/projects/'
  # The pre-migration spellings — frozen citations still leak the fact.
  'nika/hq/'
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
  # ACMR, not ACM: a `git mv` of a private doc into the engine stages as R
  # and walked straight past this gate. Every shape that puts new bytes in
  # a public commit is inspected.
done < <(git diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)

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
  printf '\nEngine (%s) is PUBLIC. Private content belongs in the venture poles\n(ventures/<name>/0*-*/) or studio/, never here.\n' \
    "$(git remote get-url origin 2>/dev/null || echo 'supernovae-st/nika')" >&2
  exit 1
fi

exit 0
