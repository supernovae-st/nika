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

if [ ! -d "$TARGET" ]; then
  echo "check-owned-strings: $TARGET not found — skipping"
  exit 0
fi

MISSING_FILE=/tmp/owned-strings-missing.txt
: >"$MISSING_FILE"

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
    echo "  ... (truncated; full list at $MISSING_FILE)"
  fi
  exit 2
fi

rm -f "$MISSING_FILE"
echo "OK: nika-catalog public API uses static-str or String exclusively"
exit 0
