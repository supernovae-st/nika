#!/usr/bin/env bash
# bench-refonte.sh · run the W0 refonte baseline harness and snapshot it.
# Both benches are own-harness (`harness = false`) and print one RB{...}
# JSON line per measurement plus SLOPE lines; this runner captures them,
# adds process peak-RSS (/usr/bin/time -l · external, no unsafe in-tree),
# and writes docs/perf/refonte-baseline-<date>.md next to the raw jsonl.
# Compare two snapshots with: diff <(grep '^{' A.md) <(grep '^{' B.md)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
DATE="$(date -u +%Y-%m-%d)"
OUT_MD="docs/perf/refonte-baseline-${DATE}.md"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "→ nika-schema baseline (parse/analyze/check · 5 topologies × 4 sizes)"
/usr/bin/time -l cargo bench --bench refonte_baseline --package nika-schema \
  >"$TMP/schema.out" 2>"$TMP/schema.time" || { cat "$TMP/schema.out"; exit 1; }
echo "→ nika-lsp baseline (hover/completion/semanticDocument)"
/usr/bin/time -l cargo bench --bench refonte_lsp_baseline --package nika-lsp \
  >"$TMP/lsp.out" 2>"$TMP/lsp.time" || { cat "$TMP/lsp.out"; exit 1; }

rss() { grep "maximum resident set size" "$1" | awk '{printf "%.0f MiB", $1/1048576}'; }
{
  echo "# W0 refonte performance baseline — ${DATE}"
  echo
  echo "- machine: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)"
  echo "- rustc: $(rustc --version | awk '{print $2}') · profile: bench (optimized)"
  echo "- engine: $(git rev-parse --short HEAD)"
  echo "- peak RSS: schema-bench $(rss "$TMP/schema.time") · lsp-bench $(rss "$TMP/lsp.time")"
  echo
  echo "## Slopes (2k→10k · budget ×6.25)"
  grep '^SLOPE\|^slope violations' "$TMP/schema.out" "$TMP/lsp.out" | sed 's/^[^:]*://'
  echo
  echo '## Raw measurements (p50/p95 µs)'
  echo '```jsonl'
  grep '^RB{' "$TMP/schema.out" "$TMP/lsp.out" | sed 's/^[^R]*RB//'
  echo '```'
} >"$OUT_MD"
echo "snapshot → $OUT_MD"
grep -c '^RB{' "$TMP/schema.out" "$TMP/lsp.out" | tr '\n' ' '
echo
grep 'slope violations' "$TMP/schema.out" "$TMP/lsp.out"
