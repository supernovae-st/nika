#!/usr/bin/env bash
# Vector 27: Forbid `Box<dyn Error>` / `Box<dyn std::error::Error>` in src/.
#
# Nika uses the `NikaErrorCode` trait + `NikaError` wrapper (ADR-007,
# nika-error Option C+ design). `Box<dyn Error>` is an escape hatch that
# loses the dual wire ("NIKA-140") + typed (num, category, slug) error
# codes and breaks `NikaResult` propagation.
#
# Banned everywhere in crates/*/src/ — except:
#   - nika-error's own impls (core::error::Error source chain)
#   - dev tests (#[cfg(test)] blocks — reviewer discretion)
#
# Exit codes:
#   0 -- GREEN (no violations)
#   2 -- RED (at least one violation)
#
# See: ADR-007 forward-compat invariants, nika-error traits.rs.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# The pattern matches Box<dyn Error ...> regardless of path prefix and generics.
# ERE: literal Box<dyn then optional path segments then `Error`.
FORBIDDEN_RE='Box<dyn[[:space:]]+([a-zA-Z_:]+::)*Error([[:space:]]*[+>]|$)'

violations=0
violation_log=""

# Scan every admitted crate except nika-error (which defines the alternative
# NikaErrorCode trait and legitimately uses core::error::Error for `source()`).
for crate_dir in crates/*/src; do
  crate_name=$(basename "$(dirname "$crate_dir")")
  case "$crate_name" in
    nika-error) continue ;; # owns the alternative pattern
  esac

  matches=$(grep -rEn "$FORBIDDEN_RE" "$crate_dir" 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*//' \
    || true)
  if [ -n "$matches" ]; then
    count=$(echo "$matches" | wc -l | tr -d ' ')
    violations=$((violations + count))
    violation_log+="\n  $crate_name:\n"
    violation_log+="$(echo "$matches" | head -5 | sed 's/^/    /')\n"
  fi
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d Box<dyn Error> occurrence(s) in src/:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: Nika errors flow through the NikaErrorCode trait + NikaError"
  echo "wrapper (ADR-007). Replace Box<dyn Error> with a crate-specific"
  echo "#[non_exhaustive] enum implementing NikaErrorCode + thiserror +"
  echo "miette::Diagnostic. See crates/nika-error/src/traits.rs for the"
  echo "pattern, and crates/nika-kernel/src/errors.rs for per-crate code"
  echo "ranges (100-139 FileIo, 140-189 Http, etc.)."
  exit 2
fi

echo "OK: no Box<dyn Error> usage in admitted crate src/"
exit 0
