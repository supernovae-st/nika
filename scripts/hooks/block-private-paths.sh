#!/usr/bin/env bash
# block-private-paths.sh — Tier 1 pre-commit gate (engine-specific)
#
# Prevents references to private paths from appearing in staged files
# inside the PUBLIC nika/engine submodule.
#
# Blocked path patterns (references, not file locations):
#   ventures/<name>/…      — every venture tree, EXCEPT the one public tier
#                            (ventures/nika/02-engineering/repos/, where this
#                            repo itself lives)
#   studio/                — cross-product brand · lore · north-star
#   dx/                    — the agent substrate + the studio's chronicle,
#                            the WHOLE tree, not a list of its children
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

# Patterns are EXTENDED REGEXES (grep -E), and each one is a SHAPE rather
# than an inventory. Three times now this list has been an inventory, and
# three times the tree moved out from under it:
#
#   · 2026-08-02 · the monorepo moved to `ventures/<name>/<pole>/` and six
#     of seven patterns named a directory that no longer existed, while
#     63,000 private files sat one rename away, unguarded.
#   · 2026-08-13 · the TIER-2/3 rules moved to `dx/doctrine/` in June; the
#     list stayed at `dx/.claude/`, guarding a door nobody used any more.
#   · 2026-08-14 · measured again. `dx/` had NINE children plus three
#     hidden ones; the list named four, so five private trees had never
#     been guarded at all. The venture list named FIVE ventures while
#     SEVEN existed, leaving two whole ventures unguarded. (Counts only —
#     naming the unguarded children here would be the leak itself.)
#
# So neither family is enumerated any more. `dx/` and `studio/` are named
# at the ROOT — the only spelling a rename inside them cannot invalidate —
# and the venture tree is matched by its shape below, with the single
# public tier carved out. That also says LESS in a public file: this hook
# ships in a PUBLIC repo, and an inventory of private subdirectory names is
# itself the leak the hook exists to stop.
#
# Each pattern is anchored at a path boundary. Unanchored `studio/` matched
# `lmstudio/` and `~/.cache/lm-studio/` — real, public LM Studio provider
# paths — so the gate also blocked legitimate work. `[^a-zA-Z0-9._-]` keeps
# `https://supernovae.studio/` out too, while still matching `/studio/`.
# The patterns themselves live in ONE file, shared with the full-tree twin
# (`scripts/hygiene/check-private-leaks.sh`, vector 14). Measured 2026-08-15:
# they had drifted to TEN patterns here against ONE there, so the sweep that
# looks everywhere searched for almost nothing and a private path slept in a
# user-facing error hint of this PUBLIC engine. Two lists drift; one cannot.
# Resolved from THIS script's location, never from `git rev-parse
# --show-toplevel`: the hook's own test harness runs it inside throwaway
# repos where that root has no scripts/ tree, and the source then failed —
# turning the gate RED on everything, including the cases it must pass.
# A script knows where it lives; it does not know whose repo it is in.
# shellcheck source=scripts/lib/private-patterns.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/private-patterns.sh"

readonly PRIVATE_PATTERNS=("${NIKA_PRIVATE_PATTERNS[@]}")
readonly VENTURE_SHAPE="$NIKA_VENTURE_SHAPE"
readonly VENTURE_PUBLIC="$NIKA_VENTURE_PUBLIC"

# Get staged files in the engine context (exclude deleted files).
# Self-exclusion — these directories legitimately enumerate the very patterns
# we guard against, which would cause self-referential false-positives:
#   scripts/lib/       the shared pattern list itself (added 2026-08-15 · the
#                      gate blocked the very commit that introduced it, which
#                      is the correct reflex on a directory it had never been
#                      told about — a new home for the constants needs the
#                      same carve-out the old one had, not an obfuscation)
#   scripts/hooks/     the guarding code + its PRIVATE_PATTERNS constants
#   scripts/hygiene/   vectors that spot-check privacy (patterns.conf, tests)
#   scripts/test/      red-team fixtures that describe blocked scenarios
#   scripts/ci/        CI ratchets that may count/grep private-path refs
# See P1-7 Batch H+ (per BATCH_H_PLUS_DECISIONS.md Q-PRIVATE).
STAGED=()
while IFS= read -r _f; do
  case "$_f" in
    '' | scripts/lib/* | scripts/hooks/* | scripts/hygiene/* | scripts/test/* | scripts/ci/*) ;;
    *) STAGED+=("$_f") ;;
  esac
  # ACMR, not ACM: a `git mv` of a private doc into the engine stages as R
  # and walked straight past this gate. Every shape that puts new bytes in
  # a public commit is inspected.
done < <(git diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)

if ((${#STAGED[@]} == 0)); then
  exit 0
fi

# Every added line, gathered once. (`^+`, minus the `+++` file headers.)
ADDED="$(
  git diff --cached -U0 -- "${STAGED[@]}" 2>/dev/null \
    | grep '^+' \
    | grep -v '^+++' \
    || true
)"

FOUND=()
for pattern in "${PRIVATE_PATTERNS[@]}"; do
  while IFS= read -r match; do
    [[ -n "$match" ]] && FOUND+=("$match")
  done < <(printf '%s\n' "$ADDED" | grep -E "$pattern" | head -5 || true)
done

# The venture tree is matched per OCCURRENCE, not per line. A line that
# names the public repos tier AND a private pole has to be caught for the
# private half; a line-level `grep -v` would have dropped the whole line on
# account of the innocent one.
while IFS= read -r match; do
  [[ -n "$match" ]] && FOUND+=("$match")
done < <(
  printf '%s\n' "$ADDED" \
    | grep -oE "$VENTURE_SHAPE" \
    | grep -vE "$VENTURE_PUBLIC" \
    | head -5 \
    || true
)

if ((${#FOUND[@]} > 0)); then
  printf '\n[block-private-paths] BLOCKED — private path reference in staged engine files:\n' >&2
  printf '  %s\n' "${FOUND[@]}" >&2
  printf '\nEngine (%s) is PUBLIC. Private content belongs in the venture poles\n(ventures/<name>/0*-*/) or studio/, never here.\n' \
    "$(git remote get-url origin 2>/dev/null || echo 'supernovae-st/nika')" >&2
  exit 1
fi

exit 0
