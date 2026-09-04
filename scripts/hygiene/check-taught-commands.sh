#!/usr/bin/env bash
# check-taught-commands — every `nika <verb>` a live surface teaches
# must be a verb the SAME binary actually accepts.
#
# The class: the 0.107 train renamed the showroom door (`examples` →
# `try`) and the concierge kept teaching the dead verb in five states —
# only a human paste could catch it. The in-crate ratchet replays
# welcome's start rows against the clap tree; THIS gate sweeps the
# whole rendered text of the teaching surfaces, `--deep` and the inline
# CTAs included.
#
# Truth by asking the binary, never by parsing its prose: a verb EXISTS
# iff `nika help <verb>` succeeds. An earlier draft scraped the verb
# list out of `--help --all` and swallowed the after-help map words
# (`begin` · `prove` · `learn`) AND the dead `examples` sitting in a
# teaching line — a reference list that validates the very ghost it
# hunts. Asking beats scraping.
#
# Runs nothing, writes nothing, needs no network.
set -uo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"

# The binary under test, most-specific first: an explicit NIKA_BIN, the
# session target dir (the house's concurrent-build discipline), then
# the repo's own target. A stale local target must never shadow the
# build the operator just made.
bin=""
for candidate in \
  "${NIKA_BIN:-}" \
  "${CARGO_TARGET_DIR:-}/debug/nika" \
  "${CARGO_TARGET_DIR:-}/release/nika" \
  "$repo/target/debug/nika" \
  "$repo/target/release/nika"; do
  if [ -n "$candidate" ] && [ -x "$candidate" ]; then
    bin="$candidate"
    break
  fi
done

if [ -z "$bin" ]; then
  # A gate that cannot judge says so VISIBLY — the same law the run
  # guard obeys (`guard_unavailable` denies rather than pretending).
  # YELLOW (rc=1), never a green skip: a green no-op reads exactly
  # like a pass, and this vector silently no-op'd in the nightly for
  # its whole first day because the runner never built a binary.
  echo "YELLOW taught-commands: no built nika-cli — nothing was swept (cargo build -p nika-cli)"
  exit 1
fi

# The teaching surfaces. `try` and `new` earn their place first: the
# dead-door class was BORN of the `examples` → `try` rename, and those
# two verbs teach the most commands per screen (the storefront's four
# doors · the clarify hint). A script is not a terminal, so `try`
# renders its full shelf here — the same wire contract the editor
# extension reads.
SURFACES=("--plain" "welcome --deep --plain" "doctor --plain" "try" "new")

fail=0
swept=0
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cd "$scratch" || exit 1 # an empty room: the surfaces render their stranger state

for surface in "${SURFACES[@]}"; do
  # The surface IS a word list — unquoted split is the point.
  # shellcheck disable=SC2086
  out="$("$bin" $surface 2>&1 || true)"
  [ -z "$out" ] && continue
  swept=$((swept + 1))
  # Taught forms: `nika <verb>` where <verb> is a bare word (flags,
  # paths and placeholders end the match). A grep that matches nothing
  # exits 1 — `|| true` keeps that from killing the sweep.
  taught="$(printf '%s\n' "$out" | grep -oE 'nika [a-z][a-z-]+' | awk '{print $2}' | sort -u || true)"
  for verb in $taught; do
    # `nika help <verb>` is the binary's own answer. `help` itself and
    # a bare `nika` are not verbs to check.
    [ "$verb" = "help" ] && continue
    if ! "$bin" help "$verb" >/dev/null 2>&1; then
      echo "FAIL taught-commands: \`nika $surface\` teaches \`nika $verb\` — this binary does not accept it"
      fail=1
    fi
  done
done

if [ "$swept" -eq 0 ]; then
  echo "YELLOW taught-commands: every surface rendered empty — nothing was swept"
  exit 1
fi
if [ "$fail" -eq 0 ]; then
  echo "OK  every taught command names a living verb ($swept surfaces swept)"
  exit 0
fi
# RED (rc=2), not yellow: a dead door taught at first contact is the
# class this vector exists for, and the pre-push gate only blocks on
# RED. A finding that cannot stop a push is a finding that ships.
exit 2
