#!/usr/bin/env bash
# Ratchet: no unexplained `allow(dead_code)` in production src/.
# Dead code must be deleted, not hidden. See deletion-first.md.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. An `allow(dead_code)`
# inside `#[cfg(test)]` is harmless and intentionally permitted.
#
# TWO spellings hide dead code, and the ratchet sees both (2026-08-01: the
# grep only knew the literal one, so `#[cfg_attr(not(test),
# allow(dead_code))]` walked past it — same effect, different letters):
#
#   #[allow(dead_code)]                        <- hidden everywhere
#   #[cfg_attr(not(test), allow(dead_code))]   <- hidden in prod, live in tests
#
# The second is a legitimate PARK — an item whose consumer has not shipped
# yet, kept honest by its tests. A park is allowed only when it says WHY,
# in the form the compiler itself understands (stable since Rust 1.81):
#
#   #[cfg_attr(not(test), allow(dead_code, reason = "the answer half of
#   the multi-root door; no surface carries a root argument yet"))]
#
# Anything without a `reason` fails. Every park that has one is PRINTED —
# a budget nobody can read is a budget nobody keeps.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

# An attribute is a BLOCK, not a line — `#[cfg_attr(not(test),
# allow(dead_code, reason = "…"))]` wraps across four lines the moment
# rustfmt touches it. A line-wise grep reads that block as absent and
# prints a reassuring zero, which is the failure mode this ratchet
# exists to prevent. So: accumulate from `#[` until the brackets
# balance, and judge the whole block. Reports the line the block opens.
scan_blocks() {
  awk '
    # Blank string contents so a bracket inside a reason ("an array [x]")
    # cannot unbalance the depth counter and swallow the file.
    function code_only(s,   out, i, n, c) {
      out = ""
      n = length(s)
      for (i = 1; i <= n; i++) {
        c = substr(s, i, 1)
        if (in_str) {
          if (c == "\\") { i++ } else if (c == "\"") { in_str = 0 }
          continue
        }
        if (c == "\"") { in_str = 1; continue }
        out = out c
      }
      return out
    }
    BEGIN { in_str = 0 }
    # Outer `#[…]` AND inner `#![…]` — an inner attribute silences the
    # WHOLE module, so missing it was the larger hole of the two.
    depth == 0 && index($0, "#[") == 0 && index($0, "#![") == 0 { code_only($0); next }
    {
      if (depth == 0) { start = FNR; block = "" }
      block = block " " $0
      code = code_only($0)
      n = gsub(/\[/, "[", code)
      m = gsub(/\]/, "]", code)
      depth += n - m
      if (depth > 0) { next }
      depth = 0
      if (block ~ /allow[[:space:]]*\(/ && block ~ /dead_code/) {
        gsub(/^[[:space:]]+/, "", block)
        verdict = (block ~ /reason[[:space:]]*=/) ? "PARKED" : "BARE"
        printf "%s\t%d\t%s\n", verdict, start, substr(block, 1, 100)
      }
    }
  '
}

total=0
found_any=0
parked=0
parked_lines=''
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(strip_test_items "$f" | scan_blocks || true)
  [ -z "$hits" ] && continue
  while IFS=$'\t' read -r verdict line body; do
    [ -z "$verdict" ] && continue
    if [ "$verdict" = "PARKED" ]; then
      parked=$((parked + 1))
      parked_lines="${parked_lines}  ${f}:${line}  ${body}"$'\n'
    else
      found_any=1
      printf '%s:%s  %s\n' "$f" "$line" "$body"
      total=$((total + 1))
    fi
  done <<EOF
$hits
EOF
done < <(rs_prod_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d unexplained allow(dead_code) in production src/\n' "$total" >&2
  printf '      delete the code, or park it with allow(dead_code, reason = "...")\n' >&2
  exit 1
fi

if [ "$parked" -gt 0 ]; then
  printf 'OK  zero unexplained allow(dead_code) — %d justified park(s):\n' "$parked"
  printf '%s' "$parked_lines"
else
  echo "OK  zero allow(dead_code) in production src/"
fi
