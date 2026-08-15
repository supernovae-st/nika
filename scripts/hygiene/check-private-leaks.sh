#!/usr/bin/env bash
# Vector 14: no private monorepo paths referenced in this PUBLIC tree.
#
# 2026-08-15 · this vector guarded ONE hardcoded pattern
# (`.claude/projects/-Users-thibaut`) while the pre-commit hook guarded TEN
# plus the venture shape. The sweep that looks EVERYWHERE searched for almost
# nothing — so 18 occurrences of a private path, one of them in a user-facing
# error hint of the public engine, slept here until a merge happened to put
# them in someone's staged diff. Both now read the SAME list.
#
# FLOW vs STOCK — the two gates are not redundant, they are complementary:
#   the hook judges the staged diff, so it only ever sees the commit it runs
#   on. Anything committed on a base older than the hook escapes it forever,
#   and a squash-merge runs no local hook at all. This vector is the twin
#   that judges the STOCK. One without the other guards half the repo.
set -uo pipefail

# Resolved from THIS script's location, not from the git root: a script
# knows where it lives, it does not know whose repo it is in.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/private-patterns.sh
. "$HERE/../lib/private-patterns.sh"

# Exemptions, each with its reason — an unexplained exemption is a hole.
#   .claude/CLAUDE.md · .claude/rules/  the local-dev onboarding surface; a
#       user path is the expected context there, not a leak.
#   scripts/lib · hooks · hygiene · test · ci   these ENUMERATE the patterns
#       (or test them), so they match themselves by construction.
#   docs/adr/   frozen decision records. The monorepo's path-resolution law
#       keeps frozen citations forever — rewriting an ADR to satisfy a gate
#       would falsify the record it exists to preserve. New private-path
#       citations are still refused at commit time by the hook, which does
#       NOT exempt this directory: the stock is grandfathered, the flow is not.
EXCLUDES=(
  ':!.gitignore' ':!*.lock'
  ':!.claude/CLAUDE.md' ':!.claude/rules/*.md'
  ':!scripts/lib/*' ':!scripts/hooks/*' ':!scripts/hygiene/*'
  ':!scripts/test/*' ':!scripts/ci/*'
  ':!docs/adr/*'
)

ALTERNATION="$(nika_private_alternation)"

leaks="$(git grep -l -E "$ALTERNATION" -- "${EXCLUDES[@]}" 2>/dev/null || true)"

# The venture tree, matched by shape, with the one public tier carved out.
#
# Per OCCURRENCE, not per file. The first cut of this block asked
# `git grep -l` for the SHAPE and reported every file it named — which
# includes every file that legitimately cites the PUBLIC tier, since the
# public tier is itself of that shape. It reported a clean handoff doc as a
# leak. A gate that names a file it never proved guilty is worse than no
# gate: it teaches the reader to disbelieve it.
#
# So: ask each candidate file for its occurrences, drop the public ones, and
# keep the file only if something private survives.
while IFS= read -r f; do
  [ -n "$f" ] || continue
  if git grep -h -oE "$NIKA_VENTURE_SHAPE" -- "$f" 2>/dev/null \
    | grep -qvE "$NIKA_VENTURE_PUBLIC"; then
    leaks="$(printf '%s\n%s' "$leaks" "$f")"
  fi
done < <(git grep -l -E "$NIKA_VENTURE_SHAPE" -- "${EXCLUDES[@]}" 2>/dev/null || true)
leaks="$(printf '%s' "$leaks" | grep -v '^$' | sort -u || true)"

if [ -n "$leaks" ]; then
  echo "private path leak in:"
  printf '%s\n' "$leaks" | sed 's/^/  /'
  exit 2
fi
echo "OK (no private paths in tracked code · $(printf '%s' "${#NIKA_PRIVATE_PATTERNS[@]}") patterns + the venture shape)"
exit 0
