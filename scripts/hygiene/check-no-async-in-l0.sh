#!/usr/bin/env bash
# Vector 22: Forbid async/await/runtime imports in L0 crates.
#
# L0 = pure value layer. Per Diamond invariants (ADR-006, ADR-024, Q1 lock
# 2026-04-16), L0 crates MUST have zero async surface — no `async fn`,
# no `.await`, no tokio/futures/async-trait imports. Async lives at L0.5
# (kernel traits) and L1+ (effect impls).
#
# Why: keeps L0 reusable everywhere (sync contexts, no-std targets, blocking
# tools, ML inference loops, embedded). Once a crate "leaks" async, every
# downstream consumer pays for the runtime.
#
# Reads L0 membership from [workspace.metadata.diamond.layers.*] in
# Cargo.toml (the same source-of-truth used by check-layering.sh).
#
# Exit codes:
#   0 -- GREEN (no violations)
#   1 -- YELLOW (currently unused; reserved for future "advisory" patterns)
#   2 -- RED (at least one violation)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Patterns forbidden in L0 src/. Each pattern is a Rust-level construct that
# either declares async surface or imports an async runtime/dep.
FORBIDDEN_PATTERNS=(
  'async fn'         # async function declaration
  '\.await'          # await suffix
  '\btokio::'        # tokio runtime path
  '\btokio_'         # tokio_test, tokio_util, etc.
  '\bfutures::'      # futures crate (futures-core also banned in L0; allowed L0.5+)
  '\bfutures_core::' # futures-core (trait-only, but still async surface)
  '\bfutures_util::' # futures-util
  '\basync_trait\b'  # async-trait macro
  '#\[tokio::'       # #[tokio::main], #[tokio::test]
  '#\[async_trait'   # #[async_trait]
  'block_on'         # block_on(...) executors
  'JoinHandle'       # tokio::task::JoinHandle
)

# Extract L0 crate names from workspace metadata.
# Format in Cargo.toml: layers.nika-types = "L0"
mapfile -t L0_CRATES < <(grep -E '^layers\.[a-z0-9-]+ = "L0"$' Cargo.toml \
  | sed -E 's/^layers\.([a-z0-9-]+) = "L0"$/\1/')

if [ "${#L0_CRATES[@]}" -eq 0 ]; then
  echo "WARN: no L0 crates found in [workspace.metadata.diamond] — vector skipped"
  exit 1
fi

violations=0
violation_log=""

for crate in "${L0_CRATES[@]}"; do
  src_dir="crates/$crate/src"
  if [ ! -d "$src_dir" ]; then
    continue
  fi

  for pat in "${FORBIDDEN_PATTERNS[@]}"; do
    # grep with -E for ERE, exclude any kind of line comment (//, ///, //!).
    # The grep -rn output is "filename:line:content", so we filter where the
    # CONTENT (after :NN:) starts with //. Block comments (/* */) are not
    # filtered — they're rare in Rust src and explicit flagging is preferred.
    # #[cfg(test)] async is intentionally NOT auto-allowed: reviewer must confirm.
    matches=$(grep -rEn "$pat" "$src_dir" 2>/dev/null \
      | grep -vE ':[0-9]+:[[:space:]]*//' \
      || true)
    if [ -n "$matches" ]; then
      violations=$((violations + 1))
      violation_log+="\n  L0 crate '$crate' has forbidden pattern '$pat':\n"
      violation_log+="$(echo "$matches" | head -5 | sed 's/^/    /')\n"
    fi
  done
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d async-pattern violation(s) in L0 crates:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: L0 must stay pure. Move async-touching code to L0.5 (kernel"
  echo "traits) or L1+ (effect crates). See ADR-006, ADR-024, and the"
  echo "Q1 architecture decision (2026-04-16) in docs/architecture/"
  echo "l0-l05-architecture-decisions.md."
  exit 2
fi

echo "OK: no async patterns in ${#L0_CRATES[@]} L0 crate(s) (${L0_CRATES[*]})"
exit 0
