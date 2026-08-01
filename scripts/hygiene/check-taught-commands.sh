#!/usr/bin/env bash
# check-taught-commands — every `nika <verb>` a live surface teaches
# must be a verb the SAME binary parses (the dead-door class: the 0.107
# train renamed `examples` → `try` while the concierge kept teaching
# the ghost in five states — only a human paste could catch it; the
# in-crate ratchet covers welcome's start rows, THIS gate sweeps the
# whole rendered text of the teaching surfaces, `--deep` and the
# inline CTAs included).
#
# Zero-cost by design: renders three surfaces with the built binary,
# extracts the taught verbs, checks membership against the binary's
# own subcommand list. No workflow runs, nothing is written.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
bin="${NIKA_BIN:-$repo/target/debug/nika-cli}"
if [ ! -x "$bin" ]; then
  # The shared-target layout (CARGO_TARGET_DIR) or a release build.
  for candidate in "$repo/target/release/nika-cli" "${CARGO_TARGET_DIR:-}/debug/nika-cli" "${CARGO_TARGET_DIR:-}/release/nika-cli"; do
    [ -x "$candidate" ] && bin="$candidate" && break
  done
fi
if [ ! -x "$bin" ]; then
  echo "SKIP taught-commands: no built nika-cli (cargo build -p nika-cli first)"
  exit 0
fi

# The binary's own verb list — clap's help is the source of truth.
verbs="$("$bin" --help --all 2>/dev/null | awk '/^  [a-z]/ {print $1}' | sort -u)"
if [ -z "$verbs" ]; then
  verbs="$("$bin" --help 2>/dev/null | awk '/^  [a-z]/ {print $1}' | sort -u)"
fi

fail=0
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cd "$scratch" # an empty room: the surfaces render their stranger state

for surface in "--plain" "welcome --deep --plain" "doctor --plain"; do
  # The surface IS a word list — unquoted split is the point.
  # shellcheck disable=SC2086
  out="$("$bin" $surface 2>/dev/null || true)"
  # Taught forms: `nika <verb>` where <verb> is a bare word (flags,
  # paths and placeholders end the match).
  taught="$(printf '%s\n' "$out" | grep -oE 'nika [a-z][a-z-]*' | awk '{print $2}' | sort -u)"
  for verb in $taught; do
    if ! printf '%s\n' "$verbs" | grep -qx "$verb"; then
      echo "FAIL taught-commands: \`nika $surface\` teaches \`nika $verb\` — not a verb this binary parses"
      fail=1
    fi
  done
done

if [ "$fail" -eq 0 ]; then
  echo "OK  every taught command names a living verb (3 surfaces swept)"
fi
exit "$fail"
