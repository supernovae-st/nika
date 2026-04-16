#!/usr/bin/env bash
# check-cancel-safety — vector 30 (Batch I.b)
#
# Every `async fn` inside crates/nika-kernel/src/** must have a
# `// CANCEL SAFETY:` or `/// CANCEL SAFETY:` annotation in the
# immediately-preceding doc block (10-line window).
#
# Kernel traits are the cross-cutting effect surface — Tokio task cancellation
# can fire at any `.await`. Documenting drop-safety + partial-work policy per
# async method makes the ISP contract explicit and prevents silent corruption
# when a caller drops the future mid-flight.
#
# Exempt sentinel: `// CANCEL SAFETY: not applicable (<reason>)` — still
# required, just names the reason for N/A.
#
# `#[cfg(test)]` modules inside kernel/src/** are skipped (in-file mocks are
# verified by the mock-crate review swarm, not by this hygiene vector).
#
# Exit codes: 0 = green, 2 = red (missing annotations listed).

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
TARGET="$REPO_ROOT/crates/nika-kernel/src"

if [ ! -d "$TARGET" ]; then
  echo "check-cancel-safety: $TARGET not found — skipping"
  exit 0
fi

# Walk every .rs file. For each `async fn` declaration, check the 10 lines
# above for a `CANCEL SAFETY:` marker in a doc comment. Skip lines inside
# `#[cfg(test)]` modules (conservative brace-depth tracking).
while IFS= read -r -d '' file; do
  awk -v file="$file" '
    BEGIN { in_cfg_test = 0; cfg_depth = 0; depth = 0 }

    # Enter/exit cfg(test) gated module
    /#\[cfg\(test\)\]/ { cfg_test_pending = 1; next }
    /^[[:space:]]*mod[[:space:]]+[a-zA-Z_]+[[:space:]]*\{/ {
      if (cfg_test_pending) { in_cfg_test = 1; cfg_depth = depth + 1 }
      cfg_test_pending = 0
    }

    {
      # Track brace depth lightly.
      for (i = 1; i <= length($0); i++) {
        c = substr($0, i, 1)
        if (c == "{") depth++
        else if (c == "}") {
          depth--
          if (in_cfg_test && depth < cfg_depth) { in_cfg_test = 0 }
        }
      }
    }

    /async fn / {
      if (in_cfg_test) next

      # Skip lines that are themselves comments (prose mentioning async fn).
      stripped = $0
      sub(/^[[:space:]]+/, "", stripped)
      if (stripped ~ /^(\/\/|\/\*|\*)/) next

      # Look backwards up to 10 lines for "CANCEL SAFETY:" in a comment.
      found = 0
      start = (NR > 10) ? NR - 10 : 1
      for (i = NR - 1; i >= start; i--) {
        line = lines[i]
        if (line ~ /CANCEL SAFETY:/) { found = 1; break }
        # Stop looking if we hit a non-comment non-attribute non-blank line.
        if (line !~ /^[[:space:]]*($|\/\/|\/\*|\*|#\[|#!\[)/) break
      }

      if (!found) {
        sub(/^[[:space:]]*/, "", $0)
        printf "%s:%d: %s\n", file, NR, $0 >> "/tmp/cancel-safety-missing.txt"
      }
    }

    { lines[NR] = $0 }
  ' "$file"
done < <(find "$TARGET" -name '*.rs' -print0)

if [ -s /tmp/cancel-safety-missing.txt ]; then
  count=$(wc -l </tmp/cancel-safety-missing.txt | tr -d '[:space:]')
  echo "RED: $count async fn without CANCEL SAFETY: doc marker"
  head -20 /tmp/cancel-safety-missing.txt | sed 's|^|  |'
  if [ "$count" -gt 20 ]; then
    echo "  ... (truncated; full list at /tmp/cancel-safety-missing.txt)"
  fi
  rm -f /tmp/cancel-safety-missing.txt.kept
  exit 2
fi

rm -f /tmp/cancel-safety-missing.txt
echo "OK: every async fn in nika-kernel/src/** has CANCEL SAFETY: doc"
exit 0
