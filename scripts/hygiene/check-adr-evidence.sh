#!/usr/bin/env bash
# Vector 20: Grep-verify file paths mentioned in Evidence sections.
# Only checks paths that look like workspace-relative (tools/, crates/, docs/, scripts/, src/).
#
# Exit codes:
#   0 -- GREEN (all evidence paths exist or no paths to check)
#   1 -- YELLOW (some paths missing -- evidence may be stale)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

ADR_DIR="docs/adr"
missing=0
checked=0

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  fname="$(basename "$f")"

  # Extract the Evidence section (between ## Evidence and next ## heading)
  evidence="$(awk '/^## Evidence/{found=1; next} found && /^## /{exit} found{print}' "$f")"
  [ -z "$evidence" ] && continue

  # A path marked as a FORMER location is provenance, not evidence: an ADR
  # that records a split says "crates/nika-proof/src/lib.rs — ex
  # crates/nika-runtime/src/proof/mod.rs", and the second path is expected to
  # be gone — that is the whole point of writing it (it tells a reader where
  # to look in git). Flagging it made this vector permanently YELLOW on a
  # correct ADR (measured 2026-07-30 on ADR-108), which is the noise that
  # teaches people to ignore a checker. Drop those citations before scanning,
  # so what remains is the set of paths the ADR claims exist TODAY.
  # shellcheck disable=SC2016  # literal backticks in the sed program — no expansion wanted
  live="$(printf '%s\n' "$evidence" \
    | sed -E 's/(ex|ex-|was|formerly|pre-split|previously)[[:space:]]+`[^`]*`//g')"

  # Find backtick-enclosed paths that look like file paths
  # shellcheck disable=SC2016
  paths="$(printf '%s\n' "$live" \
    | grep -oE '`(tools|crates|docs|scripts|src)/[^`[:space:]:]+' \
    | sed 's/^`//' || true)"

  for p in $paths; do
    [ -z "$p" ] && continue
    checked=$((checked + 1))
    if [ ! -e "$p" ]; then
      echo "MISSING: $fname: path \`$p\` not found" >&2
      missing=$((missing + 1))
    fi
  done
done

if [ "$missing" -gt 0 ]; then
  echo "$missing stale evidence path(s) ($checked checked)"
  exit 1
fi

echo "OK ($checked evidence paths verified)"
exit 0
