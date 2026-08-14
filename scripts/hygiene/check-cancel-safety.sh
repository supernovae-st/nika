#!/usr/bin/env bash
# check-cancel-safety — vector 30 (Batch I.b)
#
# Every `async fn` in the L0.5 kernel surface must carry a
# `// CANCEL SAFETY:` or `/// CANCEL SAFETY:` annotation, either in the
# immediately-preceding doc block (10-line window) or on the trait that
# declares it.
#
# SCOPE IS DERIVED, never typed. This read `crates/nika-kernel/src` when
# the kernel was one crate. The 2026-06-10 split moved its 94 async fn
# into sibling crates and left that path holding ZERO of them, so the
# vector has guarded nothing at all since — while reporting OK on every
# run. The layer metadata in Cargo.toml already says which crates are
# L0.5; asking it means the next split moves this vector by itself.
#
# Kernel traits are the cross-cutting effect surface — Tokio task cancellation
# can fire at any `.await`. Documenting drop-safety + partial-work policy per
# async method makes the ISP contract explicit and prevents silent corruption
# when a caller drops the future mid-flight.
#
# Exempt sentinel: `// CANCEL SAFETY: not applicable (<reason>)` — still
# required, just names the reason for N/A.
#
# `#[cfg(test)]` modules are skipped (in-file mocks are verified by the
# mock-crate review swarm, not by this hygiene vector). The same intent now
# needs saying out loud: when the kernel was one crate those mocks were
# `#[cfg(test)]` items and this skip covered them. The split promoted them
# to a crate of their own, where they are ordinary items, so `*-mock`
# crates are excluded by name and the exclusion is PRINTED — a scope this
# vector drops in silence would read as a scope it cleared.
#
# Exit codes: 0 = green, 2 = red (missing annotations listed).

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# The L0.5 set, straight from the layer metadata that declares it.
l05_crates() {
  awk '
    /^\[workspace\.metadata\.diamond\]/ { in_section = 1; next }
    /^\[/ && in_section { in_section = 0 }
    in_section && /^layers\./ {
      gsub(/^layers\./, "")
      gsub(/ /, "")
      gsub(/"/, "")
      print
    }
  ' Cargo.toml | grep '=L0\.5$' | cut -d= -f1
}

TARGETS=()
SKIPPED=()
while IFS= read -r crate; do
  [ -z "$crate" ] && continue
  case "$crate" in
    *-mock)
      SKIPPED+=("$crate")
      continue
      ;;
  esac
  [ -d "crates/$crate/src" ] && TARGETS+=("crates/$crate/src")
done < <(l05_crates)

# Fail CLOSED on an empty harvest. An empty scope is the exact shape this
# vector spent months in, and it looked identical to a clean one.
if [ "${#TARGETS[@]}" -eq 0 ]; then
  echo "check-cancel-safety: no L0.5 crate sources found — refusing to report a verdict" >&2
  echo "  (expected layers.<crate> = \"L0.5\" rows in Cargo.toml with a crates/<crate>/src)" >&2
  exit 2
fi

# One scratch file per run, removed on the way out. This wrote to a fixed
# `/tmp/cancel-safety-missing.txt` with `>>`, and only the GREEN path
# removed it — so a red run left the file behind and the next run appended
# to it, reporting 1 finding, then 2, then 3 on an unchanged tree. On a
# machine running more than one checkout it was a shared file besides.
MISSING="$(mktemp)"
trap 'rm -f "$MISSING"' EXIT

# Walk every .rs file. For each `async fn` declaration, check the 10 lines
# above for a `CANCEL SAFETY:` marker in a doc comment. Skip lines inside
# `#[cfg(test)]` modules (conservative brace-depth tracking).
while IFS= read -r -d '' file; do
  awk -v file="$file" -v MISSING="$MISSING" '
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
        # A trait or impl opener does not end the search: a one-method
        # trait states its cancel contract once, on the trait, which is
        # the right place for it. AgentBackend does exactly that and was
        # reported missing — the scan broke on `pub trait AgentBackend`
        # three lines short of the marker.
        if (line ~ /^[[:space:]]*(pub[[:space:]]+)?(unsafe[[:space:]]+)?(trait|impl)[[:space:]]/) continue
        # Stop looking if we hit a non-comment non-attribute non-blank line.
        if (line !~ /^[[:space:]]*($|\/\/|\/\*|\*|#\[|#!\[)/) break
      }

      if (!found) {
        sub(/^[[:space:]]*/, "", $0)
        printf "%s:%d: %s\n", file, NR, $0 >> MISSING
      }
    }

    { lines[NR] = $0 }
  ' "$file"
done < <(find "${TARGETS[@]}" -name '*.rs' -print0)

scanned="$(printf '%s\n' "${TARGETS[@]}" | sed 's|^crates/||; s|/src$||' | tr '\n' ' ')"

if [ -s "$MISSING" ]; then
  count=$(wc -l <"$MISSING" | tr -d '[:space:]')
  echo "RED: $count async fn without CANCEL SAFETY: doc marker"
  head -20 "$MISSING" | sed 's|^|  |'
  if [ "$count" -gt 20 ]; then
    echo "  ... (truncated; $((count - 20)) more)"
  fi
  exit 2
fi

echo "OK: every async fn carries a CANCEL SAFETY: doc across ${#TARGETS[@]} L0.5 crate(s) · ${scanned% }"
if [ "${#SKIPPED[@]}" -gt 0 ]; then
  echo "    (mock crate(s) not scanned: ${SKIPPED[*]} — reviewed by the mock-crate swarm)"
fi
exit 0
