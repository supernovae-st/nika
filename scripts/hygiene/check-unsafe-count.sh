#!/usr/bin/env bash
# check-unsafe-count — vector 32 (Batch I.b, lightweight substitute for
# cargo-geiger-workspace which is hostile to virtual manifests)
#
# Counts `unsafe` token occurrences in admitted `crates/nika-*/src/**/*.rs`
# and compares to scripts/hygiene/baselines/unsafe-count.txt. RED if the
# current count exceeds baseline — forces a deliberate decision when
# someone introduces new `unsafe` and a SAFETY doc note.
#
# Excludes:
#   - `// SAFETY:` doc lines (they are the mitigation, not the risk)
#   - commented-out `// unsafe { }` blocks
#   - test files (#[cfg(test)] modules implicitly scoped by `src/` walk)
#
# Baseline regeneration:
#   grep -rn '\bunsafe\b' crates/*/src --include='*.rs' \
#     | grep -v '// SAFETY' | grep -v '^[^:]*:[[:space:]]*//' | wc -l \
#     > scripts/hygiene/baselines/unsafe-count.txt
#
# Rationale vs cargo-geiger: we already have `cargo audit` + `cargo deny`
# for dep-tree security. This ratchet focuses on OUR surface area: every
# new `unsafe` block in admitted crates is an explicit risk decision that
# should go through review (doc + ADR if non-trivial). cargo-geiger on a
# virtual-workspace needs a wrapper crate that compiles everything, which
# costs ~7 minutes cold and breaks on stale deps files — unacceptable for
# a hygiene vector that must run in <30s.
#
# Exit codes: 0 = green, 2 = red.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
BASELINE_FILE="$HERE/baselines/unsafe-count.txt"

if [ ! -f "$BASELINE_FILE" ]; then
  echo "check-unsafe-count: baseline file missing at $BASELINE_FILE"
  exit 2
fi

BASELINE=$(tr -d '[:space:]' <"$BASELINE_FILE")

# Count real `unsafe` tokens in admitted-crate src trees.
# Exclude: SAFETY doc annotations, lines that start with `//` (commented).
cd "$REPO_ROOT" || exit 2
# shellcheck disable=SC2126  # two-layer grep is clearer than grep -c here
current=$(grep -rn '\bunsafe\b' crates/*/src --include='*.rs' 2>/dev/null \
  | grep -v '// SAFETY' \
  | grep -v '^[^:]*:[0-9]*:[[:space:]]*//' \
  | wc -l \
  | tr -d '[:space:]')

if [ "$current" -gt "$BASELINE" ]; then
  echo "RED: unsafe count $current exceeds baseline $BASELINE"
  grep -rn '\bunsafe\b' crates/*/src --include='*.rs' \
    | grep -v '// SAFETY' \
    | grep -v '^[^:]*:[0-9]*:[[:space:]]*//' \
    | head -10 \
    | sed 's|^|  |'
  echo "  (regenerate baseline after review: see script header)"
  exit 2
fi

if [ "$current" -lt "$BASELINE" ]; then
  echo "OK: unsafe count $current (below baseline $BASELINE — ratchet down pending)"
else
  echo "OK: unsafe count $current matches baseline"
fi
exit 0
