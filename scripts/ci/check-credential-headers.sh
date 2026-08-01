#!/usr/bin/env bash
# check-credential-headers.sh — the list must not fall behind the wires.
#
# `nika_kernel::io::http::SENSITIVE_HEADERS` is the one answer to « does
# this header's value carry a credential? ». Two duties read it: values
# are redacted in Debug output, and the header is DROPPED when a redirect
# leaves the origin the caller addressed.
#
# It fell behind once (2026-08-02). The redirect list was copied from a
# general-purpose HTTP client that has never heard of `x-api-key` — the
# header our Anthropic wire sends, and the one `nika:fetch` documents to
# workflow authors — so a cross-origin 302 forwarded a live API key.
# Prose cannot prevent the next provider from adding its own header and
# reopening it; this gate can.
#
# The rule: every header literal our own sources INSERT whose name looks
# like an authentication credential must appear in the list.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LIST_FILE="$ROOT/crates/nika-kernel-core/src/io/http.rs"

# The list, read from the source of truth (never re-typed here — a second
# copy is the very bug this gate exists to prevent).
listed=$(
  awk '/pub const SENSITIVE_HEADERS/,/\];/' "$LIST_FILE" \
    | grep -oE '"[a-z0-9-]+"' | tr -d '"' | sort -u
)

if [ -z "$listed" ]; then
  echo "FAIL  could not read SENSITIVE_HEADERS from $LIST_FILE" >&2
  exit 1
fi

# Header names our sources insert. Only real senders — tests carry
# fixtures and docs carry examples, both of which name headers they never
# actually put on a wire.
sent=$(
  grep -rhoE '\.insert\(\s*"[a-zA-Z0-9-]+"' \
    "$ROOT"/crates/*/src --include='*.rs' 2>/dev/null \
    | grep -oE '"[a-zA-Z0-9-]+"' | tr -d '"' | tr '[:upper:]' '[:lower:]' | sort -u
)

# Which of those look like an authentication credential.
auth_shaped=$(printf '%s\n' "$sent" | grep -E 'api[-_]?key|^authorization$|auth[-_]?token|^cookie|token$|secret' || true)

missing=""
for h in $auth_shaped; do
  if ! printf '%s\n' "$listed" | grep -qx "$h"; then
    missing="${missing}${h}"$'\n'
  fi
done

if [ -n "$missing" ]; then
  printf 'FAIL  auth header(s) sent by this engine but absent from SENSITIVE_HEADERS:\n' >&2
  printf '%s' "$missing" | sed 's/^/  /' >&2
  printf '\n  A header this list does not name is NOT stripped on a cross-origin\n' >&2
  printf '  redirect and NOT redacted in Debug — add it to %s\n' \
    "crates/nika-kernel-core/src/io/http.rs" >&2
  exit 1
fi

count=$(printf '%s\n' "$listed" | wc -l | tr -d ' ')
seen=$(printf '%s\n' "$auth_shaped" | grep -c . || true)
printf 'OK  %s credential header(s) listed · %s auth header(s) sent, all covered\n' \
  "$count" "$seen"
