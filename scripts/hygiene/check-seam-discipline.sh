#!/usr/bin/env bash
# Vector: seam discipline — L1.5+ crates must reach the OS through the
# injected kernel seams, never std/dep defaults directly.
#
# The builtins deep-verify (2026-07-06) named the engine's DOMINANT bug
# class: a builtin's stated contract silently defeated or bypassed by the
# layer beneath it. Two of the three real bugs were seam bypasses — the
# fs seam's unconditional mkdir defeating `create_dirs:false`, and a
# dependency reading the system directly. A source sweep proved the class
# is bounded engine-wide (one intended exception: uuid's entropy). This
# vector is the ratchet that keeps it bounded: the NEXT unintended bypass
# fails at commit time, not by luck in a review.
#
# Scope: L1.5/L2/L3 crates (the ones that COMPOSE effects and must go
# through the injected seams). The L1 effect crates (nika-fs, nika-http,
# nika-clock, nika-blob, nika-exec-runner, nika-screen, ...) ARE the seams
# / effects and legitimately touch the OS — they are NOT scanned.
#
# Allowlist: a line carrying `// seam-bypass-ok: <reason>` is exempt —
# the one intended exception is nika:uuid (entropy/freshness IS the id, so
# it rides getrandom's CSPRNG directly · the inverse of tz which must be
# sovereign). New exemptions require the marker + a reason (a reviewer's
# deliberate sign-off, mirroring the `box-dyn-ok` convention).
#
# Reads layer membership from [workspace.metadata.diamond.layers.*] —
# the same source-of-truth as check-layering.sh / check-no-async-in-l0.sh.
#
# Exit codes:
#   0 -- GREEN (no unmarked bypass)
#   1 -- YELLOW (reserved / no crates found)
#   2 -- RED (at least one unmarked seam bypass)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Direct-OS constructs that MUST go through a seam at L1.5+. Time/entropy
# (ride ClockDyn / are the uuid effect), filesystem, and network.
FORBIDDEN_PATTERNS=(
  'SystemTime::now' # wall clock — ride the injected ClockDyn (INV-027)
  'Instant::now'    # monotonic clock — same
  '\bUtc::now\b'    # chrono wall clock
  '\bLocal::now\b'  # chrono local clock
  'now_v7'          # uuid v7 (clock+entropy) — allowlisted at the uuid effect
  'new_v4'          # uuid v4 (entropy) — allowlisted at the uuid effect
  'thread_rng'      # rand thread RNG
  'rand::random'    # rand convenience RNG
  '\bgetrandom\b'   # direct entropy syscall
  'std::fs::'       # filesystem — ride the FsDyn seam
  'std::net::'      # network — ride the HttpDyn seam
  '\breqwest::'     # http client — the nika-http seam owns reqwest
  '\bTcpStream\b'   # raw sockets
  '\bTcpListener\b'
)

# L1.5/L2/L3 crate names from workspace metadata.
mapfile -t SCANNED_CRATES < <(grep -E '^layers\.[a-z0-9-]+ = "(L1\.5|L2|L3)"$' Cargo.toml \
  | sed -E 's/^layers\.([a-z0-9-]+) = "(L1\.5|L2|L3)"$/\1/')

if [ "${#SCANNED_CRATES[@]}" -eq 0 ]; then
  echo "WARN: no L1.5/L2/L3 crates found in [workspace.metadata.diamond] — vector skipped"
  exit 1
fi

violations=0
violation_log=""

for crate in "${SCANNED_CRATES[@]}"; do
  src_dir="crates/$crate/src"
  [ -d "$src_dir" ] || continue
  # Scan every .rs file's PRODUCTION region only — the lines BEFORE the
  # first `#[cfg(test)]` / `mod tests` (Rust convention: tests live in a
  # trailing test module). A test naming a scratch dir by SystemTime or a
  # boundary test writing real files is not a runtime bypass.
  while IFS= read -r rs; do
    cut=$(grep -nE '#\[cfg\(test\)\]|^[[:space:]]*mod tests' "$rs" 2>/dev/null \
      | head -1 | cut -d: -f1)
    prod=$(if [ -n "$cut" ]; then head -n $((cut - 1)) "$rs"; else cat "$rs"; fi)
    for pat in "${FORBIDDEN_PATTERNS[@]}"; do
      # Match with line numbers, then drop: line comments (//), `use`
      # import lines (a TYPE import like `use std::net::SocketAddr` is not
      # a bypass — only the CALL site is), and any line carrying the
      # explicit `seam-bypass-ok:` allowlist marker.
      # `grep -n` on a stream yields "N:content" (no filename · anchor at ^).
      hits=$(printf '%s\n' "$prod" | grep -nE "$pat" 2>/dev/null \
        | grep -vE '^[0-9]+:[[:space:]]*//' \
        | grep -vE '^[0-9]+:[[:space:]]*use ' \
        | grep -v 'seam-bypass-ok:' \
        || true)
      if [ -n "$hits" ]; then
        violations=$((violations + 1))
        violation_log+="\n  $crate ($(basename "$rs")) — unmarked bypass '$pat':\n"
        violation_log+="$(echo "$hits" | head -3 | sed 's/^/    /')\n"
      fi
    done
  done < <(find "$src_dir" -name '*.rs' 2>/dev/null)
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d unmarked seam bypass(es) in L1.5+ crates:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: L1.5+ crates must reach the OS through the injected kernel"
  echo "seams (ClockDyn, FsDyn, HttpDyn — not std/dep defaults directly)."
  echo "If a bypass is genuinely intended (like nika:uuid's entropy), add"
  echo "a '// seam-bypass-ok: <reason>' marker on the line — a deliberate,"
  echo "reviewable sign-off. See the seam-contract finding class."
  exit 2
fi

echo "OK: no unmarked seam bypass in ${#SCANNED_CRATES[@]} L1.5+ crate(s)"
exit 0
