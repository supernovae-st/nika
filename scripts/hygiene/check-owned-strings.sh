#!/usr/bin/env bash
# check-owned-strings — vector 31 (Batch I.b)
#
# nika-catalog public surface must use either static-str or owned-String
# (never ambiguously-borrowed string slices).
#
# ALLOWED in pub fields / pub fn return types:
#   - static-lifetime str references (zero-alloc, Send+Sync, chosen per
#     ADR-008 codegen pragma)
#   - String (heap-owned)
#
# FORBIDDEN in pub fields / pub fn return types:
#   - plain borrow-str (ambiguous lifetime, storage unknown)
#   - named non-static borrow-str (thread-unsafe across async boundaries,
#     see ADR-023 rationale)
#
# ALLOWED everywhere: borrow-str in function parameters (borrowing input
# is the idiomatic borrow-dont-clone pattern).
#
# Exempt marker: OWNED-STRINGS-EXEMPT with a reason on the line above
# the pub item (e.g., internal modules re-exported for completeness).
#
# Exit codes: 0 = green, 2 = red.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
TARGET="$REPO_ROOT/crates/nika-catalog/src"

# Fail CLOSED on a missing subject. This printed "not found — skipping" and
# returned 0, so a rename of nika-catalog would have turned the vector green
# forever without a word. That is not hypothetical here: the same shape,
# `TARGET=crates/<one-crate>/src` plus `[ ! -d ] && exit 0`, is exactly how
# check-cancel-safety and check-kernel-no-spawn came to guard nothing when
# nika-kernel was split in June — one guarded 0 of 94 async fn, the other
# 2% of its layer, both reporting OK. A vector that cannot find its subject
# has not cleared it.
if [ ! -d "$TARGET" ]; then
  echo "RED: $TARGET not found — refusing to report a verdict" >&2
  echo "  (the crate moved or was renamed; re-aim TARGET, do not skip)" >&2
  exit 2
fi

# One scratch file per run. The fixed `/tmp/owned-strings-missing.txt` was
# truncated at start so it was at least idempotent, but it is shared across
# every checkout on the machine — two concurrent runs read each other's
# findings. mktemp costs nothing and removes the race.
MISSING_FILE="$(mktemp)"
trap 'rm -f "$MISSING_FILE"' EXIT

# The awk program: walk each line, flag pub-field and pub-fn-return
# patterns that use non-static string borrows. We keep the previous line
# for exempt-marker lookup. Writing to MISSING_FILE via -v passthrough.
# shellcheck disable=SC2016  # awk script; variables expand inside awk, not bash
AWK_PROGRAM='
  {
    prev_line = (NR > 1 ? lines[NR - 1] : "")
    lines[NR] = $0

    stripped = $0
    sub(/^[[:space:]]+/, "", stripped)
    if (stripped ~ /^(\/\/|\/\*|\*|$)/) next

    if (prev_line ~ /OWNED-STRINGS-EXEMPT:/) next

    # (1) pub struct field with ampersand-str that is NOT static.
    # Match: "pub <ident>: &<anything-without-brace/paren/semi>str"
    if (match(stripped, /^pub [a-z_][a-zA-Z0-9_]*:[[:space:]]*&[^,;({\[]*str/)) {
      m = substr(stripped, RSTART, RLENGTH)
      if (index(m, "static") == 0) {
        printf "%s:%d: %s\n", file, NR, stripped >> out
        next
      }
    }

    # (2) pub fn return ampersand-str (non-static).
    if (stripped ~ /^[[:space:]]*pub (fn |const fn |async fn |unsafe fn )/ &&
        match(stripped, /->[[:space:]]*&[^,;({\[]*str/)) {
      m = substr(stripped, RSTART, RLENGTH)
      if (index(m, "static") == 0) {
        printf "%s:%d: %s\n", file, NR, stripped >> out
        next
      }
    }
  }
'

while IFS= read -r -d '' file; do
  awk -v file="$file" -v out="$MISSING_FILE" "$AWK_PROGRAM" "$file"
done < <(find "$TARGET" -name '*.rs' -print0)

if [ -s "$MISSING_FILE" ]; then
  count=$(wc -l <"$MISSING_FILE" | tr -d '[:space:]')
  echo "RED: $count pub non-static str in nika-catalog public API"
  head -20 "$MISSING_FILE" | sed 's|^|  |'
  if [ "$count" -gt 20 ]; then
    # Says how many more, not where to look: the scratch file is per-run and
    # the trap removes it, so pointing a reader at it would send them to a
    # path that is already gone.
    echo "  ... ($((count - 20)) more)"
  fi
  exit 2
fi

echo "OK: nika-catalog public API uses static-str or String exclusively"
exit 0
