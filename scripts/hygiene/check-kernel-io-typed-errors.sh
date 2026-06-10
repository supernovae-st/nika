#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 40: kernel I/O effect-traits return a TYPED error, never std::io::Result.
#
# THE CONVENTION (FCI-023bis · canonical): every effect-trait in
# nika-kernel-core/src/io/*.rs returns `Result<T, XError>` with a
# #[non_exhaustive] thiserror enum + NikaErrorCode — NOT `std::io::Result`. A
# typed error carries the NIKA taxonomy across the trait boundary; io::Result
# erases it. This is the answer to "type it or not" — there is no other option.
#
# This gate makes the convention STRUCTURAL (ADR-090): it scans each io module
# for trait-method `std::io::Result` returns and:
#   - RED   if a module returns io::Result and is NOT a documented laggard
#           (a NEW un-typed io trait — the forcing function);
#   - YELLOW for the documented computer-use laggards still mid-migration
#           (scripts/ci/kernel-io-typed-error-baseline.txt) — a ratchet that
#           tightens as each migrates;
#   - GREEN  when every io module is typed (the baseline holds only comments).
#
# clock + mod are infallible/types-only (no error surface) — never flagged.
#
# Exit: 0 green · 1 yellow (laggards remain · informational ratchet) · 2 red
# (a NEW un-typed io trait — must be typed before it ships).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

IO_DIR="crates/nika-kernel-core/src/io"
BASELINE="scripts/ci/kernel-io-typed-error-baseline.txt"
[ -d "$IO_DIR" ] || {
  echo "kernel io dir missing: $IO_DIR"
  exit 2
}

# Documented laggards (comment-stripped basenames, e.g. "screen").
laggards=""
[ -f "$BASELINE" ] && laggards="$(grep -vE '^[[:space:]]*#' "$BASELINE" | grep -E '^[a-z]' | sort -u)"

is_laggard() { printf '%s\n' "$laggards" | grep -qx "$1"; }

red=""    # io modules returning io::Result that are NOT documented laggards
yellow="" # documented laggards still returning io::Result (tracked)
stale=""  # baseline lists a module that is ALREADY typed (remove the line)

for f in "$IO_DIR"/*.rs; do
  name="$(basename "$f" .rs)"
  # Trait-method io::Result returns (and the stream `Item = io::Result` form).
  n="$(grep -cE 'async fn .*-> std::io::Result|-> std::io::Result<|Item = std::io::Result' "$f")"
  if [ "$n" -gt 0 ]; then
    if is_laggard "$name"; then
      yellow="$yellow $name($n)"
    else
      red="$red $name($n)"
    fi
  elif is_laggard "$name"; then
    # Listed as a laggard but no longer returns io::Result → migrated.
    stale="$stale $name"
  fi
done

if [ -n "${red# }" ]; then
  echo "KERNEL-IO TYPED-ERROR FAILED · un-typed io trait(s) NOT in the migration baseline:"
  for m in $red; do
    echo "  RED: $IO_DIR/${m%%(*}.rs returns std::io::Result — type it (Pattern A · FsError template) or add to $BASELINE with a reason"
  done
  exit 2
fi

if [ -n "${stale# }" ]; then
  echo "BASELINE STALE · these modules are now typed — delete their line from $BASELINE:"
  for m in $stale; do echo "  · $m"; done
  exit 2
fi

typed_n="$(grep -lE 'pub enum [A-Z][A-Za-z]*Error' "$IO_DIR"/*.rs 2>/dev/null | wc -l | tr -d ' ')"
if [ -n "${yellow# }" ]; then
  echo "RATCHET (${typed_n} io module(s) typed) · computer-use laggards still on std::io::Result (migrate to Pattern A · FsError template):"
  for m in $yellow; do echo "  · $m"; done
  exit 1
fi

echo "OK (every kernel io effect-trait returns a typed error · Pattern A uniform · ${typed_n} typed modules)"
exit 0
